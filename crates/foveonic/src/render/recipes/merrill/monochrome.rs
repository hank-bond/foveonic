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
    source::{Metadata, Result},
};
use std::sync::Arc;

fn guide_controls(settings: &Settings) -> crate::render::MonochromeControls {
    let mut controls = settings.monochrome;
    // The reference amount controls its selected banding method only. The
    // scalar denoiser still needs a stable guide when that method is inactive.
    if !settings.reference_banding_selected() {
        controls.banding = crate::render::MonochromeControls::default().banding;
    }
    controls
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    metadata: &Arc<Metadata>,
    settings: &Settings,
    mut recipe: BuiltRecipe,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
    grid: Option<&super::preview::Grid>,
    rows: Option<crate::nodes::sensor::banding::Description>,
    speckles: Option<super::native_layers::Speckles>,
) -> Result<Prepared> {
    let space = ColorSpace::from_metadata(metadata)?;
    if let Some(speckles) = speckles {
        recipe.final_id = speckles.append(&mut recipe.graph, recipe.final_id, grid)?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(Prepared::Requested(recipe));
        }
    }
    if settings.directional_banding_selected() {
        let params = profile.measure("prepare monochrome bands", || {
            let params = crate::noise::prepare(
                metadata,
                &settings.white_balance,
                settings.sensor_noise_controls(),
            )?;
            let params = match grid {
                Some(grid) => grid.noise(params)?,
                None => params,
            };
            crate::noise::banding_parameters(
                metadata,
                params,
                settings.merrill_banding,
                grid.map(|grid| grid.grid),
            )
        })?;
        let bands = super::reconstruction::append_banding(
            &mut recipe.graph,
            recipe.final_id,
            Arc::new(params),
        )?;
        // Project the shared band estimates back onto the monochrome working layers.
        recipe.final_id = recipe.graph.named_output(bands, "column.layers")?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(Prepared::Requested(recipe));
        }
    }
    if let Some(rows) = rows {
        recipe.final_id = recipe.graph.add(
            Operation::Banding(rows.on_grid(grid.map(|grid| grid.grid))),
            &[recipe.final_id],
        )?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(Prepared::Requested(recipe));
        }
    }
    let prepared = Arc::new(profile.measure("prepare monochrome", || {
        MonoPrepared::new(metadata, &settings.white_balance, guide_controls(settings))?
            .on_grid(grid.map(|g| g.grid))
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
    let column = if settings.reference_banding_selected() {
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
        corrected = recipe.graph.add(
            Operation::MonoTexture {
                controls: texture,
                scale: settings.working_scale.factor(),
            },
            &[corrected],
        )?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(Prepared::Requested(recipe));
        }
    }
    let signal = node!(Stage::Signal, &[corrected]);
    let mut linear = node!(Stage::Expand, &[corrected]);
    if settings.sharpen_selected() {
        let amount = profile.measure("prepare sharpening amount", || {
            settings.effective_sharpen_amount(metadata)
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
    Ok(Prepared::Linear(
        recipe,
        super::dng_base::unselected_mode(),
        None,
    ))
}
