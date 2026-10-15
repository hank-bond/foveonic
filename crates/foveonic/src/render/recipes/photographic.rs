//! Completes photographic RGB with selected camera appearance, exposure, tone, and saturation.

use super::{requested, shared, BuiltRecipe};
use crate::{
    graph::Operation,
    interactive::Execution,
    nodes::{
        finish::tone,
        sensor::calibration::{self, ColorSpace},
    },
    profile::Profile,
    render::{Engine, Settings},
    source::{Result, Source},
};

pub(super) fn build(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let (mut recipe, mode) = match engine.build_technical(
        source,
        settings,
        names,
        Some(&settings.color_mode),
        profile,
        execution,
    )? {
        shared::Prepared::Requested(recipe) => return Ok(recipe),
        shared::Prepared::Linear(recipe, mode) => (recipe, mode),
    };
    let space = ColorSpace::from_metadata(&source.metadata)?;
    let curve = profile.measure("prepare tone curve", || {
        tone::prepare(&source.metadata, &mode, settings.tone)
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
    if settings.saturation != 1. {
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
    // Reference-style brightness weights preserve signed RGB and headroom.
    let luma = [0.299, 0.587, 0.114];
    calibration::ColorParams {
        source: dimensions,
        matrix: std::array::from_fn(|row| {
            std::array::from_fn(|col| {
                if col == 3 {
                    0.
                } else {
                    (1. - factor) * luma[col] + if row == col { factor } else { 0. }
                }
            })
        }),
    }
}
