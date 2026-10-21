//! Builds typed dependency-ordered graphs and validates connections between operation ports.

use crate::{color_space::ColorSpace, source::Result};
mod operation;
pub use operation::Operation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    SensorLayers,
    ScalarField,
    Monochrome,
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
    LinearCodeRgb,
    SignedCodeRgb,
    LinearRgb(ColorSpace),
    ToneMappedRgb(ColorSpace),
    EncodedRgb16(crate::render::TiffColorSpace),
    File,
}

impl From<crate::image::Space> for DataType {
    fn from(space: crate::image::Space) -> Self {
        use crate::image::Space;
        match space {
            Space::SensorLayers => Self::SensorLayers,
            Space::ScalarField => Self::ScalarField,
            Space::Monochrome => Self::Monochrome,
            Space::LuminanceRatio => Self::LuminanceRatio,
            Space::SignedDetail => Self::SignedDetail,
            Space::CalibratedLayers => Self::CalibratedLayers,
            Space::LinearCodeRgb => Self::LinearCodeRgb,
            Space::SignedCodeRgb => Self::SignedCodeRgb,
            Space::LinearRgb(space) => Self::LinearRgb(space),
            Space::ToneMappedRgb(space) => Self::ToneMappedRgb(space),
            Space::EncodedRgb16(space) => Self::EncodedRgb16(space),
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

    /// Finds a named output of the node that produced `output`.
    /// Names are local to that producer, so repeated operation names stay unambiguous.
    pub fn named_output(&self, output: usize, name: &str) -> Result<usize> {
        let producer = self
            .outputs
            .get(output)
            .ok_or("node output does not exist")?
            .producer;
        self.nodes[producer]
            .outputs
            .iter()
            .copied()
            .find(|&id| self.outputs[id].name == name)
            .ok_or_else(|| format!("node output {name:?} does not exist").into())
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

#[cfg(test)]
#[path = "graph/tests.rs"]
mod tests;
