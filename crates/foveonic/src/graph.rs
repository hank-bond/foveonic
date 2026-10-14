//! Builds typed, dependency-ordered image graphs and validates their connections.

use crate::calibration::{Calibration, ColorParams, ColorSpace, EncodeParams};
use crate::source::Result;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    SensorLayers,
    ScalarField,
    LuminanceRatio,
    SignedDetail,
    FilteredTiles,
    ConditionedTiles,
    PatternSeedTiles,
    UnconditionedRatioTiles,
    ResidualTiles,
    RatioTiles,
    ColorTiles(ColorSpace),
    BlackLevels,
    Histogram,
    RowTable,
    CalibratedLayers,
    LinearRgb(ColorSpace),
    ToneMappedRgb(ColorSpace),
    EncodedRgb16(ColorSpace),
    File,
}

impl From<crate::image::Space> for DataType {
    fn from(space: crate::image::Space) -> Self {
        use crate::image::Space;
        match space {
            Space::SensorLayers => Self::SensorLayers,
            Space::ScalarField => Self::ScalarField,
            Space::LuminanceRatio => Self::LuminanceRatio,
            Space::SignedDetail => Self::SignedDetail,
            Space::CalibratedLayers => Self::CalibratedLayers,
            Space::LinearRgb(space) => Self::LinearRgb(space),
            Space::ToneMappedRgb(space) => Self::ToneMappedRgb(space),
            Space::EncodedRgb16(space) => Self::EncodedRgb16(space),
        }
    }
}

pub enum Operation {
    Source,
    RenderedInput {
        data: DataType,
    },
    Black(crate::calibration::CalibrationParams),
    Calibrate(Calibration),
    Color {
        params: ColorParams,
        space: ColorSpace,
    },
    Repair(crate::camera_repair::Prepared),
    Despeckle(crate::despeckle::Prepared),
    Highlights(std::sync::Arc<crate::highlight::Prepared>),
    HighlightDensity(std::sync::Arc<crate::highlight::Prepared>),
    Banding(crate::banding::Prepared),
    NoiseBase(crate::noise::Parameters),
    Column(crate::noise::Parameters),
    Ratio(crate::noise::Parameters),
    Pattern {
        params: crate::noise::Parameters,
        prepared: crate::pattern::Prepared,
    },
    Detail(crate::noise::Parameters),
    TiledColor {
        params: crate::noise::Parameters,
        color: ColorParams,
        space: ColorSpace,
    },
    TiledColorDq {
        params: crate::noise::Parameters,
        table: crate::color_dq::Table,
        space: ColorSpace,
    },
    Reconstruct {
        params: crate::noise::Parameters,
        space: ColorSpace,
    },
    ColorDq {
        table: crate::color_dq::Table,
        space: ColorSpace,
    },
    SensorGain {
        factor: f32,
    },
    Saturation {
        params: ColorParams,
        space: ColorSpace,
    },
    SharpSignal {
        space: ColorSpace,
    },
    Sharpen {
        amount: f32,
        space: ColorSpace,
    },
    Tone {
        curve: Vec<f32>,
        exposure: f32,
        space: ColorSpace,
    },
    Encode {
        params: EncodeParams,
        space: ColorSpace,
    },
    Tiff {
        path: PathBuf,
        params: EncodeParams,
        space: ColorSpace,
    },
}

impl Operation {
    /// Let each processing stage decide which settings affect its result.
    /// Track inputs separately for each output so side outputs can be reused safely.
    pub(crate) fn key(&self, output: &str) -> crate::recipe::Key {
        use crate::{noise::RecipeStage as N, recipe::Key};
        match self {
            Self::Source | Self::RenderedInput { .. } | Self::SharpSignal { .. } => Key::default(),
            Self::Black(p) => Key::pod(p),
            Self::Calibrate(p) => p.key(),
            Self::Color { params, .. } | Self::Saturation { params, .. } => Key::pod(params),
            Self::Repair(p) => p.key(),
            Self::Despeckle(p) => p.key(),
            Self::Highlights(p) | Self::HighlightDensity(p) => p.key(),
            Self::Banding(p) => p.key(),
            Self::NoiseBase(p) => p.key(N::Base),
            Self::Column(p) => p.key(N::Column),
            Self::Ratio(p) => p.key(N::Ratio),
            Self::Pattern { params, prepared } => params.key(N::Geometry).join(prepared.key()),
            Self::Detail(p) => p.key(N::Detail),
            Self::TiledColor { params, color, .. } => params.key(N::Geometry).with(color),
            Self::TiledColorDq { params, table, .. } => params.key(N::Geometry).join(table.key()),
            Self::Reconstruct { params, .. } => params.key(if output == "detail.signal" {
                N::Signal
            } else {
                N::Geometry
            }),
            Self::ColorDq { table, .. } => table.key(),
            Self::SensorGain { factor } => Key::pod(factor),
            Self::Sharpen { amount, .. } => Key::pod(amount),
            Self::Tone {
                curve, exposure, ..
            } => Key::pod(exposure).slice(curve),
            Self::Encode { params, .. } | Self::Tiff { params, .. } => Key::pod(params),
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Source => "upload sensor",
            Self::RenderedInput { .. } => "import rendered image",
            Self::Black(_) => "submit black",
            Self::Calibrate(_) => "submit calibration",
            Self::Color { .. } => "submit color",
            Self::Repair(_) => "submit repair",
            Self::Despeckle(_) => "submit despeckle",
            Self::Highlights(_) => "submit highlight restoration",
            Self::HighlightDensity(_) => "submit original saturation density",
            Self::Banding(_) => "submit row banding correction",
            Self::NoiseBase(_) => "submit reference noise base",
            Self::Column(_) => "submit reference column conditioning",
            Self::Ratio(_) => "submit reference luminance ratio",
            Self::Pattern { .. } => "submit reference pattern conditioning",
            Self::Detail(_) => "submit reference detail",
            Self::TiledColor { .. } => "submit tiled color",
            Self::TiledColorDq { .. } => "submit tiled ColorDQ",
            Self::Reconstruct { .. } => "submit reconstruction",
            Self::ColorDq { .. } => "submit ColorDQ",
            Self::SensorGain { .. } => "submit sensor gain",
            Self::Saturation { .. } => "submit saturation",
            Self::SharpSignal { .. } => "submit native high-pass",
            Self::Sharpen { .. } => "submit sharpen",
            Self::Tone { .. } => "submit tone",
            Self::Encode { .. } => "submit encode",
            Self::Tiff { .. } => "TIFF",
        }
    }

    fn types(&self) -> (Vec<DataType>, DataType) {
        use DataType::*;
        match self {
            Self::Source => (vec![], SensorLayers),
            Self::RenderedInput { data } => (vec![], *data),
            Self::Black(_) => (vec![SensorLayers], BlackLevels),
            Self::Calibrate(_) => (vec![SensorLayers, BlackLevels], CalibratedLayers),
            Self::Color { space, .. } => (vec![CalibratedLayers], LinearRgb(*space)),
            Self::Repair(_) => (vec![CalibratedLayers, SensorLayers], CalibratedLayers),
            Self::Despeckle(_) | Self::Highlights(_) | Self::Banding(_) => {
                (vec![CalibratedLayers], CalibratedLayers)
            }
            Self::HighlightDensity(_) => (vec![], ScalarField),
            Self::NoiseBase(_) => (vec![CalibratedLayers], FilteredTiles),
            Self::Column(_) => (vec![CalibratedLayers], ConditionedTiles),
            Self::Ratio(_) => (
                vec![ConditionedTiles, FilteredTiles],
                UnconditionedRatioTiles,
            ),
            Self::Pattern { .. } => (vec![UnconditionedRatioTiles, PatternSeedTiles], RatioTiles),
            Self::Detail(_) => (
                vec![ConditionedTiles, FilteredTiles, RatioTiles],
                ResidualTiles,
            ),
            Self::TiledColor { space, .. } => (vec![FilteredTiles], ColorTiles(*space)),
            Self::TiledColorDq { space, .. } => (vec![ColorTiles(*space)], ColorTiles(*space)),
            Self::Reconstruct { space, .. } => (
                vec![ColorTiles(*space), ResidualTiles, RatioTiles],
                LinearRgb(*space),
            ),
            Self::SensorGain { .. } => (vec![CalibratedLayers], CalibratedLayers),
            Self::ColorDq { space, .. } => (vec![LinearRgb(*space)], LinearRgb(*space)),
            Self::Saturation { space, .. } => (vec![ToneMappedRgb(*space)], ToneMappedRgb(*space)),
            Self::SharpSignal { space } => (vec![LinearRgb(*space)], SignedDetail),
            Self::Sharpen { space, .. } => {
                (vec![LinearRgb(*space), SignedDetail], LinearRgb(*space))
            }
            Self::Tone { space, .. } => (vec![LinearRgb(*space)], ToneMappedRgb(*space)),
            Self::Encode { space, .. } => (vec![ToneMappedRgb(*space)], EncodedRgb16(*space)),
            Self::Tiff { space, .. } => (vec![EncodedRgb16(*space)], File),
        }
    }
}

pub struct Node {
    pub operation: Operation,
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
}

pub struct Output {
    pub producer: usize,
    pub data: DataType,
    pub name: &'static str,
    pub dependencies: Vec<usize>,
}

#[derive(Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub outputs: Vec<Output>,
}

impl Graph {
    /// Requiring existing inputs prevents cycles and keeps execution order clear.
    pub fn add(&mut self, operation: Operation, inputs: &[usize]) -> Result<usize> {
        let (expected, output) = operation.types();
        if inputs.len() != expected.len() {
            return Err("wrong number of node inputs".into());
        }
        for (&input, expected) in inputs.iter().zip(expected) {
            let output = self.outputs.get(input).ok_or("node input does not exist")?;
            if output.data != expected {
                return Err(format!(
                    "incompatible node input: expected {expected:?}, received {:?}",
                    output.data
                )
                .into());
            }
        }
        let id = self.outputs.len();
        let producer = self.nodes.len();
        self.outputs.push(Output {
            producer,
            data: output,
            name: operation.output_name(),
            dependencies: inputs.to_vec(),
        });
        let mut outputs = vec![id];
        for (name, data, slots) in operation.side_outputs() {
            outputs.push(self.outputs.len());
            self.outputs.push(Output {
                producer,
                data,
                name,
                dependencies: slots.iter().map(|slot| inputs[*slot]).collect(),
            });
        }
        self.nodes.push(Node {
            operation,
            inputs: inputs.to_vec(),
            outputs,
        });
        Ok(id)
    }

    /// A node may run without keeping every side output. Track which outputs
    /// are needed so a cached main result does not hide a missing side result.
    pub fn required(&self, roots: &[usize]) -> Result<Vec<bool>> {
        self.required_until(roots, &vec![false; self.outputs.len()])
    }
    /// A retained result already has its inputs baked in, so traversal can stop
    /// there without changing the stage's type or depending on graph positions.
    pub(crate) fn required_until(&self, roots: &[usize], cached: &[bool]) -> Result<Vec<bool>> {
        let mut required = vec![false; self.outputs.len()];
        let mut pending = roots.to_vec();
        while let Some(id) = pending.pop() {
            let output = self
                .outputs
                .get(id)
                .ok_or("requested output does not exist")?;
            if !required[id] {
                required[id] = true;
                if !cached[id] {
                    pending.extend_from_slice(&output.dependencies);
                }
            }
        }
        Ok(required)
    }
}

impl Operation {
    fn side_outputs(&self) -> Vec<(&'static str, DataType, Vec<usize>)> {
        use DataType::*;
        match self {
            Self::Black(_) => vec![("black.histogram", Histogram, vec![0])],
            Self::Banding(_) => vec![("banding.rows", RowTable, vec![0])],
            Self::NoiseBase(_) => vec![
                ("noise.layers", CalibratedLayers, vec![0]),
                ("noise.edges", ScalarField, vec![0]),
                ("noise.pattern-seed-tiles", PatternSeedTiles, vec![0]),
            ],
            Self::Column(_) => vec![("column.layers", CalibratedLayers, vec![0])],
            Self::Ratio(_) => vec![("detail.raw-ratio", LuminanceRatio, vec![0, 1])],
            Self::Pattern { .. } => vec![
                ("detail.ratio", LuminanceRatio, vec![0, 1]),
                ("pattern.mask", ScalarField, vec![0, 1]),
            ],
            Self::TiledColor { space, .. } => vec![("color.rgb", LinearRgb(*space), vec![0])],
            Self::TiledColorDq { space, .. } => vec![("color-dq.rgb", LinearRgb(*space), vec![0])],
            Self::Reconstruct { .. } => vec![("detail.signal", SignedDetail, vec![0, 2])],
            _ => Vec::new(),
        }
    }

    pub fn output_name(&self) -> &'static str {
        match self {
            Self::Source => "sensor.layers",
            Self::RenderedInput { .. } => "import.image",
            Self::Black(_) => "black.levels",
            Self::Calibrate(_) => "calibration.layers",
            Self::Repair(_) => "repair.layers",
            Self::Despeckle(_) => "despeckle.layers",
            Self::Highlights(_) => "highlight.layers",
            Self::HighlightDensity(_) => "highlight.density",
            Self::Banding(_) => "banding.layers",
            Self::NoiseBase(_) => "noise.tiles",
            Self::Column(_) => "column.tiles",
            Self::Ratio(_) => "detail.raw-ratio-tiles",
            Self::Pattern { .. } => "detail.ratio-tiles",
            Self::Detail(_) => "detail.gain-tiles",
            Self::TiledColor { .. } => "color.tiles",
            Self::TiledColorDq { .. } => "color-dq.tiles",
            Self::Reconstruct { .. } => "reconstruction.rgb",
            Self::Color { .. } => "color.rgb",
            Self::ColorDq { .. } => "color-dq.rgb",
            Self::SensorGain { .. } => "gain.layers",
            Self::Tone { .. } => "tone.rgb",
            Self::Saturation { .. } => "saturation.rgb",
            Self::SharpSignal { .. } => "detail.signal",
            Self::Sharpen { .. } => "sharpen.rgb",
            Self::Encode { .. } => "export.rgb16",
            Self::Tiff { .. } => "export.file",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn demand_tracks_ports_without_requiring_siblings_or_unrelated_nodes() {
        let mut graph = Graph::default();
        let source = graph.add(Operation::Source, &[]).unwrap();
        let black = graph
            .add(
                Operation::Black(crate::calibration::CalibrationParams {
                    source: [1, 1, 1, 0],
                    dark: [0, 0, 1, 1],
                    gains: [1.; 4],
                    shading: [0.; 4],
                    shading_geometry: [0., 0., 1., 0.],
                    grids: [[0, 1, 1, 0]; 3],
                }),
                &[source],
            )
            .unwrap();
        let histogram = black + 1;
        let unrelated = graph.add(Operation::Source, &[]).unwrap();
        let required = graph.required(&[histogram]).unwrap();
        assert!(required[source]);
        assert!(required[histogram]);
        assert!(!required[black]);
        assert!(!required[unrelated]);
        let required = graph.required(&[black, histogram]).unwrap();
        assert!(required[black] && required[histogram]);
        assert!(graph.required(&[999]).is_err());
    }

    #[test]
    fn reference_outputs_have_stage_local_dependencies_and_typed_atlases() {
        let mut graph = Graph::default();
        let params = <crate::noise::Parameters as bytemuck::Zeroable>::zeroed();
        let raw = graph
            .add(
                Operation::RenderedInput {
                    data: DataType::CalibratedLayers,
                },
                &[],
            )
            .unwrap();
        let base = graph.add(Operation::NoiseBase(params), &[raw]).unwrap();
        let conditioned = graph.add(Operation::Column(params), &[raw]).unwrap();
        let ratio = graph
            .add(Operation::Ratio(params), &[conditioned, base])
            .unwrap();
        let pattern = graph
            .add(
                Operation::Pattern {
                    params,
                    prepared: crate::pattern::Prepared::new([40, 40, 1, 4], [0.025, 0.025, 0., 0.])
                        .unwrap(),
                },
                &[ratio, base + 3],
            )
            .unwrap();
        let detail = graph
            .add(Operation::Detail(params), &[conditioned, base, pattern])
            .unwrap();
        let colour = graph
            .add(
                Operation::TiledColor {
                    params,
                    color: ColorParams {
                        source: [0; 4],
                        matrix: [[0.; 4]; 3],
                    },
                    space: ColorSpace::Srgb,
                },
                &[base],
            )
            .unwrap();
        let restored = graph
            .add(
                Operation::Reconstruct {
                    params,
                    space: ColorSpace::Srgb,
                },
                &[colour, detail, pattern],
            )
            .unwrap();
        let required = graph.required(&[restored + 1]).unwrap();
        assert!(required[raw] && required[base] && required[colour] && required[pattern]);
        assert!(!required[detail] && !required[restored]);
        assert_eq!(graph.outputs[restored + 1].data, DataType::SignedDetail);
        let masks = graph.required(&[pattern + 2]).unwrap();
        assert!(masks[ratio] && masks[base + 3]);
        assert!(!masks[pattern] && !masks[detail]);
        let raw_ratio = graph.required(&[ratio + 1]).unwrap();
        assert!(raw_ratio[base] && raw_ratio[conditioned]);
        assert!(!raw_ratio[pattern] && !raw_ratio[base + 3] && !raw_ratio[detail]);
        let required = graph.required(&[base + 2]).unwrap();
        assert!(required[raw]);
        assert!(!required[base] && !required[detail] && !required[colour]);
        let tone = Operation::Tone {
            curve: vec![0., 1.],
            exposure: 1.,
            space: ColorSpace::Srgb,
        };
        assert!(graph.add(tone, &[colour]).is_err()); // projection establishes full-sensor geometry
    }

    #[test]
    fn rejects_wrong_space_and_forward_dependencies() {
        let mut graph = Graph::default();
        let source = graph.add(Operation::Source, &[]).unwrap();
        let encode = || Operation::Encode {
            params: EncodeParams {
                source: [1, 1, 0, 0],
                crop: [0, 0, 1, 1],
                output: [1, 1, 0, 0],
            },
            space: ColorSpace::Srgb,
        };
        assert!(graph.add(encode(), &[source]).is_err());
        assert!(graph.add(encode(), &[10]).is_err());
        let black = crate::calibration::CalibrationParams {
            source: [1, 1, 1, 0],
            dark: [0, 0, 1, 1],
            gains: [1.; 4],
            shading: [0.; 4],
            shading_geometry: [0., 0., 1., 0.],
            grids: [[0, 1, 1, 0]; 3],
        };
        let black_id = graph.add(Operation::Black(black), &[source]).unwrap();
        let calibrated = graph
            .add(
                Operation::Calibrate(Calibration {
                    params: black,
                    grid_values: vec![1.; 3],
                }),
                &[source, black_id],
            )
            .unwrap();
        let linear = graph
            .add(
                Operation::Color {
                    params: ColorParams {
                        source: [1, 1, 0, 0],
                        matrix: [[0.; 4]; 3],
                    },
                    space: ColorSpace::Srgb,
                },
                &[calibrated],
            )
            .unwrap();
        assert!(graph
            .add(
                Operation::Repair(crate::camera_repair::Prepared::default()),
                &[linear, source]
            )
            .is_err());
        let gain = || Operation::SensorGain { factor: 1.3 };
        assert!(graph.add(gain(), &[source]).is_err()); // gain requires black-corrected layers
        assert!(graph.add(gain(), &[linear]).is_err()); // gain belongs before working RGB
        assert!(graph.add(gain(), &[calibrated]).is_ok());
        let color_dq = |space| Operation::ColorDq {
            table: crate::color_dq::table([8.; 3]).unwrap(),
            space,
        };
        assert!(graph
            .add(color_dq(ColorSpace::Srgb), &[calibrated])
            .is_err());
        assert!(graph
            .add(color_dq(ColorSpace::AdobeRgb), &[linear])
            .is_err());
        assert!(graph.add(color_dq(ColorSpace::Srgb), &[linear]).is_ok());
        let toned = graph
            .add(
                Operation::Tone {
                    curve: vec![0., 1.],
                    exposure: 1.,
                    space: ColorSpace::Srgb,
                },
                &[linear],
            )
            .unwrap();
        assert!(graph.add(color_dq(ColorSpace::Srgb), &[toned]).is_err());
        let sharp = graph
            .add(
                Operation::SharpSignal {
                    space: ColorSpace::Srgb,
                },
                &[linear],
            )
            .unwrap();
        assert!(graph
            .add(
                Operation::Sharpen {
                    amount: 1.,
                    space: ColorSpace::AdobeRgb
                },
                &[toned, toned]
            )
            .is_err());
        assert!(graph
            .add(
                Operation::Sharpen {
                    amount: 1.,
                    space: ColorSpace::Srgb
                },
                &[toned]
            )
            .is_err());
        assert!(graph
            .add(
                Operation::Sharpen {
                    amount: 1.,
                    space: ColorSpace::Srgb
                },
                &[linear, sharp]
            )
            .is_ok());
    }
}
