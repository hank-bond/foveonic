//! Completes photographic RGB with selected camera appearance, exposure, tone, and saturation.

use super::super::{requested, BuiltRecipe};
use super::technical;
use crate::color_space::ColorSpace;
use crate::{
    graph::Operation,
    interactive::Execution,
    nodes::finish::{tone, tone_equalizer},
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
    let tonal = if !settings.tonal_ranges.is_neutral()
        || names
            .iter()
            .any(|n| matches!(*n, "tonal-ranges.mask" | "tonal-ranges.rgb"))
    {
        let extent = [recipe.params.source[0], recipe.params.source[1]];
        let guide = tone_equalizer::Guide::new(
            extent,
            tone_equalizer::radius(settings.preview_quality.is_reduced()),
            space,
        )?;
        let linear = recipe.final_id;
        // A Fast/Full transition changes the upstream grid. Keep the new linear
        // boundary immediately so the next tonal edit never warms up denoising.
        recipe.graph.retain_for_session(linear);
        let mask = recipe
            .graph
            .add(Operation::ToneMask { guide, space }, &[linear])?;
        recipe.graph.retain_for_session(mask);
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(recipe);
        }
        Some((linear, mask))
    } else {
        None
    };
    let curve = profile.measure("prepare tone curve", || {
        if settings.is_monochrome() {
            tone::prepare_monochrome(&source.metadata, settings.tone)
        } else {
            tone::prepare(&source.metadata, &mode, settings.tone)
        }
    })?;
    if let Some((linear, mask)) = tonal {
        recipe.final_id = recipe.graph.add(
            Operation::TonalRanges {
                controls: settings.tonal_ranges,
                pivot: tone_equalizer::pivot(&curve, settings.exposure)?,
                space,
            },
            &[linear, mask],
        )?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(recipe);
        }
    }
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
