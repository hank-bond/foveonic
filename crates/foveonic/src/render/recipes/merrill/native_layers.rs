//! Composes Merrill native layer corrections before Color and Monochrome diverge.

use super::super::{requested, BuiltRecipe};
use super::layer_noise;
use crate::{
    graph::{Graph, Operation},
    interactive::Execution,
    nodes::sensor::{banding, calibration, color_field, despeckle},
    profile::Profile,
    render::Settings,
    source::{Metadata, Result},
};
use std::sync::Arc;

/// Selected native corrections are independent of the working grid.
pub(super) struct Policy {
    pub multiscale: bool,
    pub directional_banding: bool,
    pub reference_rows: bool,
    pub retain_calibrated: bool,
}
impl Policy {
    pub fn new(settings: &Settings, retain_calibrated: bool) -> Self {
        Self {
            multiscale: settings.multiscale_noise_selected(),
            directional_banding: settings.directional_banding_selected(),
            reference_rows: settings.reference_rows_selected(),
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
    metadata: &Arc<Metadata>,
    settings: &Settings,
    params: calibration::EncodeParams,
    policy: &Policy,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<Output> {
    let mut graph = Graph::default();
    let input = graph.add(Operation::Source, &[])?;
    graph.set_view(input, super::super::camera_view(params));
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
        let prepared = profile.measure("prepare original saturation", || {
            crate::nodes::sensor::highlight::deferred(metadata, settings.highlight_threshold_ev)
                .map(Arc::new)
        })?;
        let density = graph.add(Operation::HighlightDensity(prepared.clone()), &[])?;
        graph.set_view(density, super::super::camera_view(params));
        highlight_params = Some(prepared);
        finish!();
    }
    let black_params = profile.measure("prepare black", || calibration::black(metadata))?;
    let black = graph.add(Operation::Black(black_params), &[input])?;
    finish!();
    let calibration = profile.measure("prepare calibration", || {
        calibration::prepare(metadata, &settings.white_balance)
    })?;
    let calibration_gains = std::array::from_fn(|c| calibration.params.gains[c]);
    let calibration_params = calibration.params;
    let defect_map = engine
        .noise_calibration
        .as_ref()
        .filter(|_| settings.processing_sections.noise && settings.despeckle)
        .and_then(|profile| profile.matching_defects(metadata))
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
    let model = engine.noise_model(metadata)?;
    let corrected = if policy.multiscale || policy.directional_banding {
        let (shields, filter) = profile.measure("prepare multiscale noise", || {
            layer_noise::prepare(
                metadata,
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
                layer_noise::prepare_row_guard(metadata, model)
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
            execution.prepare_repair(metadata, threshold)
        })?;
        calibrated = graph.add(Operation::Repair(prepared), &[calibrated, input])?;
        finish!();
    }
    if settings.highlight_restoration
        && settings.highlight_mode == crate::render::HighlightMode::ColorField
    {
        let prepared = profile.measure("prepare color-field highlights", || {
            color_field::prepare(
                metadata,
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
            None => profile.measure("prepare highlight restoration", || {
                crate::nodes::sensor::highlight::deferred(metadata, settings.highlight_threshold_ev)
                    .map(Arc::new)
            })?,
        };
        calibrated = graph.add(Operation::Highlights(prepared), &[calibrated])?;
        finish!();
    }
    let mut couplings = None;
    if settings.despeckle {
        let recipe = profile.measure("prepare noise couplings", || {
            crate::noise::couplings(
                metadata,
                &settings.white_balance,
                settings.sensor_noise_controls(),
            )
        })?;
        couplings = Some(recipe);
        let prepared = profile.measure("prepare despeckle", || {
            despeckle::prepare(metadata, calibration_gains, recipe.despeckle_scale)?
                .with_controls(settings.merrill_speckles)
        })?;
        calibrated = graph.add(Operation::Despeckle(prepared), &[calibrated, input])?;
        finish!();
    }
    // Keep the native boundary through refinement so returning to a sampled
    // overview does not repeat sensor repair and highlights.
    if policy.retain_calibrated {
        graph.retain_for_session(calibrated);
    }
    // Native coordinate corrections precede sensor pooling at every scale.
    if policy.reference_rows {
        let prepared = profile.measure("prepare row correction", || {
            banding::prepare(metadata, calibration_gains)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{MerrillBandingMethod, Scale};

    #[test]
    fn every_working_scale_keeps_selected_native_rows() {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.row_correction = true;
        settings.merrill_banding.enabled = true;
        settings.merrill_banding.method = MerrillBandingMethod::Reference;
        settings.reconstruction = Some(Default::default());
        for quality in [
            Scale::NATIVE,
            Scale::new(2).unwrap(),
            Scale::new(4).unwrap(),
            Scale::new(8).unwrap(),
        ] {
            settings.working_scale = quality;
            let policy = Policy::new(&settings, false);
            assert!(policy.reference_rows);
        }
        settings.processing_sections.noise = false;
        assert!(!Policy::new(&settings, false).reference_rows);
    }
}
