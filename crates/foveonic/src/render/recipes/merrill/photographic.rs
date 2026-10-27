//! Applies the camera tone curve and saturation after shared linear photographic edits.

use super::super::linear_edits::{self, Context};
use super::super::{requested, BuiltRecipe};
use super::technical;
use crate::{
    color_space::ColorSpace,
    graph::Operation,
    interactive::Execution,
    profile::Profile,
    render::{Engine, Settings},
    source::{Result, Source},
};

// Camera tone tables use 4095 as nominal white. The sensitivity model uses
// the reconstruction's Q12 signal unit, preserving its established response.
const LINEAR_CODE_WHITE: f32 = 4095.;
const CLASSIFIER_CODE_UNIT: f32 = 4096.;

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
    linear_edits::alias(&mut recipe, space, "linear.base")?;
    if requested(&recipe.graph, names) {
        return Ok(recipe);
    }
    let mut prepared_curve = None;
    let prepare_curve = |profile: &mut Profile| {
        profile.measure("prepare tone curve", || {
            if settings.is_monochrome() {
                crate::nodes::finish::tone::prepare_monochrome(&source.metadata, settings.tone)
            } else {
                crate::nodes::finish::tone::prepare(&source.metadata, &mode, settings.tone)
            }
        })
    };
    let context = Context {
        space,
        white: LINEAR_CODE_WHITE,
        guide_white: CLASSIFIER_CODE_UNIT,
        scale: if settings.preview_region.is_some() {
            1.
        } else {
            (recipe.params.source[0] as f32 / source.metadata.width as f32)
                .min(recipe.params.source[1] as f32 / source.metadata.height as f32)
        },
    };
    linear_edits::build(
        engine,
        source,
        settings,
        &mut recipe,
        names,
        context,
        |profile| {
            let curve = prepare_curve(profile)?;
            let pivot = crate::nodes::finish::tone_equalizer::pivot(&curve, settings.exposure)?;
            prepared_curve = Some(curve);
            Ok(pivot)
        },
        profile,
        execution,
    )?;
    if requested(&recipe.graph, names) {
        return Ok(recipe);
    }
    let curve = match prepared_curve {
        Some(curve) => curve,
        None => prepare_curve(profile)?,
    };
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
