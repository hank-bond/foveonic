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

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScalarParams {
    source: [u32; 4],
    value: [f32; 4],
}

pub(super) enum Input<'a> {
    Source(&'a Source),
    Rendered(&'a RenderedImage),
}

impl Input<'_> {
    fn metadata(&self) -> &Metadata {
        match self {
            Self::Source(source) => &source.metadata,
            Self::Rendered(image) => image.source_metadata(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Value {
    pub(crate) buffer: crate::gpu::ReadGuard,
    pub(crate) description: Option<Description>,
}

impl Engine {
    pub(super) fn execute(
        &self,
        context: Input<'_>,
        graph: &Graph,
        targets: &[usize],
        profile: &mut Profile,
    ) -> Result<Vec<Value>> {
        self.execute_controlled(context, graph, targets, profile, &mut Execution::default())
    }

    pub(super) fn execute_controlled(
        &self,
        context: Input<'_>,
        graph: &Graph,
        targets: &[usize],
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<Vec<Value>> {
        let gpu = &self.gpu;
        let mut trace = gpu.trace(profile.mode.gpu(), graph.nodes.len() as u32 * 2);
        let mut scratch = Scratch::default();
        let mut values: Vec<Option<Value>> = graph
            .outputs
            .iter()
            .map(|output| execution.seed(&output.cache_name()))
            .collect();
        let cached: Vec<bool> = values.iter().map(Option::is_some).collect();
        let required = if cached.iter().any(|cached| *cached) {
            graph.required_until(targets, &cached)?
        } else {
            graph.required(targets)?
        };
        // A cached main output does not guarantee that a side output is cached.
        // Request only what is missing so a partial run cannot replace retained values.
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
        let mut stage_completed = 0;
        if stage_total == 0 {
            if let Some(progress) = execution.progress {
                progress(super::RenderProgress::Stage {
                    completed: 0,
                    total: 0,
                });
            }
        }
        for (node_index, node) in graph.nodes.iter().enumerate() {
            execution.check()?;
            if !node.outputs.iter().any(|id| required[*id] && !cached[*id]) {
                continue;
            }
            let index = node.outputs[0];
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
                .unwrap_or_else(|| match &context {
                    Input::Source(source) => source.storage_extent().map(|v| v as u32),
                    Input::Rendered(image) => image.description.extent,
                });
            let extent = match &node.operation {
                Operation::ReduceLayers(prepared) => prepared.output_extent(),
                Operation::ToneMask { guide, .. }
                | Operation::PhotographicHdrMask { guide, .. } => guide.extent(),
                Operation::ImageRegion { prepared, .. } => prepared.extent(),
                Operation::ProjectColorTiles { params, .. } => params.output_extent(),
                Operation::NoiseContext(p) => reconstruction::context_extent(*p),
                Operation::PackedLab(p) => p.extent(),
                Operation::CodeCrop(p) => p.extent(),
                Operation::WaveletContrast(p) => p.extent(),
                Operation::CodeChannelMap(p) => p.extent(),
                Operation::ChannelNoise(p) => p.extent(),
                Operation::ColorSharpen(p) => p.extent(),
                Operation::ProportionalSharpen(p) => p.extent(),
                Operation::Encode { params, .. } => [params.output[0], params.output[1]],
                Operation::BlueGuidedReconstruct(p) => p.output_extent(),
                _ => inherited,
            };
            let view_baked = matches!(
                node.operation,
                Operation::CodeCrop(_) | Operation::Encode { .. }
            ) || reads[node_index].iter().any(|id| {
                values[*id]
                    .as_ref()
                    .and_then(|v| v.description)
                    .is_some_and(|d| d.view_baked)
            }) || matches!((&node.operation, &context), (Operation::RenderedInput { .. }, Input::Rendered(image)) if image.description.view_baked);
            let desc = graph.outputs[index]
                .data
                .image_description(extent)
                .map(|mut d| {
                    d.view_baked = view_baked;
                    d
                });
            let groups = [inherited[0].div_ceil(8), inherited[1].div_ceil(8)];
            let dimensions = [inherited[0], inherited[1], 0, 0];
            let bytes = u64::from(inherited[0]) * u64::from(inherited[1]) * 12;
            let mut dispatch = |kernel, buffers: &mut [(u32, crate::gpu::Binding<'_>)], groups| {
                gpu.dispatch(kernel, buffers, groups, &mut trace)
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
                Operation::ReduceLayers(prepared) => {
                    Some(crate::nodes::color::pyramid_resample::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        profile,
                    )?)
                }
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
                    Some(gpu.upload("Original sensor layers", &source.pixels)?)
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
                Operation::PackedLab(prepared) => Some(crate::nodes::color::packed_lab::apply(
                    gpu,
                    &input(0).buffer,
                    prepared,
                    profile,
                )?),
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
                        prepared,
                        profile,
                    )?;
                    if let Some(&side) = node.outputs.get(1) {
                        if required[side] {
                            values[side] = Some(Value {
                                buffer: output.mask,
                                description: None,
                            });
                        }
                    }
                    if let Some(&side) = node.outputs.get(2) {
                        if required[side] {
                            values[side] = Some(Value {
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
                        || input(3).description.map(|d| d.extent) != Some(inherited.map(|v| v * 2))
                    {
                        return Err("blue-guided reconstruction requires three matching RGB cells and a doubled blue plane".into());
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
                            buffer: output.residual,
                            description: Some(Description::new(
                                extent,
                                crate::image::Space::LuminanceRatio,
                            )),
                        });
                    }
                    required[index].then_some(output.rgb)
                }
                Operation::ResidualFinish { params, with_mask } => {
                    if inherited != params.extent()
                        || input(1).description.map(|d| d.extent) != Some(inherited)
                    {
                        return Err(
                            "residual finishing requires matching RGB and gain extents".into()
                        );
                    }
                    let output = crate::nodes::color::residual_finish::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        if *with_mask {
                            Some(&input(2).buffer)
                        } else {
                            None
                        },
                        params,
                        profile,
                    )?;
                    if required[node.outputs[1]] {
                        values[node.outputs[1]] = Some(Value {
                            buffer: output.detail,
                            description: Some(Description::new(
                                inherited,
                                crate::image::Space::SignedDetail,
                            )),
                        });
                    }
                    required[index].then_some(output.rgb)
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
                        || input(1).description.map(|d| d.extent) != Some(inherited.map(|v| v * 2))
                    {
                        return Err(
                            "sparse repair requires RGB cells and a doubled blue plane".into()
                        );
                    }
                    let output = crate::nodes::sensor::sparse_repair::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        prepared,
                        profile,
                    )?;
                    for (side, buffer, extent) in [
                        (node.outputs[1], output.blue, inherited.map(|v| v * 2)),
                        (node.outputs[2], output.defects, inherited),
                    ] {
                        if required[side] {
                            values[side] = Some(Value {
                                buffer,
                                description: graph.outputs[side].data.image_description(extent),
                            });
                        }
                    }
                    required[index].then_some(output.rgb)
                }
                Operation::HlpCalibration(prepared) => {
                    if inherited != prepared.extent()
                        || input(1).description.map(|d| d.extent) != Some(inherited.map(|v| v * 2))
                    {
                        return Err(
                            "sparse calibration requires RGB cells and a doubled blue plane".into(),
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
                        (node.outputs[1], output.blue, inherited.map(|v| v * 2)),
                        (node.outputs[2], output.scratch, prepared.scratch_extent()),
                    ] {
                        if required[side] {
                            values[side] = Some(Value {
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
                            buffer: blue,
                            description: graph.outputs[side]
                                .data
                                .image_description(inherited.map(|v| v * 2)),
                        });
                    }
                    required[index].then_some(rgb)
                }
                Operation::RenderedInput { data } => {
                    let Input::Rendered(image) = &context else {
                        return Err("rendered input context required".into());
                    };
                    if data
                        .image_description(image.description.extent)
                        .map(|mut d| {
                            d.view_baked = image.description.view_baked;
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
                Operation::LinearCodeBase => Some(input(0).buffer.clone()),
                Operation::CodeResponse { prepared, .. } => {
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
                Operation::Column { params, enabled } => {
                    let tiles = if *enabled {
                        reconstruction::column(gpu, &input(0).buffer, *params, profile)?
                    } else {
                        reconstruction::raw_tiles(gpu, &input(0).buffer, *params, profile)?
                    };
                    let projected = node.outputs[1];
                    if required[projected] {
                        values[projected] = Some(Value {
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
                            buffer: reconstruction::project(gpu, &tiles, *params, 3, profile)?,
                            description: graph.outputs[projected]
                                .data
                                .image_description(params.output_extent()),
                        });
                    }
                    required[index].then_some(tiles)
                }
                Operation::Banding(prepared) => {
                    let result =
                        banding::apply(gpu, &input(0).buffer, prepared, required[index], profile)?;
                    let rows = node.outputs[1];
                    if required[rows] {
                        values[rows] = Some(Value {
                            buffer: result.rows,
                            description: None,
                        });
                    }
                    result.image
                }
                Operation::Highlights(prepared) => {
                    highlight::apply(gpu, Some(&input(0).buffer), prepared, false, profile)?.pixels
                }
                Operation::ColorField(prepared) => Some(color_field::apply(
                    gpu,
                    color_field::Inputs {
                        input: &input(0).buffer,
                        original: &input(1).buffer,
                        black: &input(2).buffer,
                        unrepaired: &input(3).buffer,
                    },
                    prepared,
                    profile,
                    execution,
                )?),
                Operation::HighlightDensity(prepared) => {
                    highlight::apply(gpu, None, prepared, true, profile)?.density
                }
                Operation::Despeckle(prepared) => {
                    Some(despeckle::apply(gpu, &input(0).buffer, prepared, profile)?)
                }
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
                        Kernel::Color,
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
                Operation::ImageAlias { .. } => Some(input(0).buffer.clone()),
                Operation::MonochromeSensitivity { params, .. } => {
                    if input(0).description.unwrap().extent != input(1).description.unwrap().extent
                    {
                        return Err("monochrome guide and image have different extents".into());
                    }
                    Some(crate::nodes::finish::photographic::sensitivity::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::PhotographicTone { params, .. } => {
                    Some(crate::nodes::finish::photographic::tone::apply(
                        gpu,
                        &input(0).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::UniversalSharpen { params, .. } => {
                    Some(crate::nodes::finish::photographic::sharpen::apply(
                        gpu,
                        &input(0).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::LocalContrast { params, .. } => {
                    Some(crate::nodes::finish::photographic::local_contrast::apply(
                        gpu,
                        &input(0).buffer,
                        *params,
                        inherited,
                        profile,
                    )?)
                }
                Operation::PhotographicHdrMask { guide, .. } => {
                    Some(crate::nodes::finish::tone_equalizer::mask(
                        gpu,
                        &input(0).buffer,
                        *guide,
                        profile,
                    )?)
                }
                Operation::PhotographicHdr { controls, space } => {
                    Some(crate::nodes::finish::tone_equalizer::apply_photographic(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        inherited,
                        crate::nodes::finish::tone_equalizer::Response {
                            controls: *controls,
                            pivot: (0.18_f32).log2(),
                            space: *space,
                        },
                        profile,
                    )?)
                }
                Operation::ToneMask { guide, .. } => {
                    Some(crate::nodes::finish::tone_equalizer::mask(
                        gpu,
                        &input(0).buffer,
                        *guide,
                        profile,
                    )?)
                }
                Operation::TonalRanges {
                    controls,
                    pivot,
                    space,
                } => Some(crate::nodes::finish::tone_equalizer::apply(
                    gpu,
                    &input(0).buffer,
                    &input(1).buffer,
                    inherited,
                    crate::nodes::finish::tone_equalizer::Response {
                        controls: *controls,
                        pivot: *pivot,
                        space: *space,
                    },
                    profile,
                )?),
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
                Operation::SharpSignal { .. } => {
                    let mut output = gpu.storage(
                        "Native signed mono high-pass",
                        u64::from(inherited[0]) * u64::from(inherited[1]) * 4,
                    )?;
                    let uniform = gpu.uniform(&ScalarParams {
                        source: dimensions,
                        value: [0.; 4],
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
                Operation::Tiff {
                    path,
                    params,
                    space,
                } => {
                    let bytes = profile
                        .measure("readback wait and copy", || gpu.readback(&input(0).buffer))?;
                    let file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)?;
                    crate::output::write_tiff(
                        std::io::BufWriter::new(file),
                        &bytes,
                        params,
                        *space,
                        profile,
                    )?;
                    None
                }
            };
            if !matches!(node.operation, Operation::Tiff { .. }) {
                profile.record(node.operation.label(), start);
            }
            if !cached[index] {
                values[index] = output.map(|buffer| Value {
                    buffer,
                    description: desc,
                });
            }
            for &id in &node.outputs {
                if let Some(value) = values[id].as_ref() {
                    execution.store(&graph.outputs[id].cache_name(), value);
                }
            }
            // Queuing a whole outdated render would delay both drawing and newer edits.
            // Finish submitted work off-thread, then check for newer work between stages.
            // Background executors also yield at GPU submission boundaries.
            if execution.cancel.is_some() {
                gpu.device.poll(wgpu::Maintain::Wait);
            }
            stage_completed += 1;
            if let Some(progress) = execution.progress {
                progress(super::RenderProgress::Stage {
                    completed: stage_completed,
                    total: stage_total,
                });
            }
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
        }
        gpu.finish_trace(&trace, profile)?;
        if profile.mode.enabled() {
            profile.scratch_allocations += scratch.allocations;
            profile.scratch_reuses += scratch.reuses;
        }
        Ok(targets.iter().filter_map(|id| values[*id].take()).collect())
    }
}
