//! Completes photographic RGB with selected camera appearance, exposure, tone, and saturation.

use super::super::{requested, BuiltRecipe};
use super::technical;
use crate::color_space::ColorSpace;
use crate::{
    graph::Operation,
    interactive::Execution,
    nodes::finish::tone,
    profile::Profile,
    render::{Engine, Settings},
    source::{Result, Source},
};

pub(in crate::render::recipes) fn build(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let (mut recipe, mode) = match technical::build(
        engine,
        source,
        settings,
        names,
        Some(&settings.color_mode),
        profile,
        execution,
    )? {
        technical::Prepared::Requested(recipe) => return Ok(recipe),
        technical::Prepared::Linear(recipe, mode) => (recipe, mode),
    };
    let space = ColorSpace::from_metadata(&source.metadata)?;
    let curve = profile.measure("prepare tone curve", || {
        if settings.is_monochrome() {
            tone::prepare_monochrome(&source.metadata, settings.tone)
        } else {
            tone::prepare(&source.metadata, &mode, settings.tone)
        }
    })?;
    recipe.final_id = recipe.graph.add(
        Operation::Tone {
            curve,
            exposure: settings.exposure.exp2(),
            space,
        },
        &[recipe.final_id],
    )?;
    execution.check()?;
    if requested(&recipe.graph, names) {
        return Ok(recipe);
    }
    if !settings.is_monochrome() && settings.saturation != 1. {
        let dimensions = [recipe.params.source[0], recipe.params.source[1], 0, 0];
        recipe.final_id = recipe.graph.add(
            Operation::Saturation {
                params: super::super::saturation(dimensions, settings.saturation),
                space,
            },
            &[recipe.final_id],
        )?;
        execution.check()?;
    }
    Ok(recipe)
}
