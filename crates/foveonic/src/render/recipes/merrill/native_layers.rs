//! Composes Merrill native layer corrections before Color and Monochrome diverge.

use super::super::{requested, BuiltRecipe};
use super::layer_noise;
use crate::{
    graph::{Graph, Operation},
    interactive::Execution,
    nodes::sensor::{banding, calibration, camera_repair, color_field, despeckle, highlight},
    profile::Profile,
    render::{PreviewQuality, Settings},
    source::{Result, Source},
};
use std::sync::Arc;

/// Preview policy resolves stage selection once at the technical recipe boundary.
pub(super) struct Policy {
    pub multiscale: bool,
    pub directional_banding: bool,
    pub reference_rows: bool,
    pub retain_calibrated: bool,
}
impl Policy {
    pub fn new(settings: &Settings, reduced: bool, retain_calibrated: bool) -> Self {
        let draft = reduced && settings.preview_quality == PreviewQuality::Draft;
        Self {
            multiscale: settings.multiscale_noise_selected() && !draft,
            directional_banding: settings.directional_banding_selected() && !draft,
            reference_rows: settings.reference_rows_selected() && !reduced,
            retain_calibrated,
        }
    }
}

pub(super) enum Output {
    Requested(BuiltRecipe),
    Ready {
        recipe: BuiltRecipe,
        couplings: Option<crate::noise::Couplings>,
    },
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    engine: &crate::render::Engine,
    source: &Source,
    settings: &Settings,
    params: calibration::EncodeParams,
    policy: &Policy,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<Output> {
    let mut graph = Graph::default();
    let input = graph.add(Operation::Source, &[])?;
    macro_rules! finish {
        () => {
            execution.check()?;
            if requested(&graph, names) {
                return Ok(Output::Requested(BuiltRecipe {
                    graph,
                    final_id: input,
                    params,
                    rgb_extent: [params.source[0], params.source[1]],
                    rgb_bounds: params.crop,
                }));
            }
        };
    }
    finish!();
    let mut highlight_params = None;
    if settings.highlight_restoration && names.contains(&"highlight.density") {
        let prepared = Arc::new(profile.measure("prepare original saturation", || {
            highlight::prepare(&source.metadata, settings.highlight_threshold_ev)
        })?);
        graph.add(Operation::HighlightDensity(prepared.clone()), &[])?;
        highlight_params = Some(prepared);
        finish!();
    }
    let black_params = profile.measure("prepare black", || calibration::black(&source.metadata))?;
    let black = graph.add(Operation::Black(black_params), &[input])?;
    finish!();
    let calibration = profile.measure("prepare calibration", || {
        calibration::prepare(&source.metadata, &settings.white_balance)
    })?;
    let calibration_gains = std::array::from_fn(|c| calibration.params.gains[c]);
    let calibration_params = calibration.params;
    let defect_map = engine
        .noise_calibration
        .as_ref()
        .filter(|_| settings.processing_sections.noise && settings.despeckle)
        .and_then(|profile| profile.matching_defects(&source.metadata))
        .filter(|map| !map.points.is_empty());
    let working = if let Some(map) = defect_map {
        let repaired = graph.add(
            Operation::CalibratedDefects(Arc::new(
                crate::nodes::sensor::calibrated_defects::Prepared::new(map, false),
            )),
            &[input],
        )?;
        finish!();
        repaired
    } else {
        input
    };
    let model = engine.noise_model(&source.metadata)?;
    let corrected = if policy.multiscale || policy.directional_banding {
        let (shields, filter) = profile.measure("prepare multiscale noise", || {
            layer_noise::prepare(
                &source.metadata,
                &settings.white_balance,
                calibration_gains,
                settings.merrill_noise_method
                    == crate::render::MerrillNoiseMethod::MultiscaleRowsAndColumns,
                model,
            )
        })?;
        let corrected = graph.add(
            Operation::ShieldNoise(Arc::new(shields)),
            &[working, input, black],
        )?;
        finish!();
        let corrected = if policy.directional_banding {
            let guard = profile.measure("prepare guarded native rows", || {
                layer_noise::prepare_row_guard(&source.metadata, model)
            })?;
            let guarded = graph.add(
                Operation::RowGuard(Arc::new(guard)),
                &[input, corrected, black],
            )?;
            finish!();
            guarded
        } else {
            corrected
        };
        if policy.multiscale {
            let filtered = graph.add(
                Operation::Multiscale(Arc::new(filter)),
                &[corrected, input, black],
            )?;
            finish!();
            filtered
        } else {
            corrected
        }
    } else {
        working
    };
    // The filter preserves original clipped triplets. Check mapped outliers in
    // those triplets again without weakening the immutable clipping evidence.
    let corrected = if let Some(map) = defect_map.filter(|_| policy.multiscale) {
        let repaired = graph.add(
            Operation::CalibratedDefects(Arc::new(
                crate::nodes::sensor::calibrated_defects::Prepared::new(map, true),
            )),
            &[corrected],
        )?;
        finish!();
        repaired
    } else {
        corrected
    };
    let mut calibrated = graph.add(Operation::Calibrate(calibration), &[corrected, black])?;
    finish!();
    let unrepaired = calibrated;
    if let Some(threshold) = settings.repair {
        let prepared = profile.measure("prepare repair records", || {
            camera_repair::prepare(&source.metadata, threshold)
        })?;
        calibrated = graph.add(Operation::Repair(prepared), &[calibrated, input])?;
        finish!();
    }
    if settings.highlight_restoration
        && settings.highlight_mode == crate::render::HighlightMode::ColorField
    {
        let prepared = profile.measure("prepare color-field highlights", || {
            color_field::prepare(
                &source.metadata,
                &settings.white_balance,
                calibration_params,
                settings.repair.is_some(),
                settings.color_field,
            )
        })?;
        calibrated = graph.add(
            Operation::ColorField(prepared),
            &[calibrated, input, black, unrepaired],
        )?;
        finish!();
    } else if settings.highlight_restoration {
        let prepared = match highlight_params {
            Some(prepared) => prepared,
            None => Arc::new(profile.measure("prepare highlight restoration", || {
                highlight::prepare(&source.metadata, settings.highlight_threshold_ev)
            })?),
        };
        calibrated = graph.add(Operation::Highlights(prepared), &[calibrated])?;
        finish!();
    }
    let mut couplings = None;
    if settings.despeckle {
        let recipe = profile.measure("prepare noise couplings", || {
            crate::noise::couplings(
                &source.metadata,
                &settings.white_balance,
                settings.sensor_noise_controls(),
            )
        })?;
        couplings = Some(recipe);
        let prepared = profile.measure("prepare despeckle", || {
            despeckle::prepare(&source.metadata, calibration_gains, recipe.despeckle_scale)?
                .with_controls(settings.merrill_speckles)
        })?;
        calibrated = graph.add(Operation::Despeckle(prepared), &[calibrated, input])?;
        finish!();
    }
    // Fast and Full share this native boundary. Keep it through refinement so
    // a later edit's Fast response does not repeat sensor repair and highlights.
    if policy.retain_calibrated {
        graph.retain_for_session(calibrated);
    }
    // Reduced browsing omits the expensive native row-bias estimate. Keep the
    // requested edit intact so Full and inspection still apply its correction.
    if policy.reference_rows {
        let prepared = profile.measure("prepare row correction", || {
            banding::prepare(&source.metadata, calibration_gains)
        })?;
        calibrated = graph.add(Operation::Banding(prepared), &[calibrated])?;
        finish!();
    }
    Ok(Output::Ready {
        recipe: BuiltRecipe {
            graph,
            final_id: calibrated,
            params,
            rgb_extent: [params.source[0], params.source[1]],
            rgb_bounds: params.crop,
        },
        couplings,
    })
}
