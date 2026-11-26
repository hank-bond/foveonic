//! Executes typed graph operations with selective outputs and guarded GPU storage.

use super::Engine;
use crate::nodes::{
    color::{pattern, reconstruction},
    sensor::{banding, camera_repair, color_field, despeckle, highlight},
};
use crate::{
    gpu::{
        scratch::Scratch,
        Binding::{Read, Write},
        Kernel,
    },
    source::Metadata,
};
use crate::{
    graph::{DataType, Graph, Operation},
    image::{Description, Layout, RenderedImage},
    interactive::Execution,
    profile::Profile,
    source::{Result, Source},
};
use std::time::Instant;

fn resource_observation(value: &Value) -> crate::profile::ResourceObservation {
    let (allocation_id, generation) = value.buffer.identity();
    crate::profile::ResourceObservation {
        allocation_id,
        generation,
        byte_len: value.buffer.size(),
        working_scale: value.working_scale.divisor(),
        description: value.description.map(|d| crate::profile::ImageDescription {
            extent: d.extent,
            layout: format!("{:?}", d.layout),
            space: format!("{:?}", d.space),
            view_baked: d.view_baked,
        }),
        view: value.view.map(|v| crate::profile::ImageView {
            crop: v.crop,
            rotation: v.rotation,
            extent: v.extent,
        }),
    }
}

fn boundary_memory(
    values: &[Option<Value>],
    scratch: &Scratch,
    execution: &Execution<'_>,
) -> crate::profile::BoundaryMemory {
    fn total<'a>(buffers: impl Iterator<Item = &'a crate::gpu::ReadGuard>) -> u64 {
        let mut seen = std::collections::HashSet::new();
        buffers
            .filter_map(|buffer| seen.insert(buffer.identity().0).then_some(buffer.size()))
            .sum()
    }
    let graph = || values.iter().flatten().map(|value| &value.buffer);
    let cache = || {
        execution
            .cache
            .as_deref()
            .into_iter()
            .flat_map(|cache| cache.buffers())
    };
    crate::profile::BoundaryMemory {
        graph_bytes: total(graph()),
        scratch_bytes: total(scratch.buffers()),
        cache_bytes: total(cache()),
        union_bytes: total(graph().chain(scratch.buffers()).chain(cache())),
    }
}

#[allow(clippy::too_many_arguments)]
fn trace_graph_outputs(
    graph_id: Option<u64>,
    graph: &Graph,
    ids: &[usize],
    values: &[Option<Value>],
    cached: &[bool],
    reachable: Option<&[bool]>,
    execution: &Execution<'_>,
) {
    if !crate::diagnostics::enabled() {
        return;
    }
    for &id in ids {
        crate::diagnostics::event(
            crate::diagnostics::Category::Graph,
            "graph.output",
            |fields| {
                if let Some(graph_id) = graph_id {
                    fields.u64("graph_id", graph_id);
                }
                fields
                    .u64("output_id", id as u64)
                    .str("name", &graph.outputs[id].cache_name())
                    .str("data_type", &format!("{:?}", graph.outputs[id].data))
                    .bool("seeded", cached[id])
                    .bool("present", values[id].is_some());
                fields
                    .u64("producer", graph.outputs[id].producer as u64)
                    .str(
                        "dependencies",
                        &serde_json::to_string(&graph.outputs[id].dependencies)
                            .expect("graph output dependencies"),
                    );
                if let Some(reachable) = reachable {
                    fields
                        .bool("required", reachable[id])
                        .bool("requested_for_execution", reachable[id] && !cached[id])
                        .bool("published", !cached[id] && values[id].is_some());
                }
                if let Some(cache) = execution.cache.as_deref() {
                    fields.u64("cache_id", cache.diagnostic_id());
                    if let Some(version) = cache.diagnostic_version(&graph.outputs[id].cache_name())
                    {
                        fields.u64("cache_version", version);
                    }
                }
                if let Some(value) = &values[id] {
                    let (allocation_id, generation) = value.buffer.identity();
                    fields
                        .u64("allocation_id", allocation_id)
                        .u64("allocation_generation", generation)
                        .u64("byte_len", value.buffer.size())
                        .u64("working_scale", u64::from(value.working_scale.divisor()));
                }
            },
        );
    }
}

// Bound completed spare retention separately from live graph and cache ownership.
const COMPLETED_SCRATCH_SPARE_BYTES: u64 = 128 * 1024 * 1024;
pub(crate) const TICK_WAIT_STAGE: &str = "complete graph tick";

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScalarParams {
    source: [u32; 4],
    value: [f32; 4],
}

/// Tone may reuse scratch, but admission reserves a fresh RGB output.
pub(super) fn tone_allocation(
    extent: [u32; 2],
    curve_len: usize,
) -> Option<super::allocation::GpuAllocation> {
    use super::allocation::{image_bytes, uploaded_bytes, GpuAllocation};
    GpuAllocation::new(
        image_bytes(extent, 3)?,
        uploaded_bytes::<f32>(curve_len)?.checked_add(uploaded_bytes::<ScalarParams>(1)?)?,
    )
}

#[derive(Clone, Copy)]
pub(super) enum Input<'a> {
    Source(&'a Source),
    Metadata(&'a std::sync::Arc<Metadata>),
    Rendered(&'a RenderedImage),
}

impl Input<'_> {
    /// Storage extent of the graph's root input.
    pub(super) fn extent(&self) -> [u32; 2] {
        match self {
            Input::Source(source) => source.storage_extent().map(|v| v as u32),
            Input::Metadata(metadata) => [metadata.width as u32, metadata.height as u32],
            Input::Rendered(image) => image.description.extent,
        }
    }
}

impl Input<'_> {
    pub(super) fn metadata(&self) -> &std::sync::Arc<Metadata> {
        match self {
            Self::Source(source) => &source.metadata,
            Self::Metadata(metadata) => metadata,
            Self::Rendered(image) => &image.metadata,
        }
    }
}

#[derive(Debug)]
pub(crate) struct SourceRequired;
impl std::fmt::Display for SourceRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a required camera dependency needs original sensor samples")
    }
}
impl std::error::Error for SourceRequired {}

fn check_residual_extents(
    params: &crate::nodes::color::residual_finish::Params,
    inherited: [u32; 2],
    gain: &Value,
) -> Result<()> {
    if inherited != params.extent() || gain.description.map(|d| d.extent) != Some(inherited) {
        return Err("residual finishing requires matching RGB and gain extents".into());
    }
    Ok(())
}

/// The graph's last reader takes the value, so its node can reuse the buffer
/// when no cache or inspector also holds it. Other readers share it.
fn last_read_or_share(
    values: &mut [Option<Value>],
    consumers: &[usize],
    id: usize,
) -> crate::gpu::ReadGuard {
    if consumers[id] == 1 {
        values[id].take().expect("validated input is live").buffer
    } else {
        values[id]
            .as_ref()
            .expect("validated input is live")
            .buffer
            .clone()
    }
}

#[derive(Clone)]
pub(crate) struct Value {
    pub(crate) working_scale: super::Scale,
    pub(crate) view: Option<crate::image::View>,
    // Framing also crosses opaque numerical guides. Image descriptions snapshot it.
    pub(crate) view_baked: bool,
    pub(crate) buffer: crate::gpu::ReadGuard,
    pub(crate) description: Option<Description>,
}

fn operation_extent(operation: &Operation, inherited: [u32; 2]) -> [u32; 2] {
    match operation {
        Operation::HighlightEvidence(prepared) => prepared.grid().output_extent(),
        Operation::PoolSignal(prepared) => prepared.output_extent(),
        Operation::ReduceLayers { prepared, .. } => prepared.output_extent(),
        Operation::ResampleRgb { params, .. } => [params.output[0], params.output[1]],
        Operation::ToneMask { guide, .. }
        | Operation::AdaptiveToneGuide { guide, .. }
        | Operation::ExpandBrightnessGuide { guide, .. } => guide.extent(),
        Operation::ImageRegion { prepared, .. } => prepared.extent(),
        Operation::AdaptiveTone { window, .. } | Operation::TonalRanges { window, .. }
            if window.output.is_some() =>
        {
            let bounds = window.output.expect("regional output bounds");
            [bounds[2] - bounds[0], bounds[3] - bounds[1]]
        }
        Operation::ProjectColorTiles { params, .. } => params.output_extent(),
        Operation::NoiseContext(p) => reconstruction::context_extent(*p),
        Operation::Banding(p) => {
            let [l, t, r, b] = p.output_bounds();
            [r - l, b - t]
        }
        Operation::PackedLab(p) => p.extent(),
        Operation::CodeCrop(p) => p.extent(),
        Operation::WaveletContrast(p) => p.extent(),
        Operation::CodeChannelMap(p) => p.extent(),
        Operation::ChannelNoise(p) => p.extent(),
        Operation::Multiscale(p) => p.extent(),
        Operation::ShieldNoise(p) => p.extent(),
        Operation::RowGuard(p) => p.extent(),
        Operation::ColorSharpen(p) => p.extent(),
        Operation::ProportionalSharpen(p) => p.extent(),
        Operation::Encode { params, .. } => [params.output[0], params.output[1]],
        Operation::BlueGuidedReconstruct(p) => p.output_extent(),
        _ => inherited,
    }
}

pub(super) fn report_failure(
    error: Box<dyn std::error::Error + Send + Sync>,
    phase: &str,
    metadata: &Metadata,
    details: serde_json::Value,
) -> Box<dyn std::error::Error + Send + Sync> {
    // These typed results drive scheduling and must remain directly downcastable.
    if error.is::<SourceRequired>()
        || error.is::<crate::interactive::Superseded>()
        || error.is::<crate::priority::Interrupted>()
    {
        return error;
    }
    crate::support::reported_error(
        "render.core",
        error,
        crate::support::OperationContext {
            component: Some("renderer".into()),
            phase: Some(phase.into()),
            settings: Some(
                serde_json::json!({"camera":metadata.properties.get("CAMMODEL"),
            "extent":[metadata.width,metadata.height],"operation":details}),
            ),
            ..Default::default()
        },
    )
}

pub(super) fn report_graph_failure(
    error: Box<dyn std::error::Error + Send + Sync>,
    metadata: &Metadata,
    graph: &Graph,
    targets: &[usize],
) -> Box<dyn std::error::Error + Send + Sync> {
    report_failure(
        error,
        "execute-graph",
        metadata,
        serde_json::json!({
            "target_ids": targets,
            "target_names": targets.iter().map(|&id| graph.outputs.get(id).map(|output| output.name)).collect::<Vec<_>>(),
            "node_count": graph.nodes.len(),
        }),
    )
}

/// Walks one graph's uncached nodes and owns live storage between steps. Each step
/// borrows the same graph, input, execution cache and profile. An error is terminal.
pub(super) struct Traversal {
    next_node: usize,
    graph_id: Option<u64>,
    trace: crate::gpu::GpuTrace,
    scratch: Scratch,
    values: Vec<Option<Value>>,
    cached: Vec<bool>,
    required: Vec<bool>,
    reachable: Option<Vec<bool>>,
    reads: Vec<Vec<usize>>,
    consumers: Vec<usize>,
    stage_total: usize,
    stage_completed: usize,
    pending_stages: usize,
    /// Profile index of the node observation from the latest step, completed at its tick.
    pending_observation: Option<usize>,
    shared_ranges: Option<Vec<(usize, usize, usize)>>,
}

impl Engine {
    pub(super) fn start_graph(
        &self,
        context: Input<'_>,
        graph: &Graph,
        targets: &[usize],
        profile: &mut Profile,
        execution: &mut Execution<'_>,
        graph_id: Option<u64>,
    ) -> Result<Traversal> {
        let preparation =
            (profile.mode.enabled() || crate::diagnostics::enabled()).then(Instant::now);
        let gpu = &self.gpu;
        let trace = gpu.trace(profile.mode.gpu(), graph.nodes.len() as u32 * 2);
        let scratch = Scratch::default();
        let values: Vec<Option<Value>> = graph
            .outputs
            .iter()
            .map(|output| execution.seed(&output.cache_name()))
            .collect();
        let cached: Vec<bool> = values.iter().map(Option::is_some).collect();
        if profile.mode.enabled() {
            profile.graphs.push(crate::profile::GraphObservation {
                id: graph_id.expect("enabled graph has an identity"),
                requested_roots: targets.to_vec(),
                boundary_memory_scope: "Logical unique buffer bytes at node boundaries; excludes node-internal temporaries, driver and frontend allocations, and decoded CPU memory",
                outputs: graph
                    .outputs
                    .iter()
                    .enumerate()
                    .map(|(id, output)| crate::profile::GraphOutput {
                        id,
                        name: output.cache_name(),
                        producer: output.producer,
                        dependencies: output.dependencies.clone(),
                        data_type: format!("{:?}", output.data),
                        session_hint: graph.session_outputs.contains(&id),
                        sensor_data: graph.sensor_data_outputs.contains(&id),
                    })
                    .collect(),
            });
        }
        let required = if cached.iter().any(|cached| *cached) {
            graph.required_until(targets, &cached)?
        } else {
            graph.required(targets)?
        };
        if let (Some(graph_id), Some(cache)) = (graph_id, execution.cache.as_deref()) {
            cache.trace_checkpoints(graph_id);
        }
        if matches!(context, Input::Metadata(_))
            && graph.outputs.iter().enumerate().any(|(id, output)| {
                required[id]
                    && !cached[id]
                    && matches!(
                        graph.nodes[output.producer].operation,
                        Operation::Source | Operation::SourcePhaseCells
                    )
            })
        {
            return Err(Box::new(SourceRequired));
        }
        // A cached main output does not guarantee that a side output is cached.
        // Request only what is missing so a partial run cannot replace retained values.
        let reachable =
            (profile.mode.enabled() || crate::diagnostics::enabled()).then(|| required.clone());
        let required: Vec<bool> = required
            .iter()
            .zip(&cached)
            .map(|(needed, cached)| *needed && !*cached)
            .collect();
        let mut reads = vec![Vec::new(); graph.nodes.len()];
        for (id, output) in graph.outputs.iter().enumerate() {
            if required[id] && !cached[id] {
                for &input in &output.dependencies {
                    if !reads[output.producer].contains(&input) {
                        reads[output.producer].push(input);
                    }
                }
            }
        }
        let mut consumers = vec![0; graph.outputs.len()];
        for inputs in &reads {
            for &input in inputs {
                consumers[input] += 1;
            }
        }
        for &target in targets {
            consumers[target] += 1;
        }
        let stage_total = graph
            .nodes
            .iter()
            .filter(|node| node.outputs.iter().any(|id| required[*id] && !cached[*id]))
            .count();
        let stage_completed = 0;
        if stage_total == 0 {
            if let Some(progress) = execution.progress {
                progress(super::RenderProgress::Stage {
                    completed: 0,
                    total: 0,
                });
            }
        }
        if let Some(start) = preparation {
            profile.record("prepare graph execution", start);
        }
        let shared_ranges = profile.mode.gpu().then(Vec::new);
        Ok(Traversal {
            next_node: 0,
            graph_id,
            trace,
            scratch,
            values,
            cached,
            required,
            reachable,
            reads,
            consumers,
            stage_total,
            stage_completed,
            pending_stages: 0,
            pending_observation: None,
            shared_ranges,
        })
    }
}

impl Traversal {
    pub(super) fn graph_id(&self) -> Option<u64> {
        self.graph_id
    }

    pub(super) fn next_node(&self) -> usize {
        self.next_node
    }

    /// Estimated GPU allocations for every remaining node, audited where a contract
    /// exists and generic otherwise, with the bytes each node releases once it has
    /// read its inputs for the last time. Profiling and diagnostics do not change it.
    pub(super) fn allocation_plan(
        &self,
        graph: &Graph,
        context: Input<'_>,
    ) -> Option<Vec<super::allocation::NodeCost>> {
        let mut extents: Vec<_> = self
            .values
            .iter()
            .map(|value| {
                value
                    .as_ref()
                    .and_then(|value| value.description.map(|description| description.extent))
            })
            .collect();
        let mut sizes: Vec<u64> = self
            .values
            .iter()
            .map(|value| value.as_ref().map_or(0, |value| value.buffer.size()))
            .collect();
        let mut consumers = self.consumers.clone();
        let mut costs = vec![super::allocation::NodeCost::default(); graph.nodes.len()];
        for (index, node) in graph.nodes.iter().enumerate().skip(self.next_node) {
            let outputs: Vec<_> = node.outputs.iter().map(|id| self.required[*id]).collect();
            if !outputs.iter().any(|required| *required) {
                continue;
            }
            let input_extent = self.reads[index]
                .iter()
                .find_map(|id| extents[*id])
                .unwrap_or_else(|| context.extent());
            let extent = operation_extent(&node.operation, input_extent);
            let mut output_bytes = 0_u64;
            for (&id, required) in node.outputs.iter().zip(&outputs) {
                if *required {
                    let description = graph.outputs[id].data.image_description(extent);
                    extents[id] =
                        Some(description.map_or(extent, |description| description.extent));
                    sizes[id] = description.map_or_else(
                        || super::allocation::undescribed_bytes(extent),
                        |description| Some(description.byte_len()),
                    )?;
                    output_bytes = output_bytes.checked_add(sizes[id])?;
                }
            }
            let allocation =
                match super::allocation::estimate(&node.operation, &outputs, Some(input_extent)) {
                    Some(audited) => audited,
                    None => super::allocation::generic(output_bytes)?,
                };
            let mut released_bytes = 0_u64;
            for &id in &self.reads[index] {
                consumers[id] -= 1;
                if consumers[id] == 0 && !graph.session_outputs.contains(&id) {
                    released_bytes = released_bytes.checked_add(sizes[id])?;
                }
            }
            costs[index] = super::allocation::NodeCost {
                allocation,
                released_bytes,
            };
        }
        Some(costs)
    }

    pub(super) fn account(&self, retained: &mut crate::recipe::RetainedBytes) -> Option<()> {
        retained.add_bytes(self.values.capacity() * size_of::<Option<Value>>());
        retained.add_bytes(self.cached.capacity() * size_of::<bool>());
        retained.add_bytes(self.required.capacity() * size_of::<bool>());
        if let Some(reachable) = &self.reachable {
            retained.add_bytes(reachable.capacity() * size_of::<bool>());
        }
        retained.add_bytes(self.reads.capacity() * size_of::<Vec<usize>>());
        for reads in &self.reads {
            retained.add_bytes(reads.capacity() * size_of::<usize>());
        }
        retained.add_bytes(self.consumers.capacity() * size_of::<usize>());
        if let Some(ranges) = &self.shared_ranges {
            retained.add_bytes(ranges.capacity() * size_of::<(usize, usize, usize)>());
        }
        self.scratch.account(retained);
        self.trace.account(retained)
    }

    pub(super) fn buffers(&self) -> impl Iterator<Item = &crate::gpu::ReadGuard> {
        self.values
            .iter()
            .flatten()
            .map(|value| &value.buffer)
            .chain(self.scratch.buffers())
    }

    /// Run skipped nodes and at most one required node in the current GPU tick.
    /// Trailing skipped nodes are consumed too, so the last step reports completion.
    /// The caller completes the step's tick before the next step.
    pub(super) fn step(
        &mut self,
        engine: &Engine,
        context: Input<'_>,
        graph: &Graph,
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<bool> {
        let gpu = &engine.gpu;
        let Self {
            next_node,
            graph_id,
            trace,
            scratch,
            values,
            cached,
            required,
            reachable,
            reads,
            consumers,
            stage_total,
            stage_completed,
            pending_stages,
            pending_observation,
            shared_ranges,
        } = self;
        let graph_id = *graph_id;
        while *next_node < graph.nodes.len() {
            let node_index = *next_node;
            *next_node += 1;
            let node = &graph.nodes[node_index];
            execution.check()?;
            let _node_context = crate::diagnostics::current_context().map(|mut context| {
                context.node = Some(node_index as u64);
                crate::diagnostics::enter(context)
            });
            let run = node.outputs.iter().any(|id| required[*id] && !cached[*id]);
            let _node_scope = crate::diagnostics::scope_with(
                crate::diagnostics::Category::Graph,
                node.operation.label(),
                |fields| {
                    fields.u64("node", node_index as u64);
                    if let Some(id) = graph_id {
                        fields.u64("graph_id", id);
                    }
                    fields.str("operation", node.operation.kind());
                    fields.bool("executed", run);
                    fields.u64("outputs", node.outputs.len() as u64);
                    fields.u64(
                        "seed_hits",
                        node.outputs.iter().filter(|id| cached[**id]).count() as u64,
                    );
                    for (slot, id) in node.inputs.iter().enumerate() {
                        fields.u64(&format!("input_{slot}"), *id as u64);
                    }
                    for (slot, id) in node.outputs.iter().enumerate() {
                        fields.u64(&format!("output_{slot}"), *id as u64);
                    }
                },
            );
            let diagnostic = if profile.mode.enabled() {
                let reachable = reachable.as_ref().expect("profiled output reachability");
                let diagnostic_started = Instant::now();
                let mut observation = crate::profile::NodeObservation {
                    graph_id: graph_id.expect("profiled graph has an identity"),
                    ordinal: node_index,
                    operation: node.operation.kind(),
                    outputs: node
                        .outputs
                        .iter()
                        .map(|&id| crate::profile::NodeOutput {
                            id,
                            name: graph.outputs[id].cache_name(),
                            required: reachable[id],
                            cached_available: cached[id],
                            requested_for_execution: run && required[id],
                            published: false,
                            resource: values[id].as_ref().map(resource_observation),
                        })
                        .collect(),
                    executed: run,
                    host_milliseconds: None,
                    started_ms: 0.,
                    publication_ms: None,
                    completion_wait_ms: None,
                    retirement_ms: None,
                    total_ms: None,
                    activity: None,
                    nested_gpu_start: profile.gpu.len(),
                    nested_gpu_end: profile.gpu.len(),
                    shared_gpu_start: None,
                    shared_gpu_end: None,
                    memory_before: boundary_memory(values, scratch, execution),
                    memory_after_publication: Default::default(),
                    memory_after_retirement: Default::default(),
                    diagnostic_overhead_ms: 0.,
                };
                observation.diagnostic_overhead_ms =
                    diagnostic_started.elapsed().as_secs_f64() * 1000.;
                observation.started_ms = profile.elapsed_ms();
                Some((
                    observation,
                    Instant::now(),
                    gpu.activity(),
                    profile.mode.gpu().then(|| trace.len()),
                ))
            } else {
                None
            };
            if !run {
                trace_graph_outputs(
                    graph_id,
                    graph,
                    &node.outputs,
                    values,
                    cached,
                    reachable.as_deref(),
                    execution,
                );
                if let Some((mut observation, _, _, _)) = diagnostic {
                    observation.memory_after_publication = observation.memory_before;
                    observation.memory_after_retirement = observation.memory_before;
                    profile.nodes.push(observation);
                }
                continue;
            }
            let index = node.outputs[0];
            let mut submission_stage = gpu.background_stage();
            let start = Instant::now();
            let input = |slot: usize| {
                values[node.inputs[slot]].as_ref().unwrap_or_else(|| {
                    panic!(
                        "validated input {} for {} is not live",
                        graph.outputs[node.inputs[slot]].name,
                        node.operation.label()
                    )
                })
            };
            let inherited = reads[node_index]
                .iter()
                .find_map(|id| {
                    values[*id]
                        .as_ref()
                        .and_then(|value| value.description)
                        .map(|d| d.extent)
                })
                .unwrap_or_else(|| context.extent());
            let inherited_scale = reads[node_index]
                .iter()
                .find_map(|id| values[*id].as_ref().map(|value| value.working_scale))
                .unwrap_or(super::Scale::NATIVE);
            let coarser = || -> Result<super::Scale> {
                super::Scale::new(
                    inherited_scale
                        .divisor()
                        .checked_mul(2)
                        .ok_or("working scale overflow")?,
                )
            };
            let working_scale = match &node.operation {
                Operation::HighlightEvidence(prepared) => {
                    prepared.grid().working_scale().map_or_else(coarser, Ok)?
                }
                Operation::Source | Operation::SourcePhaseCells => super::Scale::NATIVE,
                Operation::RenderedInput { .. } => match &context {
                    Input::Rendered(image) => image.settings.working_scale,
                    _ => super::Scale::NATIVE,
                },
                Operation::ReduceLayers { prepared, .. } => {
                    prepared.working_scale().map_or_else(coarser, Ok)?
                }
                Operation::PoolSignal(prepared) => {
                    prepared.working_scale().map_or_else(coarser, Ok)?
                }
                _ => inherited_scale,
            };
            let extent = operation_extent(&node.operation, inherited);
            let inherited_view = reads[node_index]
                .iter()
                .find_map(|id| values[*id].as_ref().and_then(|value| value.view));
            crate::diagnostics::event(
                crate::diagnostics::Category::Graph,
                "graph.geometry",
                |fields| {
                    fields.u64("node", node_index as u64);
                    fields.str("operation", node.operation.kind());
                    fields.u64("scale", working_scale.divisor() as u64);
                    fields.u64("width", extent[0] as u64);
                    fields.u64("height", extent[1] as u64);
                    fields.u64("input_scale", inherited_scale.divisor() as u64);
                    fields.u64("input_width", inherited[0] as u64);
                    fields.u64("input_height", inherited[1] as u64);
                },
            );
            let view = match &node.operation {
                Operation::Banding(p)
                    if p.output_bounds() != [0, 0, inherited[0], inherited[1]] =>
                {
                    None
                }
                Operation::AdaptiveTone { window, .. } | Operation::TonalRanges { window, .. } => {
                    if let Some(prepared) = window.crop(inherited)? {
                        inherited_view.map(|view| {
                            crate::nodes::finish::image_region::map_view(
                                view,
                                prepared
                                    .input_bounds(None)
                                    .expect("validated regional crop"),
                            )
                        })
                    } else {
                        inherited_view
                    }
                }
                Operation::HighlightEvidence(prepared) => {
                    inherited_view.map(|view| prepared.grid().map_view(view))
                }
                Operation::ReduceLayers { prepared, .. } => {
                    inherited_view.map(|view| prepared.map_view(view))
                }
                Operation::PoolSignal(prepared) => {
                    inherited_view.map(|view| prepared.map_view(view))
                }
                Operation::CodeCrop(_) | Operation::Encode { .. } | Operation::NoiseContext(_) => {
                    Some(crate::image::View {
                        crop: [0, 0, extent[0], extent[1]],
                        rotation: 0,
                        extent,
                    })
                }
                Operation::RenderedInput { .. } => match &context {
                    Input::Rendered(image) => Some(image.uncropped_view()),
                    _ => None,
                },
                _ => inherited_view,
            };
            let view_baked = matches!(
                node.operation,
                Operation::CodeCrop(_) | Operation::Encode { .. }
            ) || reads[node_index]
                .iter()
                .any(|id| values[*id].as_ref().is_some_and(|v| v.view_baked))
                || matches!((&node.operation, &context), (Operation::RenderedInput { .. }, Input::Rendered(image)) if image.description.view_baked);
            let desc = graph.outputs[index]
                .data
                .image_description(extent)
                .map(|mut d| {
                    d.view_baked = view_baked;
                    if matches!(d.space, crate::image::Space::LinearRgb(_)) {
                        d.linear_white = match node.operation {
                            Operation::LinearCodeRgb { white, .. } => white,
                            Operation::RenderedInput { .. } => match &context {
                                Input::Rendered(image) => image.description.linear_white,
                                _ => 4095,
                            },
                            _ => reads[node_index]
                                .first()
                                .and_then(|id| values[*id].as_ref())
                                .and_then(|v| v.description)
                                .map_or(4095, |d| d.linear_white),
                        };
                    }
                    d
                });
            let groups = [inherited[0].div_ceil(8), inherited[1].div_ceil(8)];
            let dimensions = [inherited[0], inherited[1], 0, 0];
            let bytes = u64::from(inherited[0]) * u64::from(inherited[1]) * 12;
            let mut dispatch = |kernel, buffers: &mut [(u32, crate::gpu::Binding<'_>)], groups| {
                gpu.dispatch(kernel, buffers, groups, trace)
            };
            let output = match &node.operation {
                Operation::CodeCrop(prepared) => {
                    if inherited != prepared.input_extent() {
                        return Err("code crop input extent mismatch".into());
                    }
                    Some(crate::nodes::finish::code_crop::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::ResampleRgb { params, .. } => Some(
                    crate::nodes::finish::resize::apply(gpu, &input(0).buffer, params, profile)?.0,
                ),
                Operation::HighlightEvidence(prepared) => {
                    Some(prepared.apply(gpu, &input(0).buffer, profile)?)
                }
                Operation::PoolSignal(prepared) => Some(crate::nodes::sensor::pool::apply(
                    gpu,
                    &input(0).buffer,
                    prepared,
                    profile,
                )?),
                Operation::ReduceLayers { prepared, .. } => Some(
                    crate::nodes::sensor::pool::apply(gpu, &input(0).buffer, prepared, profile)?,
                ),
                Operation::ImageRegion { prepared, .. } => Some(
                    crate::nodes::finish::image_region::apply(gpu, &input(0).buffer, prepared)?,
                ),
                Operation::EmptyDefectFlags([width, height]) => {
                    if *width == 0 || *height == 0 {
                        return Err("empty defect flags require nonzero geometry".into());
                    }
                    // Fresh WebGPU buffers are zero initialized. These flags
                    // belong to factory repair, independently of sparse-grid flags.
                    Some(gpu.storage(
                        "empty factory defect flags",
                        u64::from(*width) * u64::from(*height) * 4,
                    )?)
                }
                Operation::Source | Operation::SourcePhaseCells => {
                    let Input::Source(source) = &context else {
                        return Err("sensor input context required".into());
                    };
                    let expected = if matches!(node.operation, Operation::SourcePhaseCells) {
                        crate::source::SourceStorage::PackedRgbBluePhases
                    } else {
                        crate::source::SourceStorage::InterleavedRgb
                    };
                    if source.storage() != expected {
                        return Err("source storage does not match graph input".into());
                    }
                    Some(gpu.upload_sensor("Original sensor layers", &source.pixels)?)
                }
                Operation::BlueRefresh(prepared) => {
                    if inherited != prepared.extent()
                        || input(1).description.map(|d| d.extent) != Some(inherited.map(|v| v * 2))
                    {
                        return Err(
                            "blue refresh requires RGB cells and a doubled blue plane".into()
                        );
                    }
                    Some(crate::nodes::sensor::blue_refresh::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::OrderedBlueRepair(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("ordered blue repair extent does not match its input".into());
                    }
                    let prior = if prepared.stage()
                        == crate::nodes::sensor::ordered_blue_repair::Stage::General
                    {
                        Some(&input(1).buffer)
                    } else {
                        None
                    };
                    let output = crate::nodes::sensor::ordered_blue_repair::apply(
                        gpu,
                        &input(0).buffer,
                        prior,
                        prepared,
                        profile,
                    )?;
                    if let Some(&side) = node.outputs.get(1) {
                        if required[side] {
                            values[side] = Some(Value {
                                working_scale,
                                view_baked,
                                view,
                                buffer: output.mask,
                                description: None,
                            });
                        }
                    }
                    required[index].then_some(output.blue)
                }
                Operation::BlueDespeckle(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("blue despeckle extent does not match its input".into());
                    }
                    Some(crate::nodes::sensor::blue_despeckle::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::PackedLab(prepared) => {
                    let source = last_read_or_share(values, consumers, node.inputs[0]);
                    Some(crate::nodes::color::packed_lab::apply(
                        gpu, source, prepared, profile,
                    )?)
                }
                Operation::WaveletContrast(prepared) => {
                    Some(crate::nodes::finish::wavelet_contrast::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::CodeChannelMap(p) => {
                    Some(crate::nodes::finish::code_channel_map::apply(
                        gpu,
                        &input(0).buffer,
                        p,
                        profile,
                    )?)
                }
                Operation::ColorSharpen(params) => {
                    Some(crate::nodes::finish::color_sharpen::apply(
                        gpu,
                        &input(0).buffer,
                        params,
                        profile,
                    )?)
                }
                Operation::ProportionalSharpen(prepared) => {
                    Some(crate::nodes::finish::proportional_sharpen::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::BlueMismatch(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("blue mismatch extent does not match its input".into());
                    }
                    Some(crate::nodes::sensor::blue_mismatch::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::CellGain(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("phase-cell gain extent mismatch".into());
                    }
                    Some(crate::nodes::color::cell_gain::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::SpatialColorGain { prepared, .. } => {
                    if inherited != prepared.extent() {
                        return Err("spatial color-gain extent mismatch".into());
                    }
                    Some(crate::nodes::color::spatial_color_gain::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::BoxContrast(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("box contrast extent mismatch".into());
                    }
                    Some(crate::nodes::finish::box_contrast::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::ColumnGain(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("column-gain extent mismatch".into());
                    }
                    Some(crate::nodes::sensor::column_gain::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::BluePattern(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("blue-pattern extent mismatch".into());
                    }
                    Some(crate::nodes::sensor::blue_pattern::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::CalibratedDefects(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("calibrated defect extent mismatch".into());
                    }
                    Some(crate::nodes::sensor::calibrated_defects::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                    )?)
                }
                Operation::Multiscale(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("multiscale sensor filter extent mismatch".into());
                    }
                    use crate::nodes::sensor::multiscale::Evidence;
                    let flags = prepared
                        .grid()
                        .map(|grid| engine.clipping_flags(context.metadata(), Some(grid)))
                        .transpose()?;
                    let clipping = flags
                        .as_ref()
                        .map(|flags| gpu.upload("Multiscale clipping evidence", flags))
                        .transpose()?;
                    let evidence = if let Some(clipping) = &clipping {
                        Evidence::Calibrated { clipping }
                    } else {
                        if input(1).description.map(|d| d.extent) != Some(inherited) {
                            return Err("multiscale original extent mismatch".into());
                        }
                        Evidence::Native {
                            original: &input(1).buffer,
                            black: &input(2).buffer,
                        }
                    };
                    Some(crate::nodes::sensor::multiscale::apply(
                        gpu,
                        &input(0).buffer,
                        evidence,
                        prepared,
                        profile,
                    )?)
                }
                Operation::RowGuard(prepared) => {
                    if inherited != prepared.extent()
                        || input(1).description.map(|d| d.extent) != Some(inherited)
                    {
                        return Err("guarded row extent mismatch".into());
                    }
                    use crate::nodes::sensor::row_guard::Inputs;
                    let flags = prepared
                        .grid()
                        .map(|grid| engine.clipping_flags(context.metadata(), Some(grid)))
                        .transpose()?;
                    let clipping = flags
                        .as_ref()
                        .map(|flags| gpu.upload("Row guard clipping evidence", flags))
                        .transpose()?;
                    let inputs = if let Some(clipping) = &clipping {
                        if input(2).description.map(|d| d.extent) != Some(inherited) {
                            return Err("guarded row shield evidence extent mismatch".into());
                        }
                        Inputs::Calibrated {
                            input: &input(0).buffer,
                            original: &input(1).buffer,
                            shield_corrected: &input(2).buffer,
                            black: &input(3).buffer,
                            clipping,
                        }
                    } else {
                        Inputs::Native {
                            original: &input(0).buffer,
                            shield_corrected: &input(1).buffer,
                            black: &input(2).buffer,
                        }
                    };
                    Some(crate::nodes::sensor::row_guard::apply(
                        gpu, inputs, prepared, profile,
                    )?)
                }
                Operation::ShieldNoise(prepared) => {
                    if inherited != prepared.extent()
                        || input(1).description.map(|d| d.extent) != Some(inherited)
                    {
                        return Err("shield-noise extent mismatch".into());
                    }
                    Some(crate::nodes::sensor::shield_noise::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        &input(2).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::Fppd(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("phase-detection extent mismatch".into());
                    }
                    Some(crate::nodes::sensor::fppd::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::PurpleFringe(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("purple-fringe extent mismatch".into());
                    }
                    Some(crate::nodes::color::purple_fringe::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::SectorColor(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("chroma-sector extent mismatch".into());
                    }
                    Some(crate::nodes::color::sector_color::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::RawColorBlend(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("raw-color extent mismatch".into());
                    }
                    let output = crate::nodes::color::raw_color_blend::apply(
                        gpu,
                        &input(0).buffer,
                        prepared.requires_highlight_mask().then(|| &input(1).buffer),
                        prepared,
                        profile,
                    )?;
                    for (slot, buffer) in [(1, output.neutralized), (2, output.raw_color)] {
                        if let Some(&side) = node.outputs.get(slot) {
                            if required[side] {
                                values[side] = Some(Value {
                                    working_scale,
                                    view_baked,
                                    view,
                                    buffer,
                                    description: Some(Description::new(
                                        inherited,
                                        crate::image::Space::CorrectedSensorRgb,
                                    )),
                                });
                            }
                        }
                    }
                    Some(output.rgb)
                }
                Operation::ChannelNoise(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("channel noise filter extent mismatch".into());
                    }
                    let output = crate::nodes::color::channel_noise::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?;
                    if let Some(&side) = node.outputs.get(1) {
                        if required[side] {
                            values[side] = Some(Value {
                                working_scale,
                                view_baked,
                                view,
                                buffer: output.source_after,
                                description: Some(Description::new(
                                    inherited,
                                    crate::image::Space::CorrectedSensorRgb,
                                )),
                            });
                        }
                    }
                    Some(output.rgb)
                }
                Operation::TwoScaleNoise(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("two-scale RGB filter extent mismatch".into());
                    }
                    let output = crate::nodes::color::two_scale_noise::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?;
                    if let Some(&side) = node.outputs.get(1) {
                        if required[side] {
                            values[side] = Some(Value {
                                working_scale,
                                view_baked,
                                view,
                                buffer: output.source_after,
                                description: Some(Description::new(
                                    inherited,
                                    crate::image::Space::CorrectedSensorRgb,
                                )),
                            });
                        }
                    }
                    Some(output.rgb)
                }
                Operation::HighlightRestoration(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("highlight restoration extent does not match its input".into());
                    }
                    let output = crate::nodes::sensor::highlight_restoration::apply(
                        gpu,
                        &input(0).buffer,
                        prepared.requires_evidence().then(|| {
                            crate::nodes::sensor::highlight_restoration::EvidenceInput {
                                controls: &input(1).buffer,
                                unfiltered: &input(2).buffer,
                            }
                        }),
                        prepared,
                        profile,
                    )?;
                    if let Some(&side) = node.outputs.get(1) {
                        if required[side] {
                            values[side] = Some(Value {
                                working_scale,
                                view_baked,
                                view,
                                buffer: output.mask,
                                description: None,
                            });
                        }
                    }
                    if let Some(&side) = node.outputs.get(2) {
                        if required[side] {
                            values[side] = Some(Value {
                                working_scale,
                                view_baked,
                                view,
                                buffer: output
                                    .recovery_mask
                                    .ok_or("missing highlight recovery mask")?,
                                description: None,
                            });
                        }
                    }
                    Some(output.rgb)
                }
                Operation::LumaClusters(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("blue cluster extent does not match its input".into());
                    }
                    Some(crate::nodes::sensor::luma_cluster::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::ByteMaskDouble(params) => {
                    Some(crate::nodes::color::byte_mask_double::apply(
                        gpu,
                        &input(0).buffer,
                        params,
                        profile,
                    )?)
                }
                Operation::MaskImage { params, .. } => {
                    if inherited != params.extent() {
                        return Err("mask inspection extent mismatch".into());
                    }
                    Some(crate::nodes::finish::mask_image::apply(
                        gpu,
                        &input(0).buffer,
                        params,
                        profile,
                    )?)
                }
                Operation::BlueGuidedReconstruct(prepared) => {
                    if inherited != prepared.extent()
                        || input(1).description.map(|d| d.extent) != Some(inherited)
                        || input(2).description.map(|d| d.extent) != Some(inherited)
                        || input(3).description.map(|d| d.extent) != Some(prepared.blue_extent())
                    {
                        return Err("blue-guided reconstruction input extents do not match its sampling grid".into());
                    }
                    let output = crate::nodes::color::blue_guided_reconstruct::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        &input(2).buffer,
                        &input(3).buffer,
                        prepared,
                        profile,
                    )?;
                    let extent = prepared.output_extent();
                    if required[node.outputs[1]] {
                        values[node.outputs[1]] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: output.residual,
                            description: Some(Description::new(
                                extent,
                                crate::image::Space::LuminanceRatio,
                            )),
                        });
                    }
                    required[index].then_some(output.rgb)
                }
                Operation::ResidualRgb {
                    params,
                    wait_for_reconstruction,
                } => {
                    check_residual_extents(params, inherited, input(1))?;
                    // The native lookahead can leave reconstruction scratch in flight.
                    if *wait_for_reconstruction
                        && node.inputs.iter().any(|&id| !cached[id])
                        && gpu.has_work_in_flight()
                    {
                        gpu.settle()?;
                        execution.check()?;
                    }
                    let source = last_read_or_share(values, consumers, node.inputs[0]);
                    let gain = &values[node.inputs[1]].as_ref().unwrap().buffer;
                    Some(crate::nodes::color::residual_finish::rgb(
                        gpu, source, gain, params, profile,
                    )?)
                }
                Operation::ResidualDetail(params) => {
                    check_residual_extents(params, inherited, input(1))?;
                    Some(crate::nodes::color::residual_finish::detail(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        &input(2).buffer,
                        params,
                        profile,
                    )?)
                }
                Operation::RgbDespeckle(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("RGB despeckle extent does not match its input".into());
                    }
                    Some(crate::nodes::sensor::rgb_despeckle::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::ChromaClusters(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("RGB cluster extent does not match its input".into());
                    }
                    Some(crate::nodes::sensor::chroma_clusters::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::ChromaRepair { prepared, stage } => {
                    if inherited != prepared.extent() {
                        return Err("chroma repair extent does not match its input".into());
                    }
                    Some(crate::nodes::sensor::chroma_repair::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        prepared,
                        *stage,
                        profile,
                    )?)
                }
                Operation::SparseRepair(prepared) => {
                    if inherited != prepared.extent()
                        || input(1).description.map(|d| d.extent) != Some(prepared.blue_extent())
                    {
                        return Err(
                            "sparse repair input extents do not match its sampling grid".into()
                        );
                    }
                    let output = crate::nodes::sensor::sparse_repair::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        prepared,
                        crate::nodes::sensor::sparse_repair::Requested {
                            blue: required[node.outputs[1]],
                            defects: required[node.outputs[2]],
                        },
                        profile,
                    )?;
                    for (side, buffer, extent) in [
                        (node.outputs[1], output.blue, prepared.blue_extent()),
                        (node.outputs[2], output.defects, inherited),
                    ] {
                        if required[side] {
                            values[side] = Some(Value {
                                working_scale,
                                view_baked,
                                view,
                                buffer: buffer.ok_or("missing requested sparse repair output")?,
                                description: graph.outputs[side].data.image_description(extent),
                            });
                        }
                    }
                    required[index].then_some(output.rgb)
                }
                Operation::HlpCalibration(prepared) => {
                    if inherited != prepared.extent()
                        || input(1).description.map(|d| d.extent) != Some(prepared.blue_extent())
                    {
                        return Err(
                            "sparse calibration input extents do not match its sampling grid"
                                .into(),
                        );
                    }
                    let output = crate::nodes::sensor::hlp_calibration::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        prepared,
                        profile,
                    )?;
                    for (side, buffer, extent) in [
                        (node.outputs[1], output.blue, prepared.blue_extent()),
                        (node.outputs[2], output.scratch, prepared.scratch_extent()),
                    ] {
                        if required[side] {
                            values[side] = Some(Value {
                                working_scale,
                                view_baked,
                                view,
                                buffer,
                                description: graph.outputs[side].data.image_description(extent),
                            });
                        }
                    }
                    required[index].then_some(output.rgb)
                }
                Operation::PhaseGain(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("phase gain extent does not match its input".into());
                    }
                    Some(crate::nodes::sensor::phase_gain::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::PhaseExtraction(prepared) => {
                    if inherited != prepared.extent() {
                        return Err("phase extraction extent does not match source cells".into());
                    }
                    let (rgb, blue) = crate::nodes::sensor::phase_extraction::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?;
                    let side = node.outputs[1];
                    if required[side] {
                        values[side] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: blue,
                            description: graph.outputs[side]
                                .data
                                .image_description(prepared.blue_extent()),
                        });
                    }
                    required[index].then_some(rgb)
                }
                Operation::ExtractBlue(extent) => {
                    if inherited != *extent {
                        return Err("blue-channel extraction extent mismatch".into());
                    }
                    Some(crate::nodes::color::extract_blue::apply(
                        gpu,
                        &input(0).buffer,
                        *extent,
                        profile,
                    )?)
                }
                Operation::RenderedInput { data } => {
                    let Input::Rendered(image) = &context else {
                        return Err("rendered input context required".into());
                    };
                    if data
                        .image_description(image.description.extent)
                        .map(|mut d| {
                            d.view_baked = image.description.view_baked;
                            d.linear_white = image.description.linear_white;
                            d
                        })
                        != Some(image.description)
                        || image.buffer.size() < image.description.byte_len()
                    {
                        return Err(
                            "imported image representation does not match graph input".into()
                        );
                    }
                    Some(image.buffer.clone())
                }
                Operation::FixedPointCalibration(prepared) => {
                    Some(crate::nodes::sensor::fixed_point::apply_correction(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::FixedPointColor(prepared) => {
                    let (rgb, layers) = crate::nodes::sensor::fixed_point::apply_transform(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?;
                    let layer_id = node.outputs[1];
                    if required[layer_id] {
                        values[layer_id] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: layers,
                            description: DataType::CalibratedLayers.image_description(inherited),
                        });
                    }
                    required[index].then_some(rgb)
                }
                Operation::ChromaSmooth(prepared) | Operation::CodeChromaSmooth(prepared) => Some(
                    crate::nodes::color::chroma::apply(gpu, &input(0).buffer, prepared, profile)?,
                ),
                Operation::SignedRepair(prepared) => {
                    Some(prepared.apply(gpu, &input(0).buffer, profile)?)
                }
                Operation::SignedDespeckle(prepared) => {
                    Some(prepared.apply(gpu, &input(0).buffer, profile)?)
                }
                Operation::RedDetail(prepared) => Some(crate::nodes::sensor::red_detail::apply(
                    gpu,
                    &input(0).buffer,
                    prepared,
                    profile,
                )?),
                Operation::HighlightStream(prepared) => {
                    Some(crate::nodes::color::highlight_stream::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::CodeGain(prepared) => Some(crate::nodes::color::code_gain::apply(
                    gpu,
                    &input(0).buffer,
                    prepared,
                    profile,
                )?),
                Operation::LogReconstruct(prepared) => {
                    Some(crate::nodes::color::log_reconstruction::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::RangeReconstruct(prepared) => {
                    Some(crate::nodes::color::range_reconstruction::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
                Operation::RootDetail(prepared) => {
                    let result = crate::nodes::finish::root_detail::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?;
                    let detail_id = node.outputs[1];
                    if required[detail_id] {
                        values[detail_id] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: result.detail,
                            description: DataType::SignedDetail.image_description(inherited),
                        });
                    }
                    required[index].then_some(result.rgb)
                }
                Operation::SignedLinearDetail(prepared) => {
                    Some(crate::nodes::finish::signed_response::apply_linear_detail(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        inherited,
                        prepared,
                        profile,
                    )?)
                }
                Operation::LinearCodeBase | Operation::LinearCodeRgb { .. } => {
                    Some(input(0).buffer.clone())
                }
                Operation::LinearSignedResponse(prepared) => {
                    Some(crate::nodes::finish::signed_response::apply_linear(
                        gpu,
                        &input(0).buffer,
                        inherited,
                        prepared,
                        profile,
                    )?)
                }
                Operation::CodeResponse { prepared, .. }
                | Operation::LinearCodeResponse { prepared, .. } => {
                    Some(crate::nodes::finish::code_response::apply(
                        gpu,
                        &input(0).buffer,
                        inherited,
                        prepared,
                        profile,
                    )?)
                }
                Operation::SignedPhotographic(prepared) => {
                    Some(crate::nodes::finish::signed_response::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        inherited,
                        prepared,
                        profile,
                    )?)
                }
                Operation::Black(params) => {
                    let mut histogram = gpu.storage("Dark-shield histogram", 3 * 4096 * 4)?;
                    let uniform = gpu.uniform(params);
                    dispatch(
                        Kernel::Histogram,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Write(histogram.write())),
                            (2, Read(&uniform)),
                        ],
                        [params.source[2].div_ceil(64), 1],
                    );
                    let black = if required[index] {
                        let mut black = gpu.storage("Black levels", 12)?;
                        dispatch(
                            Kernel::BlackMean,
                            &mut [
                                (1, Read(&histogram)),
                                (2, Read(&uniform)),
                                (3, Write(black.write())),
                            ],
                            [3, 1],
                        );
                        Some(black)
                    } else {
                        None
                    };
                    let side = node.outputs[1];
                    if required[side] {
                        values[side] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: histogram,
                            description: None,
                        });
                    }
                    black
                }
                Operation::Calibrate(calibration) => {
                    let metadata = context.metadata();
                    if inherited != [metadata.width as u32, metadata.height as u32] {
                        return Err("calibration requires original sensor coordinates".into());
                    }
                    let mut output = scratch.take(gpu, bytes)?;
                    let grid = gpu.upload("Spatial gain grids", &calibration.grid_values)?;
                    let uniform = gpu.uniform(&calibration.params);
                    dispatch(
                        Kernel::Calibration,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Read(&input(1).buffer)),
                            (2, Read(&grid)),
                            (3, Read(&uniform)),
                            (4, Write(output.write())),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::Monochrome {
                    stage, prepared, ..
                } => {
                    let inputs: Vec<_> = (0..node.inputs.len()).map(|i| &input(i).buffer).collect();
                    Some(crate::nodes::monochrome::apply(
                        gpu, *stage, &inputs, prepared, profile,
                    )?)
                }
                Operation::NoiseContext(params) => {
                    let context =
                        reconstruction::global_context(gpu, &input(0).buffer, *params, profile)?;
                    let edge_id = node.outputs[1];
                    if required[edge_id] {
                        values[edge_id] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: context.edges,
                            description: DataType::ScalarField.image_description(extent),
                        });
                    }
                    Some(context.layers)
                }
                Operation::NoiseBase(params) => {
                    let projected = node.outputs[1];
                    let edge_id = node.outputs[2];
                    let base = if params.uses_global_context() {
                        reconstruction::base_from_context(
                            gpu,
                            &input(0).buffer,
                            &input(1).buffer,
                            *params,
                            required[edge_id],
                            required[index] || required[projected],
                            required[node.outputs[3]],
                            profile,
                        )?
                    } else {
                        reconstruction::base(
                            gpu,
                            &input(0).buffer,
                            *params,
                            required[edge_id],
                            required[index] || required[projected],
                            required[node.outputs[3]],
                            profile,
                        )?
                    };
                    if required[projected] {
                        values[projected] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: reconstruction::project(
                                gpu,
                                base.tiles.as_ref().unwrap(),
                                *params,
                                3,
                                profile,
                            )?,
                            description: DataType::CalibratedLayers
                                .image_description(params.output_extent()),
                        });
                    }
                    if required[edge_id] {
                        values[edge_id] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: reconstruction::project(
                                gpu,
                                base.edges.as_ref().unwrap(),
                                *params,
                                1,
                                profile,
                            )?,
                            description: DataType::ScalarField
                                .image_description(params.output_extent()),
                        });
                    }
                    if required[node.outputs[3]] {
                        values[node.outputs[3]] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: base.seed.unwrap(),
                            description: None,
                        });
                    }
                    if required[index] {
                        base.tiles
                    } else {
                        None
                    }
                }
                Operation::DirectionalBanding { stage, prepared } => {
                    let params = prepared.params();
                    let estimates = node.inputs.get(1).map(|_| &input(1).buffer);
                    let tiles = crate::nodes::color::directional_banding::apply_stage(
                        gpu,
                        &input(0).buffer,
                        estimates,
                        prepared,
                        *stage,
                        profile,
                    )?;
                    if let Some(&projected) = node.outputs.get(1).filter(|id| required[**id]) {
                        values[projected] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: reconstruction::project(gpu, &tiles, params, 3, profile)?,
                            description: DataType::CalibratedLayers
                                .image_description(params.output_extent()),
                        });
                    }
                    required[index].then_some(tiles)
                }
                Operation::Column { params, enabled } => {
                    let tiles = if *enabled {
                        reconstruction::column(gpu, &input(0).buffer, *params, profile)?
                    } else {
                        reconstruction::raw_tiles(gpu, &input(0).buffer, *params, profile)?
                    };
                    let projected = node.outputs[1];
                    if required[projected] {
                        values[projected] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: reconstruction::project(gpu, &tiles, *params, 3, profile)?,
                            description: DataType::CalibratedLayers
                                .image_description(params.output_extent()),
                        });
                    }
                    required[index].then_some(tiles)
                }
                Operation::Ratio(params) => {
                    let ratio = reconstruction::ratio(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        *params,
                        profile,
                    )?;
                    let projected = node.outputs[1];
                    if required[projected] {
                        values[projected] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: reconstruction::project(gpu, &ratio, *params, 1, profile)?,
                            description: DataType::LuminanceRatio
                                .image_description(params.output_extent()),
                        });
                    }
                    required[index].then_some(ratio)
                }
                Operation::Pattern { params, prepared } => {
                    let projected = node.outputs[1];
                    let mask = node.outputs[2];
                    let result = pattern::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        prepared,
                        required[index] || required[projected],
                        required[mask],
                        profile,
                    )?;
                    if required[projected] {
                        values[projected] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: reconstruction::project(
                                gpu,
                                result.ratio.as_ref().unwrap(),
                                *params,
                                1,
                                profile,
                            )?,
                            description: DataType::LuminanceRatio
                                .image_description(params.output_extent()),
                        });
                    }
                    if required[mask] {
                        values[mask] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: reconstruction::project(
                                gpu,
                                result.mask.as_ref().unwrap(),
                                *params,
                                1,
                                profile,
                            )?,
                            description: DataType::ScalarField
                                .image_description(params.output_extent()),
                        });
                    }
                    if required[index] {
                        result.ratio
                    } else {
                        None
                    }
                }
                Operation::ProjectColorTiles { params, .. } => Some(reconstruction::project(
                    gpu,
                    &input(0).buffer,
                    *params,
                    3,
                    profile,
                )?),
                Operation::Texture { params, prepared } => {
                    use crate::nodes::color::texture;
                    let result = texture::apply(
                        gpu,
                        texture::Inputs {
                            raw: &input(0).buffer,
                            base: (node.inputs.len() > 1).then(|| &input(1).buffer),
                            ratio: (node.inputs.len() > 2).then(|| &input(2).buffer),
                        },
                        *params,
                        *prepared,
                        texture::Requested {
                            layers: required[index],
                            change: required[node.outputs[1]],
                        },
                        profile,
                    )?;
                    if let Some(buffer) = result.change {
                        values[node.outputs[1]] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer,
                            description: DataType::LuminanceRatio
                                .image_description(params.output_extent()),
                        });
                    }
                    result.layers
                }
                Operation::TiledColor { params, color, .. } => {
                    let tiles =
                        reconstruction::color(gpu, &input(0).buffer, *color, *params, profile)?;
                    let projected = node.outputs[1];
                    if required[projected] {
                        values[projected] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: reconstruction::project(gpu, &tiles, *params, 3, profile)?,
                            description: graph.outputs[projected]
                                .data
                                .image_description(params.output_extent()),
                        });
                    }
                    required[index].then_some(tiles)
                }
                Operation::TiledColorDq { params, table, .. } => {
                    let tiles =
                        reconstruction::color_dq(gpu, &input(0).buffer, table, *params, profile)?;
                    let projected = node.outputs[1];
                    if required[projected] {
                        values[projected] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: reconstruction::project(gpu, &tiles, *params, 3, profile)?,
                            description: graph.outputs[projected]
                                .data
                                .image_description(params.output_extent()),
                        });
                    }
                    required[index].then_some(tiles)
                }
                Operation::Banding(description) => {
                    let prepared = profile
                        .measure("materialize row correction", || description.materialize())?;
                    let result =
                        banding::apply(gpu, &input(0).buffer, &prepared, required[index], profile)?;
                    let rows = node.outputs[1];
                    if required[rows] {
                        values[rows] = Some(Value {
                            working_scale,
                            view_baked,
                            view,
                            buffer: result.rows,
                            description: None,
                        });
                    }
                    result.image
                }
                Operation::Highlights(prepared) => {
                    let materialized;
                    let prepared = if prepared.flags.is_none() {
                        materialized =
                            engine.materialize_highlights(context.metadata(), prepared)?;
                        &materialized
                    } else {
                        prepared
                    };
                    highlight::apply(gpu, Some(&input(0).buffer), prepared, false, profile)?.pixels
                }
                Operation::ColorField(prepared) => {
                    let clipping = prepared
                        .grid
                        .map(|grid| engine.clipping_flags(context.metadata(), Some(grid)))
                        .transpose()?;
                    Some(color_field::apply(
                        gpu,
                        color_field::Inputs {
                            input: &input(0).buffer,
                            original: &input(1).buffer,
                            black: &input(2).buffer,
                            unrepaired: &input(3).buffer,
                            repaired: &input(4).buffer,
                            clipping: clipping.as_ref(),
                        },
                        prepared,
                        profile,
                        execution,
                    )?)
                }
                Operation::HighlightDensity(prepared) => {
                    let materialized;
                    let prepared = if prepared.flags.is_none() {
                        materialized =
                            engine.materialize_highlights(context.metadata(), prepared)?;
                        &materialized
                    } else {
                        prepared
                    };
                    highlight::apply(gpu, None, prepared, true, profile)?.density
                }
                Operation::Despeckle(prepared) => Some(despeckle::apply(
                    gpu,
                    &input(0).buffer,
                    prepared.needs_sensor_codes().then(|| &input(1).buffer),
                    prepared,
                    profile,
                )?),
                Operation::Repair(prepared) => Some(camera_repair::apply(
                    gpu,
                    &input(1).buffer,
                    &input(0).buffer,
                    prepared,
                    profile,
                )?),
                Operation::Color { params, .. } | Operation::Saturation { params, .. } => {
                    let mut output = scratch.take(gpu, bytes)?;
                    let mut params = *params;
                    params.source[..2].copy_from_slice(&inherited);
                    let uniform = gpu.uniform(&params);
                    dispatch(
                        if matches!(node.operation, Operation::Saturation { .. }) {
                            Kernel::ReferenceSaturation
                        } else {
                            Kernel::Color
                        },
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Read(&uniform)),
                            (2, Write(output.write())),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::ColorDq { table, .. } => {
                    let mut output = scratch.take(gpu, bytes)?;
                    let lookup = gpu.upload("ColorDQ lookup", &table.values)?;
                    let uniform = gpu.uniform(&[inherited[0], inherited[1], table.period, 0]);
                    dispatch(
                        Kernel::ColorDq,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Read(&lookup)),
                            (2, Read(&uniform)),
                            (3, Write(output.write())),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::ShadowNeutralGuide { params, .. } => {
                    Some(crate::nodes::finish::shadow_neutralize::prepare_guide(
                        gpu,
                        &input(0).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::ShadowNeutralize { params, .. } => {
                    Some(crate::nodes::finish::shadow_neutralize::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::LinearColorEditor { params, .. } => {
                    Some(crate::nodes::finish::color_editor::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::PerceptualGrade { params, .. } => {
                    Some(crate::nodes::finish::perceptual_grade::apply(
                        gpu,
                        &input(0).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::AdvancedColor { params, .. } => {
                    Some(crate::nodes::finish::advanced_color::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        **params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::DehazeEstimate { params, .. } => {
                    Some(crate::nodes::finish::dehaze::estimate(
                        gpu,
                        &input(0).buffer,
                        *params,
                        profile,
                    )?)
                }
                Operation::Dehaze { params, .. } => Some(crate::nodes::finish::dehaze::apply(
                    gpu,
                    &input(0).buffer,
                    &input(1).buffer,
                    *params,
                    inherited,
                    profile,
                )?),
                Operation::MonoTexture { controls, scale } => {
                    Some(crate::nodes::monochrome::texture::apply(
                        gpu,
                        &input(0).buffer,
                        *controls,
                        *scale,
                        inherited,
                        profile,
                    )?)
                }
                Operation::LinearBasis { rows, .. } => {
                    Some(crate::nodes::finish::linear_basis::apply(
                        gpu,
                        &input(0).buffer,
                        *rows,
                        inherited,
                        input(0).description.unwrap().linear_white,
                        profile,
                    )?)
                }
                Operation::LinearWhiteBalance { rows, .. } => {
                    Some(crate::nodes::finish::linear_basis::apply(
                        gpu,
                        &input(0).buffer,
                        *rows,
                        inherited,
                        1,
                        profile,
                    )?)
                }
                Operation::TransferLinearGain { .. } => {
                    Some(crate::nodes::finish::linear_gain::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        &input(2).buffer,
                        inherited,
                        profile,
                    )?)
                }
                Operation::ImageAlias { .. } => Some(input(0).buffer.clone()),
                Operation::PhotographicTone { params, .. } => {
                    Some(crate::nodes::finish::photographic::tone::apply(
                        gpu,
                        &input(0).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::LinearUniversalSharpen { params, .. } => {
                    Some(crate::nodes::finish::photographic::sharpen::apply(
                        gpu,
                        &input(0).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::LinearBrightness { params, .. } => {
                    Some(crate::nodes::finish::photographic::tone::brightness(
                        gpu,
                        &input(0).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::LinearLocalContrast { params, .. } => {
                    Some(crate::nodes::finish::photographic::local_contrast::apply(
                        gpu,
                        &input(0).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::ToneMask { guide, .. } | Operation::AdaptiveToneGuide { guide, .. } => {
                    Some(crate::nodes::finish::tone_equalizer::build_guide(
                        gpu,
                        &input(0).buffer,
                        *guide,
                        profile,
                    )?)
                }
                Operation::ExpandBrightnessGuide { guide, .. } => {
                    Some(crate::nodes::finish::tone_equalizer::expand_mask(
                        gpu,
                        &input(0).buffer,
                        *guide,
                        profile,
                    )?)
                }
                Operation::AdaptiveTone {
                    window,
                    params,
                    analysis,
                    curve,
                    ..
                } => {
                    use crate::nodes::finish::adaptive_tone::{apply_with_analysis, Samples};
                    let result = apply_with_analysis(
                        gpu,
                        Samples {
                            rgb: &input(0).buffer,
                            mask: &input(1).buffer,
                        },
                        Samples {
                            rgb: &input(2).buffer,
                            mask: &input(3).buffer,
                        },
                        curve,
                        *params,
                        *analysis,
                        profile,
                    )?;
                    Some(if let Some(prepared) = window.crop(inherited)? {
                        crate::nodes::finish::image_region::apply(gpu, &result.rgb, &prepared)?
                    } else {
                        result.rgb
                    })
                }
                Operation::TonalRanges {
                    sampling,
                    window,
                    white,
                    controls,
                    pivot,
                    space,
                } => {
                    let result = crate::nodes::finish::tone_equalizer::apply_with_white(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        inherited,
                        crate::nodes::finish::tone_equalizer::Response {
                            sampling: *sampling,
                            controls: *controls,
                            pivot: *pivot,
                            space: *space,
                        },
                        *white,
                        profile,
                    )?;
                    Some(if let Some(prepared) = window.crop(inherited)? {
                        crate::nodes::finish::image_region::apply(gpu, &result, &prepared)?
                    } else {
                        result
                    })
                }
                Operation::Tone {
                    curve, exposure, ..
                } => {
                    let mut output = scratch.take(gpu, bytes)?;
                    let curve = gpu.upload("Color-mode tone curve", curve)?;
                    let uniform = gpu.uniform(&ScalarParams {
                        source: dimensions,
                        value: [*exposure, 0., 0., 0.],
                    });
                    dispatch(
                        Kernel::Tone,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Read(&curve)),
                            (2, Read(&uniform)),
                            (3, Write(output.write())),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::SharpSignal { scale, .. } => {
                    if !scale.is_finite() || *scale <= 0. || *scale > 1. {
                        return Err("invalid high-pass working scale".into());
                    }
                    let mut output = gpu.storage(
                        "Native signed mono high-pass",
                        u64::from(inherited[0]) * u64::from(inherited[1]) * 4,
                    )?;
                    let uniform = gpu.uniform(&ScalarParams {
                        source: dimensions,
                        value: [*scale, 0., 0., 0.],
                    });
                    dispatch(
                        Kernel::SharpSignal,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (2, Read(&uniform)),
                            (3, Write(output.write())),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::Sharpen { amount, .. } | Operation::MonoSharpen { amount, .. } => {
                    if input(0).description.unwrap().extent != input(1).description.unwrap().extent
                    {
                        return Err("sharpen inputs have different image extents".into());
                    }
                    // A camera base of zero means Off too. Running the
                    // native kernel at zero would still clamp signed RGB.
                    if *amount == 0. {
                        Some(input(0).buffer.clone())
                    } else {
                        let mut output = scratch.take(gpu, bytes)?;
                        let uniform = gpu.uniform(&ScalarParams {
                            source: dimensions,
                            value: [*amount, 0., 0., 0.],
                        });
                        dispatch(
                            if matches!(node.operation, Operation::MonoSharpen { .. }) {
                                Kernel::MonoSharpen
                            } else {
                                Kernel::Sharpen
                            },
                            &mut [
                                (0, Read(&input(0).buffer)),
                                (1, Read(&input(1).buffer)),
                                (2, Read(&uniform)),
                                (3, Write(output.write())),
                            ],
                            groups,
                        );
                        Some(output)
                    }
                }
                Operation::Encode {
                    params,
                    space,
                    destination,
                } => {
                    let (resized, encoded_params) = crate::nodes::finish::resize::apply(
                        gpu,
                        &input(0).buffer,
                        params,
                        profile,
                    )?;
                    let transform = gpu.uniform(&destination.transform(*space));
                    let mut output = gpu.storage(
                        "Encoded packed RGB16",
                        desc.expect("encoded description").byte_len(),
                    )?;
                    let uniform = gpu.uniform(&encoded_params);
                    dispatch(
                        Kernel::Encode,
                        &mut [
                            (0, Read(&resized)),
                            (1, Read(&uniform)),
                            (2, Write(output.write())),
                            (3, Read(&transform)),
                        ],
                        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
                    );
                    Some(output)
                }
            };
            let mut diagnostic =
                diagnostic.map(|(mut observation, started, before, shared_start)| {
                    observation.host_milliseconds = Some(started.elapsed().as_secs_f64() * 1000.);
                    observation.activity = Some(gpu.activity().since(before));
                    observation.nested_gpu_end = profile.gpu.len();
                    let shared_end = profile.mode.gpu().then(|| trace.len());
                    (observation, started, shared_start.zip(shared_end))
                });
            profile.record(node.operation.label(), start);
            let publication = profile.mode.enabled().then(Instant::now);
            if !cached[index] {
                values[index] = output.map(|buffer| Value {
                    working_scale,
                    view_baked,
                    view,
                    buffer,
                    description: desc,
                });
            }
            for &id in &node.outputs {
                if let (Some(view), Some(value)) = (graph.outputs[id].view, values[id].as_mut()) {
                    value.view = Some(view);
                }
                if let Some(value) = values[id].as_ref() {
                    execution.store(&graph.outputs[id].cache_name(), value);
                }
            }
            if let Some((observation, _, _)) = &mut diagnostic {
                observation.publication_ms =
                    publication.map(|start| start.elapsed().as_secs_f64() * 1000.);
                let diagnostic_started = Instant::now();
                for (output, &id) in observation.outputs.iter_mut().zip(&node.outputs) {
                    output.published = !cached[id] && values[id].is_some();
                    output.resource = values[id].as_ref().map(resource_observation);
                }
                observation.memory_after_publication = boundary_memory(values, scratch, execution);
                observation.diagnostic_overhead_ms +=
                    diagnostic_started.elapsed().as_secs_f64() * 1000.;
            }
            trace_graph_outputs(
                graph_id,
                graph,
                &node.outputs,
                values,
                cached,
                reachable.as_deref(),
                execution,
            );
            submission_stage.complete();
            *stage_completed += 1;
            *pending_stages += 1;
            let retirement = profile.mode.enabled().then(Instant::now);
            for &id in &reads[node_index] {
                consumers[id] -= 1;
                if consumers[id] == 0 {
                    if let Some(value) = values[id].take() {
                        // The scratch pool can reuse ordinary image buffers, not tiled or
                        // atomic workspaces. Release also checks for retained readers
                        // so a later render cannot overwrite an earlier image.
                        if value.description.is_some_and(|d| {
                            matches!(d.layout, Layout::Rgb32Float | Layout::Scalar32Float)
                        }) {
                            scratch.release(value.buffer);
                        }
                    }
                }
            }
            if let Some((mut observation, started, shared_range)) = diagnostic {
                observation.retirement_ms =
                    retirement.map(|start| start.elapsed().as_secs_f64() * 1000.);
                let diagnostic_started = Instant::now();
                observation.memory_after_retirement = boundary_memory(values, scratch, execution);
                observation.diagnostic_overhead_ms +=
                    diagnostic_started.elapsed().as_secs_f64() * 1000.;
                observation.total_ms = Some(started.elapsed().as_secs_f64() * 1000.);
                if let (Some(ranges), Some((start, end))) = (shared_ranges.as_mut(), shared_range) {
                    ranges.push((profile.nodes.len(), start, end));
                }
                *pending_observation = Some(profile.nodes.len());
                profile.nodes.push(observation);
            }
            if *stage_completed == *stage_total {
                continue;
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Called after the tick's GPU wait. Publishes progress for the step and
    /// releases spare scratch that the step's retired inputs left behind.
    pub(super) fn complete_tick(
        &mut self,
        waited: std::time::Duration,
        profile: &mut Profile,
        progress: Option<&dyn Fn(super::RenderProgress)>,
    ) {
        if let Some(index) = self.pending_observation.take() {
            let waited_ms = waited.as_secs_f64() * 1000.;
            let observation = &mut profile.nodes[index];
            observation.completion_wait_ms = Some(waited_ms);
            observation.total_ms = observation.total_ms.map(|total| total + waited_ms);
            profile.record_duration(TICK_WAIT_STAGE, waited);
        }
        if self.pending_stages > 0 {
            if let Some(progress) = progress {
                progress(super::RenderProgress::Stage {
                    completed: self.stage_completed,
                    total: self.stage_total,
                });
            }
            self.pending_stages = 0;
            self.scratch.trim_completed(COMPLETED_SCRATCH_SPARE_BYTES);
        }
    }

    pub(super) fn finish(
        self,
        engine: &Engine,
        targets: &[usize],
        profile: &mut Profile,
    ) -> Result<Vec<Value>> {
        let Self {
            graph_id,
            trace,
            scratch,
            mut values,
            shared_ranges,
            ..
        } = self;
        let gpu = &engine.gpu;
        let shared_gpu_base = profile.mode.gpu().then_some(profile.gpu.len());
        gpu.finish_trace(&trace, profile)?;
        if let (Some(base), Some(ranges)) = (shared_gpu_base, shared_ranges) {
            for (index, start, end) in ranges {
                profile.nodes[index].shared_gpu_start = Some(base + start);
                profile.nodes[index].shared_gpu_end = Some(base + end);
            }
        }
        if profile.mode.enabled() {
            profile.scratch_allocations += scratch.allocations;
            profile.scratch_reuses += scratch.reuses;
        }
        crate::diagnostics::event(
            crate::diagnostics::Category::Graph,
            "graph.complete",
            |fields| {
                if let Some(graph_id) = graph_id {
                    fields.u64("graph_id", graph_id);
                }
            },
        );
        Ok(targets.iter().filter_map(|id| values[*id].take()).collect())
    }
}

#[cfg(test)]
mod support_failure_tests {
    use super::*;

    #[test]
    fn scheduling_results_stay_typed_and_terminal_errors_keep_camera_context() {
        let metadata = Metadata {
            width: 3,
            height: 2,
            rotation: 0,
            processing_version: None,
            properties: [("CAMMODEL".into(), "test camera".into())].into(),
            matrices: Default::default(),
            lists: Default::default(),
        };
        for (kind, error) in [
            Box::new(SourceRequired) as Box<dyn std::error::Error + Send + Sync>,
            Box::new(crate::interactive::Superseded),
            Box::new(crate::priority::Interrupted),
        ]
        .into_iter()
        .enumerate()
        {
            let returned =
                report_failure(error, "execute-graph", &metadata, serde_json::Value::Null);
            assert!(match kind {
                0 => returned.is::<SourceRequired>(),
                1 => returned.is::<crate::interactive::Superseded>(),
                _ => returned.is::<crate::priority::Interrupted>(),
            });
            assert!(!returned.is::<crate::support::ReportedError>());
        }
        let error = report_failure(
            std::io::Error::from_raw_os_error(5).into(),
            "execute-graph",
            &metadata,
            serde_json::json!({"target_ids":[2]}),
        );
        let report = &error
            .downcast_ref::<crate::support::ReportedError>()
            .unwrap()
            .report;
        assert_eq!(
            report.operation.settings.as_ref().unwrap()["camera"],
            "test camera"
        );
        assert!(report.causes.iter().any(|cause| cause.os_code == Some(5)));
    }
}

#[cfg(test)]
mod observation_tests {
    use super::*;
    use crate::{
        image::{Description, Space, View},
        interactive::Cache,
        nodes::finish::root_detail,
        profile::Mode,
        render::Settings,
    };
    use std::sync::Arc;

    fn resumable_fixture(engine: &Engine) -> (RenderedImage, Graph, usize) {
        let extent = [2, 2];
        let image = RenderedImage {
            buffer: engine
                .gpu
                .upload(
                    "resumable signed codes",
                    &[100_f32, -200., 13000.].repeat(4),
                )
                .unwrap(),
            description: Description::new(extent, Space::NormalizedCodeRgb),
            view: View {
                crop: [0, 0, 2, 2],
                rotation: 0,
                extent,
            },
            metadata: Arc::new(Metadata {
                processing_version: None,
                width: 2,
                height: 2,
                rotation: 0,
                properties: Default::default(),
                matrices: Default::default(),
                lists: Default::default(),
            }),
            settings: Settings::new("Sunlight", "Standard"),
            owner: engine.owner.clone(),
            regional: None,
            dehaze_basis: None,
        };
        let mut graph = Graph::default();
        let input = graph
            .add(
                Operation::RenderedInput {
                    data: DataType::NormalizedCodeRgb,
                },
                &[],
            )
            .unwrap();
        let target = graph
            .add(
                Operation::CodeGain(
                    crate::nodes::color::code_gain::Prepared::new(extent, 2.).unwrap(),
                ),
                &[input],
            )
            .unwrap();
        graph
            .add(
                Operation::ImageAlias {
                    dehaze_basis: None,
                    data: DataType::LinearCodeRgb,
                    output: "unused.trailing",
                },
                &[target],
            )
            .unwrap();
        (image, graph, target)
    }

    #[test]
    #[ignore = "requires a GPU"]
    fn packed_lab_preserves_the_requested_intermediate_and_reuses_the_last_read() {
        use crate::nodes::color::packed_lab::{Direction, Prepared};
        let engine = Engine::new().unwrap();
        let (mut image, _, _) = resumable_fixture(&engine);
        let extent = image.description.extent;
        let samples = [100_f32, 200., 13000.].repeat(4);
        image.buffer = engine.gpu.upload("Unsigned RGB codes", &samples).unwrap();
        image.description = Description::new(extent, Space::UnsignedCodeRgb);
        let mut graph = Graph::default();
        let source = graph
            .add(
                Operation::RenderedInput {
                    data: DataType::UnsignedCodeRgb,
                },
                &[],
            )
            .unwrap();
        let convert = |direction| {
            Operation::PackedLab(
                Prepared::new(extent, direction, [1., 0., 0., 0., 1., 0., 0., 0., 1.]).unwrap(),
            )
        };
        let lab = graph.add(convert(Direction::Forward), &[source]).unwrap();
        let rgb = graph.add(convert(Direction::Inverse), &[lab]).unwrap();
        let render = |roots: &[usize]| {
            let before = engine.gpu_activity();
            let values = engine
                .run_graph(
                    Input::Rendered(&image),
                    &graph,
                    roots,
                    &mut Profile::default(),
                    &mut Execution::default(),
                )
                .unwrap();
            let copied = engine.gpu_activity().since(before).device_copy_bytes;
            let bytes: Vec<_> = values
                .iter()
                .map(|v| engine.gpu.readback(&v.buffer).unwrap())
                .collect();
            (bytes, copied)
        };
        let (lab_only, _) = render(&[lab]);
        let (rgb_only, copied) = render(&[rgb]);
        let (both, retained_copied) = render(&[lab, rgb]);
        assert_eq!(both, [lab_only[0].clone(), rgb_only[0].clone()]);
        assert_ne!(both[0], both[1]);
        assert_eq!(retained_copied, copied + image.buffer.size());
        assert_eq!(
            engine.gpu.readback(&image.buffer).unwrap(),
            bytemuck::cast_slice::<_, u8>(&samples)
        );
    }

    #[test]
    #[ignore = "requires a GPU"]
    fn each_tick_waits_once_and_preserves_samples_and_progress() {
        let engine = Engine::new().unwrap();
        let (image, graph, target) = resumable_fixture(&engine);
        let never_cancel = || false;
        {
            for cancellable in [false, true] {
                let progress = std::cell::RefCell::new(Vec::new());
                let report = |stage| progress.borrow_mut().push(stage);
                let mut execution = Execution {
                    cancel: cancellable.then_some(&never_cancel as &dyn Fn() -> bool),
                    progress: Some(&report),
                    ..Default::default()
                };
                let mut profile = Profile::new(Mode::Wall);
                let values = engine
                    .run_graph(
                        Input::Rendered(&image),
                        &graph,
                        &[target],
                        &mut profile,
                        &mut execution,
                    )
                    .unwrap();
                assert_eq!(profile.nodes.iter().filter(|node| node.executed).count(), 2);
                assert_eq!(
                    profile
                        .nodes
                        .iter()
                        .filter(|node| node.completion_wait_ms.is_some())
                        .count(),
                    2,
                    "cancellable={cancellable}",
                );
                assert_eq!(
                    *progress.borrow(),
                    [
                        super::super::RenderProgress::Stage {
                            completed: 1,
                            total: 2
                        },
                        super::super::RenderProgress::Stage {
                            completed: 2,
                            total: 2
                        },
                    ],
                );
                let bytes = engine.gpu.readback(&values[0].buffer).unwrap();
                let samples: &[f32] = bytemuck::cast_slice(&bytes);
                assert_eq!(samples, [200_f32, -400., 24000.].repeat(4));
            }
        }
        let error = engine
            .run_graph(
                Input::Rendered(&image),
                &graph,
                &[target, usize::MAX],
                &mut Profile::default(),
                &mut Execution::default(),
            )
            .err()
            .expect("unknown target must be rejected");
        let report = &error
            .downcast_ref::<crate::support::ReportedError>()
            .unwrap()
            .report;
        assert_eq!(report.operation.phase.as_deref(), Some("execute-graph"));
        let details = report.operation.settings.as_ref().unwrap();
        assert_eq!(details["extent"], serde_json::json!([2, 2]));
        assert_eq!(
            details["operation"]["target_ids"],
            serde_json::json!([target, usize::MAX])
        );
        assert_eq!(
            details["operation"]["target_names"],
            serde_json::json!([graph.outputs[target].name, null])
        );
        assert_eq!(details["operation"]["node_count"], graph.nodes.len());
    }

    #[test]
    #[ignore = "requires a GPU"]
    fn resumed_graph_preserves_signed_samples_progress_and_trailing_observations() {
        let engine = Engine::new().unwrap();
        let (image, graph, target) = resumable_fixture(&engine);
        let progress = std::cell::RefCell::new(Vec::new());
        let report = |stage| {
            if let super::super::RenderProgress::Stage { completed, total } = stage {
                progress.borrow_mut().push((completed, total));
            }
        };
        let mut execution = Execution {
            cancel: Some(&|| false),
            progress: Some(&report),
            ..Default::default()
        };
        let mut profile = Profile::new(Mode::Wall);
        let context = Input::Rendered(&image);
        let mut state = engine
            .start_graph(
                context,
                &graph,
                &[target],
                &mut profile,
                &mut execution,
                Some(1),
            )
            .unwrap();
        assert!(state
            .step(&engine, context, &graph, &mut profile, &mut execution,)
            .unwrap());
        assert!(progress.borrow().is_empty(), "progress waits for the tick");
        state.complete_tick(std::time::Duration::ZERO, &mut profile, execution.progress);
        assert_eq!(*progress.borrow(), [(1, 2)]);
        // The caller regains control with a live input and can resume later.
        assert!(state.values[0].is_some());
        assert!(
            !state
                .step(&engine, context, &graph, &mut profile, &mut execution,)
                .unwrap(),
            "the last required node also consumes trailing skipped nodes"
        );
        state.complete_tick(std::time::Duration::ZERO, &mut profile, execution.progress);
        assert_eq!(*progress.borrow(), [(1, 2), (2, 2)]);
        assert!(!state
            .step(&engine, context, &graph, &mut profile, &mut execution,)
            .unwrap());
        assert_eq!(profile.nodes.len(), 3);
        assert!(!profile.nodes[2].executed);
        let values = state.finish(&engine, &[target], &mut profile).unwrap();
        let bytes = engine.gpu.readback(&values[0].buffer).unwrap();
        let samples: &[f32] = bytemuck::cast_slice(&bytes);
        assert_eq!(samples, [200_f32, -400., 24000.].repeat(4));
        assert_eq!(values[0].description.unwrap().extent, [2, 2]);
    }

    #[test]
    #[ignore = "requires a GPU"]
    fn cancellation_between_advancements_does_not_execute_the_next_operation() {
        let engine = Engine::new().unwrap();
        let (image, graph, target) = resumable_fixture(&engine);
        let cancelled = std::cell::Cell::new(false);
        let cancel = || cancelled.get();
        let mut execution = Execution {
            cancel: Some(&cancel),
            ..Default::default()
        };
        let mut profile = Profile::new(Mode::Wall);
        let context = Input::Rendered(&image);
        let mut state = engine
            .start_graph(
                context,
                &graph,
                &[target],
                &mut profile,
                &mut execution,
                Some(1),
            )
            .unwrap();
        assert!(state
            .step(&engine, context, &graph, &mut profile, &mut execution,)
            .unwrap());
        cancelled.set(true);
        let before = engine.gpu_activity();
        assert!(state
            .step(&engine, context, &graph, &mut profile, &mut execution,)
            .is_err());
        assert_eq!(engine.gpu_activity().dispatches, before.dispatches);
        assert!(state.values[target].is_none());
        assert_eq!(profile.nodes.len(), 1);
    }

    #[test]
    #[ignore = "requires a GPU with timestamps"]
    fn cancelled_graph_never_publishes_unresolved_shared_trace_indices() {
        let engine = Engine::new().unwrap();
        assert!(
            engine
                .device()
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY),
            "this ignored test requires GPU timestamp-query support"
        );
        let extent = [8, 8];
        let image = RenderedImage {
            buffer: engine
                .gpu
                .upload("calibrated fixture", &[1_f32; 8 * 8 * 3])
                .unwrap(),
            description: Description::new(extent, Space::CalibratedLayers),
            view: View {
                crop: [0, 0, 8, 8],
                rotation: 0,
                extent,
            },
            metadata: Arc::new(Metadata {
                processing_version: None,
                width: 8,
                height: 8,
                rotation: 0,
                properties: Default::default(),
                matrices: Default::default(),
                lists: Default::default(),
            }),
            settings: Settings::new("Sunlight", "Standard"),
            owner: engine.owner.clone(),
            regional: None,
            dehaze_basis: None,
        };
        let mut graph = Graph::default();
        let input = graph
            .add(
                Operation::RenderedInput {
                    data: DataType::CalibratedLayers,
                },
                &[],
            )
            .unwrap();
        let space = crate::color_space::ColorSpace::Srgb;
        let color = graph
            .add(
                Operation::Color {
                    params: bytemuck::Zeroable::zeroed(),
                    space,
                },
                &[input],
            )
            .unwrap();
        let target = graph
            .add(
                Operation::ImageAlias {
                    dehaze_basis: None,
                    data: DataType::LinearRgb(space),
                    output: "cancel.target",
                },
                &[color],
            )
            .unwrap();
        let checks = std::cell::Cell::new(0);
        let cancel = || {
            let n = checks.get() + 1;
            checks.set(n);
            n == 3
        };
        let mut execution = Execution {
            cancel: Some(&cancel),
            ..Default::default()
        };
        let mut profile = Profile::new(Mode::Gpu);
        let result = engine.run_graph(
            Input::Rendered(&image),
            &graph,
            &[target],
            &mut profile,
            &mut execution,
        );
        assert!(result.is_err());
        assert!(profile
            .nodes
            .iter()
            .any(|node| node.operation == "Color" && node.executed));
        assert!(profile
            .nodes
            .iter()
            .all(|node| node.shared_gpu_start.is_none() && node.shared_gpu_end.is_none()));
    }

    #[test]
    #[ignore = "requires a GPU"]
    fn cached_main_and_missing_side_preserve_main_and_report_partial_publication() {
        let engine = Engine::new().unwrap();
        let extent = [8, 8];
        let image = RenderedImage {
            buffer: engine
                .gpu
                .upload("linear fixture", &[100_i32; 8 * 8 * 3])
                .unwrap(),
            description: Description::new(extent, Space::LinearCodeRgb),
            view: View {
                crop: [0, 0, 8, 8],
                rotation: 0,
                extent,
            },
            metadata: Arc::new(Metadata {
                processing_version: None,
                width: 8,
                height: 8,
                rotation: 0,
                properties: Default::default(),
                matrices: Default::default(),
                lists: Default::default(),
            }),
            settings: Settings::new("Sunlight", "Standard"),
            owner: engine.owner.clone(),
            regional: None,
            dehaze_basis: None,
        };
        let mut graph = Graph::default();
        let input = graph
            .add(
                Operation::RenderedInput {
                    data: DataType::LinearCodeRgb,
                },
                &[],
            )
            .unwrap();
        let prepared = root_detail::Prepared::new(extent, [0, 0, 8, 8], 0.5, 100, 1.).unwrap();
        let main = graph
            .add(Operation::RootDetail(prepared), &[input])
            .unwrap();
        let side = graph.named_output(main, "detail.signal").unwrap();
        graph.retain_for_session(main);
        let mut cache = Cache::default();
        let mut execution = Execution {
            cache: Some(&mut cache),
            ..Default::default()
        };
        execution.plan(&graph, &[main]).unwrap();
        let retained = engine
            .run_graph(
                Input::Rendered(&image),
                &graph,
                &[main],
                &mut Profile::default(),
                &mut execution,
            )
            .unwrap()
            .remove(0);
        execution.plan(&graph, &[main, side]).unwrap();
        let mut profile = Profile::new(Mode::Wall);
        let results = engine
            .run_graph(
                Input::Rendered(&image),
                &graph,
                &[main, side],
                &mut profile,
                &mut execution,
            )
            .unwrap();
        assert!(retained.buffer.shares_storage(&results[0].buffer));
        let observation = profile
            .nodes
            .iter()
            .find(|node| node.operation == "RootDetail")
            .unwrap();
        assert!(observation.executed);
        assert_eq!(observation.outputs.len(), 2);
        let first = &observation.outputs[0];
        assert!(
            first.required
                && first.cached_available
                && !first.requested_for_execution
                && !first.published
        );
        let second = &observation.outputs[1];
        assert!(
            second.required
                && !second.cached_available
                && second.requested_for_execution
                && second.published
        );
    }
}
