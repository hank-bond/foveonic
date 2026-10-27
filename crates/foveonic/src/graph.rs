//! Builds typed dependency-ordered graphs and validates connections between operation ports.

use crate::{color_space::ColorSpace, source::Result};
mod operation;
pub use operation::Operation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    /// Six original codes per cell: R, G and four spatial blue phases.
    PhaseCells,
    /// Integer defect flags retained for later repair operations, not display samples.
    DefectFlags,
    /// Integer processing flags. Inspection requires explicit floating conversion.
    HighlightMask,
    SensorRgb,
    SensorBlue,
    CorrectedSensorRgb,
    CorrectedSensorBlue,
    SensorLayers,
    SensorMask,
    ScalarField,
    Monochrome,
    LuminanceRatio,
    SignedDetail,
    FilteredTiles,
    ConditionedTiles,
    PatternSeedTiles,
    UnconditionedRatioTiles,
    RatioTiles,
    ColorTiles(ColorSpace),
    BlackLevels,
    Histogram,
    RowTable,
    CalibratedLayers,
    /// Linear codes awaiting the reconstruction finalizer's required gain.
    NormalizedCodeRgb,
    /// Linear-light RGB code values. The recipe supplies their nominal white.
    LinearCodeRgb,
    SignedCodeRgb,
    /// Unsigned16 integral RGB codes carried exactly in Float32 storage.
    UnsignedCodeRgb,
    /// Unsigned16 L/a/b component codes carried in three Float32 samples.
    PackedLabCodes,
    LinearRgb(ColorSpace),
    ToneMappedRgb(ColorSpace),
    EncodedRgb16(crate::render::TiffColorSpace),
    File,
}

impl DataType {
    /// Image outputs carry physical dimensions and a canonical buffer layout.
    /// Tables, packed cells and reconstruction tiles are not displayable images.
    pub(crate) fn image_description(self, extent: [u32; 2]) -> Option<crate::image::Description> {
        use crate::image::{Description, Space};
        let space = match self {
            Self::PhaseCells | Self::DefectFlags | Self::HighlightMask | Self::PackedLabCodes => {
                return None
            }
            Self::CorrectedSensorRgb => Space::CorrectedSensorRgb,
            Self::CorrectedSensorBlue => Space::CorrectedSensorBlue,
            Self::SensorRgb => Space::SensorRgb,
            Self::SensorBlue => Space::SensorBlue,
            Self::SensorLayers => Space::SensorLayers,
            Self::ScalarField => Space::ScalarField,
            Self::SensorMask => Space::SensorMask,
            Self::Monochrome => Space::Monochrome,
            Self::LuminanceRatio => Space::LuminanceRatio,
            Self::SignedDetail => Space::SignedDetail,
            Self::CalibratedLayers => Space::CalibratedLayers,
            Self::NormalizedCodeRgb => Space::NormalizedCodeRgb,
            Self::LinearCodeRgb => Space::LinearCodeRgb,
            Self::SignedCodeRgb => Space::SignedCodeRgb,
            Self::UnsignedCodeRgb => Space::UnsignedCodeRgb,
            Self::LinearRgb(space) => Space::LinearRgb(space),
            Self::ToneMappedRgb(space) => Space::ToneMappedRgb(space),
            Self::EncodedRgb16(space) => Space::EncodedRgb16(space),
            Self::BlackLevels
            | Self::Histogram
            | Self::RowTable
            | Self::File
            | Self::FilteredTiles
            | Self::ConditionedTiles
            | Self::PatternSeedTiles
            | Self::UnconditionedRatioTiles
            | Self::RatioTiles
            | Self::ColorTiles(_) => return None,
        };
        Some(Description::new(extent, space))
    }
}

impl From<crate::image::Space> for DataType {
    fn from(space: crate::image::Space) -> Self {
        use crate::image::Space;
        match space {
            Space::SensorLayers => Self::SensorLayers,
            Space::SensorRgb => Self::SensorRgb,
            Space::SensorBlue => Self::SensorBlue,
            Space::CorrectedSensorRgb => Self::CorrectedSensorRgb,
            Space::CorrectedSensorBlue => Self::CorrectedSensorBlue,
            Space::ScalarField => Self::ScalarField,
            Space::SensorMask => Self::SensorMask,
            Space::Monochrome => Self::Monochrome,
            Space::LuminanceRatio => Self::LuminanceRatio,
            Space::SignedDetail => Self::SignedDetail,
            Space::CalibratedLayers => Self::CalibratedLayers,
            Space::NormalizedCodeRgb => Self::NormalizedCodeRgb,
            Space::LinearCodeRgb => Self::LinearCodeRgb,
            Space::SignedCodeRgb => Self::SignedCodeRgb,
            Space::UnsignedCodeRgb => Self::UnsignedCodeRgb,
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
    cache_scope: String,
    pub dependencies: Vec<usize>,
}

impl Output {
    pub(crate) fn cache_name(&self) -> String {
        if self.cache_scope.is_empty() {
            self.name.to_owned()
        } else {
            format!("{}/{}", self.cache_scope, self.name)
        }
    }
}

#[derive(Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub outputs: Vec<Output>,
    pub(crate) session_outputs: Vec<usize>,
}

impl Graph {
    /// Recipe-owned restart boundaries supplement the generic editing frontier.
    /// The session's normal byte budget and input-version checks still apply.
    pub(crate) fn retain_for_session(&mut self, output: usize) {
        assert!(output < self.outputs.len());
        if !self.session_outputs.contains(&output) {
            self.session_outputs.push(output);
        }
    }

    /// Prefer an expensive editing boundary when the session cannot retain every stage.
    pub(crate) fn prioritize_for_session(&mut self, output: usize) {
        self.session_outputs.retain(|id| *id != output);
        self.retain_for_session(output);
    }

    /// Requiring existing inputs prevents cycles and keeps execution order clear.
    pub fn add(&mut self, operation: Operation, inputs: &[usize]) -> Result<usize> {
        if matches!(operation, Operation::LinearCodeRgb { white: 0, .. }) {
            return Err("linear RGB requires a nonzero nominal white".into());
        }
        if matches!(operation, Operation::TransferLinearGain { data, .. } | Operation::LinearBasis { data, .. } if !matches!(data, DataType::LinearRgb(_) | DataType::LinearCodeRgb))
        {
            return Err("linear edit baking requires a linear sample base".into());
        }
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
            cache_scope: String::new(),
            dependencies: inputs.to_vec(),
        });
        let mut outputs = vec![id];
        for (name, data, slots) in operation.side_outputs() {
            outputs.push(self.outputs.len());
            self.outputs.push(Output {
                producer,
                data,
                name,
                cache_scope: String::new(),
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

    /// Distinguish repeated numerical operations at different recipe boundaries.
    /// Public inspection names stay independent of their stable cache scope.
    pub(crate) fn add_scoped(
        &mut self,
        operation: Operation,
        inputs: &[usize],
        scope: &str,
    ) -> Result<usize> {
        let id = self.add(operation, inputs)?;
        let producer = self.outputs[id].producer;
        for output in &self.nodes[producer].outputs {
            self.outputs[*output].cache_scope = scope.to_owned();
        }
        Ok(id)
    }

    /// Append an independently authored branch with separate cache identities.
    /// Its public inspection names remain local to the original recipe.
    pub(crate) fn append(&mut self, branch: Graph, scope: &str) -> Vec<usize> {
        let mut mapping = vec![0; branch.outputs.len()];
        for node in branch.nodes {
            let inputs: Vec<_> = node.inputs.iter().map(|id| mapping[*id]).collect();
            // Reuse identical technical prefixes, including the original upload.
            // Different parameters or inputs retain independently scoped outputs.
            let same = self.nodes.iter().find(|candidate| {
                std::mem::discriminant(&candidate.operation)
                    == std::mem::discriminant(&node.operation)
                    && candidate.inputs == inputs
                    && candidate.outputs.len() == node.outputs.len()
                    && candidate.outputs.iter().zip(&node.outputs).all(|(a, b)| {
                        let a = &self.outputs[*a];
                        let b = &branch.outputs[*b];
                        a.data == b.data
                            && a.name == b.name
                            && candidate.operation.key(a.name) == node.operation.key(b.name)
                    })
            });
            if let Some(same) = same {
                for (old, new) in node.outputs.iter().zip(&same.outputs) {
                    mapping[*old] = *new;
                }
                continue;
            }
            let producer = self.nodes.len();
            let mut outputs = Vec::new();
            for old in node.outputs {
                let output = &branch.outputs[old];
                let id = self.outputs.len();
                mapping[old] = id;
                outputs.push(id);
                self.outputs.push(Output {
                    producer,
                    data: output.data,
                    name: output.name,
                    cache_scope: if output.cache_scope.is_empty() {
                        scope.to_owned()
                    } else {
                        format!("{scope}/{}", output.cache_scope)
                    },
                    dependencies: output.dependencies.iter().map(|id| mapping[*id]).collect(),
                });
            }
            self.nodes.push(Node {
                operation: node.operation,
                inputs,
                outputs,
            });
        }
        for id in branch.session_outputs {
            self.retain_for_session(mapping[id]);
        }
        mapping
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
