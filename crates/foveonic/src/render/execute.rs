//! Executes typed graph operations with selective outputs and guarded GPU storage.

use super::Engine;
use crate::nodes::{
    color::{pattern, reconstruction},
    sensor::{banding, camera_repair, despeckle, highlight},
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
    image::{Description, Layout, RenderedImage, Space},
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

fn description(data: DataType, extent: [u32; 2]) -> Option<Description> {
    let (layout, space) = match data {
        DataType::SensorLayers => (Layout::Rgb32Float, Space::SensorLayers),
        DataType::ScalarField => (Layout::Scalar32Float, Space::ScalarField),
        DataType::Monochrome => (Layout::Scalar32Float, Space::Monochrome),
        DataType::LuminanceRatio => (Layout::Scalar32Float, Space::LuminanceRatio),
        DataType::SignedDetail => (Layout::Scalar32Float, Space::SignedDetail),
        DataType::CalibratedLayers => (Layout::Rgb32Float, Space::CalibratedLayers),
        DataType::LinearRgb(space) => (Layout::Rgb32Float, Space::LinearRgb(space)),
        DataType::ToneMappedRgb(space) => (Layout::Rgb32Float, Space::ToneMappedRgb(space)),
        DataType::EncodedRgb16(space) => (Layout::PackedRgb16, Space::EncodedRgb16(space)),
        DataType::BlackLevels
        | DataType::Histogram
        | DataType::RowTable
        | DataType::File
        | DataType::FilteredTiles
        | DataType::ConditionedTiles
        | DataType::PatternSeedTiles
        | DataType::UnconditionedRatioTiles
        | DataType::ResidualTiles
        | DataType::RatioTiles
        | DataType::ColorTiles(_) => return None,
    };
    Some(Description {
        extent,
        layout,
        space,
    })
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
            .map(|output| execution.seed(output.name))
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
                    Input::Source(source) => {
                        [source.metadata.width as u32, source.metadata.height as u32]
                    }
                    Input::Rendered(image) => image.description.extent,
                });
            let extent = match &node.operation {
                Operation::Encode { params, .. } => [params.output[0], params.output[1]],
                _ => inherited,
            };
            let desc = description(graph.outputs[index].data, extent);
            let groups = [inherited[0].div_ceil(8), inherited[1].div_ceil(8)];
            let dimensions = [inherited[0], inherited[1], 0, 0];
            let bytes = u64::from(inherited[0]) * u64::from(inherited[1]) * 12;
            let mut dispatch = |kernel, buffers: &mut [(u32, crate::gpu::Binding<'_>)], groups| {
                gpu.dispatch(kernel, buffers, groups, &mut trace)
            };
            let output = match &node.operation {
                Operation::Source => {
                    let Input::Source(source) = &context else {
                        return Err("sensor input context required".into());
                    };
                    Some(gpu.upload("Original sensor layers", &source.pixels)?)
                }
                Operation::RenderedInput { data } => {
                    let Input::Rendered(image) = &context else {
                        return Err("rendered input context required".into());
                    };
                    if description(*data, image.description.extent) != Some(image.description)
                        || image.buffer.size() < image.description.byte_len()
                    {
                        return Err(
                            "imported image representation does not match graph input".into()
                        );
                    }
                    Some(image.buffer.clone())
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
                Operation::NoiseBase(params) => {
                    let projected = node.outputs[1];
                    let edge_id = node.outputs[2];
                    let base = reconstruction::base(
                        gpu,
                        &input(0).buffer,
                        *params,
                        required[edge_id],
                        required[index] || required[projected],
                        required[node.outputs[3]],
                        profile,
                    )?;
                    if required[projected] {
                        values[projected] = Some(Value {
                            buffer: reconstruction::project(
                                gpu,
                                base.tiles.as_ref().unwrap(),
                                *params,
                                3,
                                false,
                                profile,
                            )?,
                            description: description(DataType::CalibratedLayers, inherited),
                        });
                    }
                    if required[edge_id] {
                        values[edge_id] = Some(Value {
                            buffer: reconstruction::project(
                                gpu,
                                base.edges.as_ref().unwrap(),
                                *params,
                                1,
                                false,
                                profile,
                            )?,
                            description: description(DataType::ScalarField, inherited),
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
                Operation::Column(params) => {
                    let tiles = reconstruction::column(gpu, &input(0).buffer, *params, profile)?;
                    let projected = node.outputs[1];
                    if required[projected] {
                        values[projected] = Some(Value {
                            buffer: reconstruction::project(
                                gpu, &tiles, *params, 3, false, profile,
                            )?,
                            description: description(DataType::CalibratedLayers, inherited),
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
                            buffer: reconstruction::project(
                                gpu, &ratio, *params, 1, false, profile,
                            )?,
                            description: description(DataType::LuminanceRatio, inherited),
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
                                false,
                                profile,
                            )?,
                            description: description(DataType::LuminanceRatio, inherited),
                        });
                    }
                    if required[mask] {
                        values[mask] = Some(Value {
                            buffer: reconstruction::project(
                                gpu,
                                result.mask.as_ref().unwrap(),
                                *params,
                                1,
                                false,
                                profile,
                            )?,
                            description: description(DataType::ScalarField, inherited),
                        });
                    }
                    if required[index] {
                        result.ratio
                    } else {
                        None
                    }
                }
                Operation::Detail(params) => Some(reconstruction::mix(
                    gpu,
                    &input(0).buffer,
                    &input(1).buffer,
                    &input(2).buffer,
                    *params,
                    profile,
                )?),
                Operation::TiledColor { params, color, .. } => {
                    let tiles =
                        reconstruction::color(gpu, &input(0).buffer, *color, *params, profile)?;
                    let projected = node.outputs[1];
                    if required[projected] {
                        values[projected] = Some(Value {
                            buffer: reconstruction::project(
                                gpu, &tiles, *params, 3, false, profile,
                            )?,
                            description: description(graph.outputs[projected].data, inherited),
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
                            buffer: reconstruction::project(
                                gpu, &tiles, *params, 3, false, profile,
                            )?,
                            description: description(graph.outputs[projected].data, inherited),
                        });
                    }
                    required[index].then_some(tiles)
                }
                Operation::Reconstruct { params, .. } => {
                    let signal_id = node.outputs[1];
                    let signal = if required[signal_id] {
                        Some(reconstruction::signal(
                            gpu,
                            &input(0).buffer,
                            &input(2).buffer,
                            *params,
                            profile,
                        )?)
                    } else {
                        None
                    };
                    let rgb = if required[index] {
                        Some(reconstruction::restore(
                            gpu,
                            &input(0).buffer,
                            &input(1).buffer,
                            *params,
                            profile,
                        )?)
                    } else {
                        None
                    };
                    if let Some(buffer) = signal {
                        values[signal_id] = Some(Value {
                            buffer,
                            description: description(DataType::SignedDetail, inherited),
                        });
                    }
                    rgb
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
                    execution.store(graph.outputs[id].name, value);
                }
            }
            // Queuing a whole outdated render would delay both drawing and newer edits.
            // Finish submitted work off-thread, then check for newer work between stages.
            // Batch rendering has no interactive queue to protect and can stay asynchronous.
            if execution.cancel.is_some() {
                gpu.device.poll(wgpu::Maintain::Wait);
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
