//! Composes shared photographic edits after each camera's base appearance.

use super::BuiltRecipe;
use crate::{
    graph::{DataType, Operation},
    interactive::Execution,
    nodes::finish::photographic::{Method, Params},
    render::Settings,
    source::Result,
};

/// Camera adapters retain their existing RAW exposure and base appearance.
/// Universal contrast/saturation are evaluated once after that boundary.
pub(super) fn camera_settings(settings: &Settings) -> Settings {
    let mut base = settings.clone();
    if base.photographic.tone_method == Method::Universal {
        base.tone.contrast = 0.;
        base.saturation = 1.;
    }
    base
}

pub(super) fn build(
    settings: &Settings,
    mut recipe: BuiltRecipe,
    names: &[&str],
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    if super::requested(&recipe.graph, names) {
        return Ok(recipe);
    }
    let DataType::ToneMappedRgb(space) = recipe.graph.outputs[recipe.final_id].data else {
        // Early inspection deliberately stops before the photographic boundary.
        return Ok(recipe);
    };
    let extent = recipe.rgb_extent;
    if settings.photographic != Default::default()
        || !settings.tonal_ranges.is_neutral()
        || names.iter().any(|n| n.starts_with("finish."))
    {
        recipe.final_id = recipe.graph.add(
            Operation::ImageAlias {
                data: DataType::ToneMappedRgb(space),
                output: "finish.base",
            },
            &[recipe.final_id],
        )?;
    }
    if settings.photographic.tone_method == Method::Universal
        && (settings.tone.contrast != 0.
            || (!settings.is_monochrome() && settings.saturation != 1.))
    {
        let p = Params::tone(
            extent,
            space,
            settings.tone.contrast,
            if settings.is_monochrome() {
                1.
            } else {
                settings.saturation
            },
        );
        recipe.graph.retain_for_session(recipe.final_id);
        recipe.final_id = recipe.graph.add(
            Operation::PhotographicTone { params: p, space },
            &[recipe.final_id],
        )?;
    }
    execution.check()?;
    Ok(recipe)
}

#[cfg(test)]
#[path = "finishing_test.rs"]
mod tests;
