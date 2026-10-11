//! Builds typed, dependency-ordered image graphs and validates their connections.

use crate::calibration::{Calibration, ColorParams, ColorSpace, EncodeParams};
use crate::source::Result;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    SensorLayers,
    BlackLevels,
    CalibratedLayers,
    LinearRgb(ColorSpace),
    ToneMappedRgb(ColorSpace),
    EncodedRgb16(ColorSpace),
    File,
}

pub enum Operation {
    Source,
    RenderedInput {
        space: ColorSpace,
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
    Gains {
        params: ColorParams,
        space: ColorSpace,
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
            Self::Gains { .. } => "submit gains",
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
            Self::RenderedInput { space } => (vec![], ToneMappedRgb(*space)),
            Self::Black(_) => (vec![SensorLayers], BlackLevels),
            Self::Calibrate(_) => (vec![SensorLayers, BlackLevels], CalibratedLayers),
            Self::Color { space, .. } => (vec![CalibratedLayers], LinearRgb(*space)),
            Self::Repair { .. } => (vec![CalibratedLayers], CalibratedLayers),
            Self::Gains { space, .. } | Self::ColorDq { space, .. } => {
                (vec![LinearRgb(*space)], LinearRgb(*space))
            }
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
    pub output: DataType,
}

#[derive(Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
}

impl Graph {
    /// Inputs must already exist, keeping graph construction acyclic and execution ordered.
    pub fn add(&mut self, operation: Operation, inputs: &[usize]) -> Result<usize> {
        let (expected, output) = operation.types();
        if inputs.len() != expected.len() {
            return Err("wrong number of node inputs".into());
        }
        for (&input, expected) in inputs.iter().zip(expected) {
            let node = self.nodes.get(input).ok_or("node input does not exist")?;
            if node.output != expected {
                return Err(format!(
                    "incompatible node input: expected {expected:?}, received {:?}",
                    node.output
                )
                .into());
            }
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            operation,
            inputs: inputs.to_vec(),
            output,
        });
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
