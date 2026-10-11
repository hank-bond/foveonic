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
    ResidualTiles,
    RatioTiles,
    ColorTiles(ColorSpace),
    BlackLevels,
    Histogram,
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
    Repair(crate::repair::Prepared),
    Despeckle(crate::despeckle::Prepared),
    NoiseBase(crate::noise::Parameters),
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
    Blur {
        axis: u32,
        weights: Vec<f32>,
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
    pub fn label(&self) -> &'static str {
        match self {
            Self::Source => "upload sensor",
            Self::RenderedInput { .. } => "import rendered image",
            Self::Black(_) => "submit black",
            Self::Calibrate(_) => "submit calibration",
            Self::Color { .. } => "submit color",
            Self::Repair(_) => "submit repair",
            Self::Despeckle(_) => "submit despeckle",
            Self::NoiseBase(_) => "submit reference noise base",
            Self::Detail(_) => "submit reference detail",
            Self::TiledColor { .. } => "submit tiled color",
            Self::TiledColorDq { .. } => "submit tiled ColorDQ",
            Self::Reconstruct { .. } => "submit reconstruction",
            Self::ColorDq { .. } => "submit ColorDQ",
            Self::SensorGain { .. } => "submit sensor gain",
            Self::Saturation { .. } => "submit saturation",
            Self::Blur { .. } => "submit blur",
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
            Self::Repair(_) | Self::Despeckle(_) => (vec![CalibratedLayers], CalibratedLayers),
            Self::NoiseBase(_) => (vec![CalibratedLayers], FilteredTiles),
            Self::Detail(_) => (vec![CalibratedLayers, FilteredTiles], ResidualTiles),
            Self::TiledColor { space, .. } => (vec![FilteredTiles], ColorTiles(*space)),
            Self::TiledColorDq { space, .. } => (vec![ColorTiles(*space)], ColorTiles(*space)),
            Self::Reconstruct { space, .. } => (
                vec![ColorTiles(*space), ResidualTiles, RatioTiles],
                LinearRgb(*space),
            ),
            Self::SensorGain { .. } => (vec![CalibratedLayers], CalibratedLayers),
            Self::ColorDq { space, .. } => (vec![LinearRgb(*space)], LinearRgb(*space)),
            Self::Saturation { space, .. } | Self::Blur { space, .. } => {
                (vec![ToneMappedRgb(*space)], ToneMappedRgb(*space))
            }
            Self::Sharpen { space, .. } => (
                vec![ToneMappedRgb(*space), ToneMappedRgb(*space)],
                ToneMappedRgb(*space),
            ),
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
    /// Inputs must already exist, keeping graph construction acyclic and execution ordered.
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

    /// Output demand is independent of node execution: side outputs can be transient.
    pub fn required(&self, roots: &[usize]) -> Result<Vec<bool>> {
        let mut required = vec![false; self.outputs.len()];
        let mut pending = roots.to_vec();
        while let Some(id) = pending.pop() {
            let output = self
                .outputs
                .get(id)
                .ok_or("requested output does not exist")?;
            if !required[id] {
                required[id] = true;
                pending.extend_from_slice(&output.dependencies);
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
            Self::NoiseBase(_) => vec![
                ("noise.layers", CalibratedLayers, vec![0]),
                ("noise.edges", ScalarField, vec![0]),
            ],
            Self::Detail(_) => vec![
                ("detail.ratio-tiles", RatioTiles, vec![0, 1]),
                ("detail.ratio", LuminanceRatio, vec![0, 1]),
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
            Self::NoiseBase(_) => "noise.tiles",
            Self::Detail(_) => "detail.gain-tiles",
            Self::TiledColor { .. } => "color.tiles",
            Self::TiledColorDq { .. } => "color-dq.tiles",
            Self::Reconstruct { .. } => "reconstruction.rgb",
            Self::Color { .. } => "color.rgb",
            Self::ColorDq { .. } => "color-dq.rgb",
            Self::SensorGain { .. } => "gain.layers",
            Self::Tone { .. } => "tone.rgb",
            Self::Saturation { .. } => "saturation.rgb",
            Self::Blur { axis: 0, .. } => "sharpen.horizontal",
            Self::Blur { .. } => "sharpen.blurred",
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
        let detail = graph.add(Operation::Detail(params), &[raw, base]).unwrap();
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
                &[colour, detail, detail + 1],
            )
            .unwrap();
        let required = graph.required(&[restored + 1]).unwrap();
        assert!(required[raw] && required[base] && required[colour] && required[detail + 1]);
        assert!(!required[detail] && !required[restored]);
        assert_eq!(graph.outputs[restored + 1].data, DataType::SignedDetail);
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
                Operation::Repair(crate::repair::Prepared {
                    params: [3, 3, 0, 0],
                    records: vec![]
                }),
                &[linear]
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
                &[toned, toned]
            )
            .is_ok());
    }
}
