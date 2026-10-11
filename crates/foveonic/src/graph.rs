//! Builds typed, dependency-ordered image graphs and validates their connections.

use crate::calibration::{Calibration, ColorParams, ColorSpace, EncodeParams};
use crate::source::Result;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    SensorLayers,
    ScalarField,
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
    Repair {
        flags: Vec<u32>,
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
            Self::Repair { .. } => "submit repair",
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
            Self::Repair { .. } => (vec![CalibratedLayers], CalibratedLayers),
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
        if matches!(operation, Operation::Black(_)) {
            outputs.push(self.outputs.len());
            self.outputs.push(Output {
                producer,
                data: DataType::Histogram,
                name: "black.histogram",
                dependencies: inputs.to_vec(),
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
    pub fn output_name(&self) -> &'static str {
        match self {
            Self::Source => "sensor.layers",
            Self::RenderedInput { .. } => "import.image",
            Self::Black(_) => "black.levels",
            Self::Calibrate(_) => "calibration.layers",
            Self::Repair { .. } => "repair.layers",
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
            .add(Operation::Repair { flags: vec![0] }, &[linear])
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
