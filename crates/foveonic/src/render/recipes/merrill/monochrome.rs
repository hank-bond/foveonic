//! Authors sensor-layer monochrome filtering and finishing without the color reconstruction path.

use super::super::{requested, BuiltRecipe};
use super::technical::Prepared;
use crate::{
    graph::Operation,
    image::ColorSpace,
    interactive::Execution,
    nodes::monochrome::{Prepared as MonoPrepared, Stage},
    profile::Profile,
    render::Settings,
    source::{Result, Source},
};
use std::sync::Arc;

pub(super) fn build(
    source: &Source,
    settings: &Settings,
    mut recipe: BuiltRecipe,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<Prepared> {
    let space = ColorSpace::from_metadata(&source.metadata)?;
    let prepared = Arc::new(profile.measure("prepare monochrome", || {
        MonoPrepared::new(
            &source.metadata,
            &settings.white_balance,
            settings.monochrome,
        )
    })?);
    macro_rules! node {
        ($stage:expr, $inputs:expr) => {{
            let id = recipe.graph.add(
                Operation::Monochrome {
                    stage: $stage,
                    prepared: prepared.clone(),
                    space,
                },
                $inputs,
            )?;
            execution.check()?;
            if requested(&recipe.graph, names) {
                return Ok(Prepared::Requested(recipe));
            }
            id
        }};
    }
    let mix = node!(Stage::Mix, &[recipe.final_id]);
    let directional = node!(Stage::Directional, &[mix]);
    let column = if settings.processing_sections.noise {
        node!(Stage::Column, &[mix, directional])
    } else {
        mix
    };
    let (mono, neighbors) = if settings.processing_sections.noise && prepared.params.filter[1] > 0.
    {
        let filtered = node!(Stage::Histogram, &[column, directional]);
        (filtered, filtered)
    } else {
        (column, directional)
    };
    let mut corrected = if settings.despeckle {
        node!(Stage::Despeckle, &[mono, neighbors])
    } else {
        mono
    };
    if let Some(texture) = settings.monochrome.detail() {
        corrected = recipe
            .graph
            .add(Operation::MonoTexture(texture), &[corrected])?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(Prepared::Requested(recipe));
        }
    }
    let signal = node!(Stage::Signal, &[corrected]);
    let mut linear = node!(Stage::Expand, &[corrected]);
    if settings.sharpen_selected() {
        let amount = profile.measure("prepare sharpening amount", || {
            settings.effective_sharpen_amount(&source.metadata)
        })?;
        linear = recipe
            .graph
            .add(Operation::MonoSharpen { amount, space }, &[linear, signal])?;
        execution.check()?;
    }
    recipe.final_id = linear;
    if requested(&recipe.graph, names) {
        return Ok(Prepared::Requested(recipe));
    }
    Ok(Prepared::Linear(recipe, super::dng_base::unselected_mode()))
}
