//! Retains exact Auto Haze color statistics without retaining their native analysis inputs.

use super::{recipes, Engine, Settings};
use crate::{
    graph::{DataType, Operation},
    image::RenderedImage,
    interactive::Execution,
    profile::Profile,
    recipe::{Key, RetainedBytes},
    source::{Result, Source},
};

#[derive(PartialEq, Eq)]
struct Step {
    name: String,
    operation: std::mem::Discriminant<Operation>,
    parameters: Key,
    data: DataType,
    inputs: Vec<String>,
}

pub(super) struct AutoHaze {
    pub color: Option<[f32; 3]>,
    identity: Vec<Step>,
}
impl AutoHaze {
    pub fn prepare(
        engine: &Engine,
        original: &Source,
        sensor: &RenderedImage,
        settings: &Settings,
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<Self> {
        // Request the existing analysis output, which authors its own fixed Fast
        // guide. The reduced sensor is never substituted for its numerical input.
        let artifact = engine
            .render_recipe(
                recipes::RecipeInput::Original(original),
                settings,
                recipes::Recipe::Photographic(&["dehaze.analysis"]),
                profile,
                execution,
            )?
            .pop()
            .ok_or("missing Auto Haze analysis")?;
        Ok(Self {
            color: artifact.dehaze_estimate,
            identity: identity(engine, sensor, settings, profile, execution)?,
        })
    }
    pub fn matches(
        &self,
        engine: &Engine,
        sensor: &RenderedImage,
        settings: &Settings,
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<bool> {
        Ok(self.identity == identity(engine, sensor, settings, profile, execution)?)
    }
    pub fn account(&self, retained: &mut RetainedBytes) -> usize {
        let mut bytes = self.identity.capacity() * std::mem::size_of::<Step>();
        for step in &self.identity {
            bytes += step.name.capacity() + step.inputs.capacity() * std::mem::size_of::<String>();
            bytes += step.inputs.iter().map(String::capacity).sum::<usize>();
            step.parameters.account(retained);
        }
        bytes
    }
}

fn identity(
    engine: &Engine,
    sensor: &RenderedImage,
    settings: &Settings,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<Vec<Step>> {
    let effective = settings.processing_settings(crate::source::ProcessingFamily::Merrill);
    let mut analysis = effective.into_owned();
    analysis.preview_quality = super::PreviewQuality::Fast;
    analysis.preview_region = None;
    analysis.output_size = None;
    let built = recipes::build_camera(
        engine,
        recipes::RecipeInput::Reduced(sensor),
        &analysis,
        &["dehaze.analysis"],
        profile,
        execution,
    )?;
    let target = built
        .graph
        .outputs
        .iter()
        .position(|output| output.name == "dehaze.analysis")
        .ok_or("missing Auto Haze identity")?;
    let required = built.graph.required(&[target])?;
    // Only numerical guide dependencies participate. Native corrections are
    // already covered by the sealed sensor identity and immutable source metadata.
    // This graph is inspected, never executed with the Thumb/Draft sensor.
    Ok(built
        .graph
        .outputs
        .iter()
        .enumerate()
        .filter_map(|(id, output)| {
            let operation = &built.graph.nodes[output.producer].operation;
            (required[id] && !matches!(operation, Operation::RenderedInput { .. })).then(|| Step {
                name: output.cache_name(),
                operation: std::mem::discriminant(operation),
                parameters: operation.key(output.name),
                data: output.data,
                inputs: output
                    .dependencies
                    .iter()
                    .map(|&id| built.graph.outputs[id].cache_name())
                    .collect(),
            })
        })
        .collect())
}
