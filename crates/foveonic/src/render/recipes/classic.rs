//! Composes equal-resolution photographic and editable DNG recipes from signed-code operations.

mod highlight;
mod prepare;

use super::{requested, BuiltRecipe};
use crate::{
    color_space::ColorSpace,
    graph::{Graph, Operation},
    interactive::Execution,
    nodes::sensor::{calibration, signed_repair},
    profile::Profile,
    render::{Engine, Settings},
    source::{Result, Source},
};
use std::sync::Arc;

pub(super) fn build(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    compose(engine, source, settings, names, profile, execution, false)
}

pub(super) fn build_dng(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    compose(
        engine,
        source,
        settings,
        &["final"],
        profile,
        execution,
        true,
    )
}

pub(crate) use prepare::prepare_dng_appearance;

fn compose(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
    dng: bool,
) -> Result<BuiltRecipe> {
    let metadata = &source.metadata;
    let mut params = calibration::encode(metadata, ColorSpace::ProPhotoRgb)?;
    if let Some(size) = settings.output_size {
        params.output[..2].copy_from_slice(&size);
    }
    engine.check_extent([params.output[0], params.output[1]])?;
    let mut graph = Graph::default();
    let mut final_id = graph.add(Operation::Source, &[])?;
    macro_rules! finish {
        () => {
            execution.check()?;
            if requested(&graph, names) {
                return Ok(BuiltRecipe {
                    graph,
                    final_id,
                    params,
                });
            }
        };
    }
    finish!();
    prepare::validate_request(metadata, settings)?;
    if dng {
        prepare::validate_color_mode(metadata, &settings.color_mode)?;
    }
    let prepared = profile.measure("prepare fixed-point camera recipe", || {
        prepare::prepare(metadata, &settings.white_balance)
    })?;
    let gains = prepared.final_gain;
    final_id = graph.add(
        Operation::FixedPointCalibration(Arc::new(prepared.correction)),
        &[final_id],
    )?;
    finish!();
    if settings.repair.is_some() {
        let repair = signed_repair::Defects::prepare(metadata)?;
        final_id = graph.add(Operation::SignedRepair(Arc::new(repair)), &[final_id])?;
        finish!();
    }
    if settings.highlight_restoration {
        if let Some(p) = prepare::prepare_highlight(metadata, gains)? {
            final_id = graph.add(Operation::HighlightStream(p), &[final_id])?;
            finish!();
        }
    }
    if settings.despeckle {
        final_id = graph.add(
            Operation::SignedDespeckle(prepare::prepare_despeckle(metadata)?),
            &[final_id],
        )?;
        finish!();
    }
    if settings.reconstruction.is_some() {
        for p in prepare::prepare_chroma(metadata, gains)? {
            final_id = graph.add(Operation::ChromaSmooth(p), &[final_id])?;
            finish!();
        }
    }
    let color = profile.measure("prepare signed camera color", || {
        if dng {
            prepare::prepare_unselected_color(metadata, &prepared.color)
        } else {
            prepare::prepare_color(metadata, &prepared.color, &settings.color_mode)
        }
    })?;
    final_id = graph.add(
        Operation::FixedPointColor(Arc::new(color.transform)),
        &[final_id],
    )?;
    finish!();
    if settings.reconstruction.is_some() {
        if let Some(fir) =
            prepare::prepare_reconstruction(metadata, color.matrix_green_sum, color.digital_gain)?
        {
            let corrected = graph.named_output(final_id, "color.corrected-layers")?;
            final_id = graph.add(Operation::LogReconstruct(fir), &[final_id, corrected])?;
            finish!();
        }
    }
    let root = prepare::prepare_root(metadata, params.crop, params.source[2])?;
    final_id = graph.add(Operation::RootDetail(root), &[final_id])?;
    let detail = graph.named_output(final_id, "detail.signal")?;
    finish!();
    if dng {
        let detail_amount = settings.sharpen_amount.map_or(0., |a| f64::from(a) - 1.);
        let linear = crate::nodes::finish::signed_response::prepare_linear_detail(detail_amount)?;
        final_id = graph.add(
            Operation::SignedLinearDetail(Arc::new(linear)),
            &[final_id, detail],
        )?;
        return Ok(BuiltRecipe {
            graph,
            final_id,
            params,
        });
    }
    let finish = prepare::prepare_photographic(metadata, settings)?;
    final_id = graph.add(
        Operation::SignedPhotographic(Arc::new(finish)),
        &[final_id, detail],
    )?;
    Ok(BuiltRecipe {
        graph,
        final_id,
        params,
    })
}
