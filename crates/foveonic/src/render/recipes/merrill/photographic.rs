//! Completes photographic RGB with selected camera appearance, exposure, tone, and saturation.

use super::super::{requested, BuiltRecipe};
use super::technical;
use crate::color_space::ColorSpace;
use crate::{
    graph::Operation,
    interactive::Execution,
    nodes::{finish::tone, sensor::calibration},
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
        let dimensions = [
            source.metadata.width as u32,
            source.metadata.height as u32,
            0,
            0,
        ];
        recipe.final_id = recipe.graph.add(
            Operation::Saturation {
                params: saturation(dimensions, settings.saturation),
                space,
            },
            &[recipe.final_id],
        )?;
        execution.check()?;
    }
    Ok(recipe)
}

pub(in crate::render) fn saturation(dimensions: [u32; 4], factor: f32) -> calibration::ColorParams {
    // Fixed reference saturation weights are independent of the working primaries.
    let saturation_weights = [0.299, 0.587, 0.114];
    calibration::ColorParams {
        source: dimensions,
        matrix: std::array::from_fn(|row| {
            std::array::from_fn(|col| {
                if col == 3 {
                    0.
                } else {
                    (1. - factor) * saturation_weights[col] + if row == col { factor } else { 0. }
                }
            })
        }),
    }
}
