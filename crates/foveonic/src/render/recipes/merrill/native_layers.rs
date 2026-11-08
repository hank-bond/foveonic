//! Composes Merrill native layer corrections before Color and Monochrome diverge.

use super::super::{requested, BuiltRecipe};
use super::layer_noise;
use crate::{
    graph::{Graph, Operation},
    interactive::Execution,
    nodes::sensor::{banding, calibration, color_field, despeckle, highlight, multiscale},
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

// This short-lived recipe handoff stays on the stack.
#[allow(clippy::large_enum_variant)]
pub(super) enum Output {
    Requested(BuiltRecipe),
    Ready {
        recipe: BuiltRecipe,
        highlights: Option<Highlights>,
        calibration_gains: [f32; 3],
        sensor_codes: usize,
        multiscale: Option<Arc<multiscale::Prepared>>,
    },
}

/// Recovery follows reduction with original camera clipping evidence on the
/// working grid. Black levels remain a three-value table.
// A transient recipe handoff keeps its small preparation inline.
#[allow(clippy::large_enum_variant)]
pub(super) enum Highlights {
    Standard {
        prepared: Arc<highlight::Prepared>,
    },
    ColorField {
        prepared: color_field::Prepared,
        original: usize,
        black: usize,
        unrepaired: usize,
        repaired: usize,
    },
}
impl Highlights {
    pub fn append(
        self,
        graph: &mut Graph,
        input: usize,
        grid: Option<&super::preview::Grid>,
    ) -> Result<usize> {
        let working = grid.map(|g| g.grid);
        match self {
            Self::Standard { prepared } => graph.add(
                Operation::Highlights(Arc::new((*prepared).clone().on_grid(working))),
                &[input],
            ),
            Self::ColorField {
                prepared,
                original,
                black,
                unrepaired,
                repaired,
            } => {
                // Recovery still needs the measured scalar table after native
                // sensor buffers have been released.
                graph.retain_for_seal(black);
                let (original, unrepaired, repaired) = if let Some(grid) = grid {
                    (
                        grid.append(graph, original)?,
                        grid.append(graph, unrepaired)?,
                        grid.append(graph, repaired)?,
                    )
                } else {
                    (original, unrepaired, repaired)
                };
                graph.add(
                    Operation::ColorField(prepared.on_grid(working)),
                    &[input, original, black, unrepaired, repaired],
                )
            }
        }
    }
}

/// Each speckle input follows the same sampling grid. Only Compact needs codes.
pub(super) struct Speckles {
    prepared: despeckle::Prepared,
    sensor_codes: usize,
}
impl Speckles {
    pub fn append(
        self,
        graph: &mut Graph,
        input: usize,
        grid: Option<&super::preview::Grid>,
    ) -> Result<usize> {
        let mut inputs = vec![input];
        if self.prepared.needs_sensor_codes() {
            inputs.push(match grid {
                Some(grid) => grid.append(graph, self.sensor_codes)?,
                None => self.sensor_codes,
            });
        }
        graph.add(
            Operation::Despeckle(self.prepared.on_grid(grid.map(|grid| grid.grid))),
            &inputs,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    engine: &crate::render::Engine,
    metadata: &Arc<Metadata>,
    settings: &Settings,
    params: calibration::EncodeParams,
    policy: &Policy,
    grid: Option<crate::render::WorkingGrid>,
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
    let mut working_multiscale = None;
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
            if let Some(grid) = grid {
                working_multiscale = Some(Arc::new(
                    filter.on_grid(grid, Arc::new(calibration.clone())),
                ));
                corrected
            } else {
                let filtered = graph.add(
                    Operation::Multiscale(Arc::new(filter)),
                    &[corrected, input, black],
                )?;
                finish!();
                filtered
            }
        } else {
            corrected
        }
    } else {
        working
    };
    // The filter preserves original clipped triplets. Check mapped outliers in
    // those triplets again without weakening the immutable clipping evidence.
    let corrected = if let Some(map) = defect_map.filter(|_| policy.multiscale && grid.is_none()) {
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
    let highlights = if settings.highlight_restoration
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
        Some(Highlights::ColorField {
            prepared,
            original: input,
            black,
            unrepaired,
            repaired: calibrated,
        })
    } else if settings.highlight_restoration {
        let prepared = match highlight_params {
            Some(prepared) => prepared,
            None => profile.measure("prepare highlight restoration", || {
                crate::nodes::sensor::highlight::deferred(metadata, settings.highlight_threshold_ev)
                    .map(Arc::new)
            })?,
        };
        Some(Highlights::Standard { prepared })
    } else {
        None
    };
    Ok(Output::Ready {
        recipe: BuiltRecipe {
            graph,
            final_id: calibrated,
            params,
            rgb_extent: [params.source[0], params.source[1]],
            rgb_bounds: params.crop,
        },
        highlights,
        calibration_gains,
        sensor_codes: input,
        multiscale: working_multiscale,
    })
}

/// Prepare later spatial corrections only after earlier requested outputs exist.
pub(super) fn prepare_working(
    metadata: &Metadata,
    settings: &Settings,
    calibration_gains: [f32; 3],
    sensor_codes: usize,
    reference_rows: bool,
    profile: &mut Profile,
) -> Result<(
    Option<crate::noise::Couplings>,
    Option<banding::Description>,
    Option<Speckles>,
)> {
    let mut couplings = None;
    let speckles = if settings.despeckle {
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
        Some(Speckles {
            prepared,
            sensor_codes,
        })
    } else {
        None
    };
    let rows = if reference_rows {
        Some(profile.measure("prepare row correction", || {
            banding::describe(metadata, calibration_gains)
        })?)
    } else {
        None
    };
    Ok((couplings, rows, speckles))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{MerrillBandingMethod, Scale};

    #[test]
    fn row_correction_stays_selected_at_every_working_scale() {
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
