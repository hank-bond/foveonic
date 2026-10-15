//! Builds the shared original-source technical graph with lazy inspection preparation.

use super::{dng_base, requested, BuiltRecipe};
use crate::nodes::{
    color::{color_dq, color_mode},
    sensor::{
        banding,
        calibration::{self, ColorSpace},
        camera_repair, despeckle, highlight,
    },
};
use crate::{
    graph::{Graph, Operation},
    interactive::Execution,
    profile::Profile,
    render::{Engine, Settings},
    source::{Result, Source},
};
use std::sync::Arc;

pub(super) enum Prepared {
    Requested(BuiltRecipe),
    Linear(BuiltRecipe, color_mode::Settings),
}

impl Engine {
    pub(super) fn build_technical(
        &self,
        source: &Source,
        settings: &Settings,
        names: &[&str],
        mode: Option<&str>,
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<Prepared> {
        let space = ColorSpace::from_metadata(&source.metadata)?;
        let mut params = calibration::encode(&source.metadata, space)?;
        if let Some(size) = settings.output_size {
            params.output[..2].copy_from_slice(&size);
        }
        self.check_extent([params.output[0], params.output[1]])?;
        let mut graph = Graph::default();
        let input = graph.add(Operation::Source, &[])?;
        macro_rules! finish {
            () => {
                execution.check()?;
                if requested(&graph, names) {
                    return Ok(Prepared::Requested(BuiltRecipe {
                        graph,
                        final_id: input,
                        params,
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
        let black_params =
            profile.measure("prepare black", || calibration::black(&source.metadata))?;
        let black = graph.add(Operation::Black(black_params), &[input])?;
        finish!();
        let calibration = profile.measure("prepare calibration", || {
            calibration::prepare(&source.metadata, &settings.white_balance)
        })?;
        let calibration_gains = std::array::from_fn(|c| calibration.params.gains[c]);
        let mut calibrated = graph.add(Operation::Calibrate(calibration), &[input, black])?;
        finish!();
        if let Some(threshold) = settings.repair {
            let prepared = profile.measure("prepare repair records", || {
                camera_repair::prepare(&source.metadata, threshold)
            })?;
            calibrated = graph.add(Operation::Repair(prepared), &[calibrated, input])?;
            finish!();
        }
        if settings.highlight_restoration {
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
                despeckle::prepare(&source.metadata, calibration_gains, recipe.despeckle_scale)
            })?;
            calibrated = graph.add(Operation::Despeckle(prepared), &[calibrated])?;
            finish!();
        }
        if settings.row_correction {
            let prepared = profile.measure("prepare row correction", || {
                banding::prepare(&source.metadata, calibration_gains)
            })?;
            calibrated = graph.add(Operation::Banding(prepared), &[calibrated])?;
            finish!();
        }
        if settings.is_monochrome() {
            return super::monochrome::build(
                source,
                settings,
                BuiltRecipe {
                    graph,
                    final_id: calibrated,
                    params,
                },
                names,
                profile,
                execution,
            );
        }
        let reconstruction = if let Some(controls) = settings.reconstruction {
            let params = profile.measure("prepare reference noise", || {
                crate::noise::prepare(&source.metadata, &settings.white_balance, controls)
            })?;
            let base = graph.add(Operation::NoiseBase(params), &[calibrated])?;
            finish!();
            let column_params = profile.measure("prepare column cuts", || {
                crate::noise::column_parameters(&source.metadata, params)
            })?;
            let conditioned = graph.add(Operation::Column(column_params), &[calibrated])?;
            finish!();
            let ratio = graph.add(Operation::Ratio(params), &[conditioned, base])?;
            finish!();
            let prepared = profile.measure("prepare pattern recipe", || {
                crate::noise::pattern_parameters(&source.metadata, params, controls)
            })?;
            let pattern = graph.add(Operation::Pattern { params, prepared }, &[ratio, base + 3])?;
            finish!();
            let detail = graph.add(Operation::Detail(params), &[conditioned, base, pattern])?;
            finish!();
            Some((params, base, detail, pattern))
        } else {
            None
        };
        let mode = match mode {
            Some(name) => profile.measure("prepare color mode", || {
                color_mode::prepare(&source.metadata, name)
            })?,
            None => dng_base::unselected_mode(),
        };
        let color = profile.measure("prepare color matrix", || {
            calibration::color(&source.metadata, &settings.white_balance, &mode)
        })?;
        let mut linear = if let Some((params, base, _, _)) = reconstruction {
            graph.add(
                Operation::TiledColor {
                    params,
                    color,
                    space,
                },
                &[base],
            )?
        } else {
            graph.add(
                Operation::Color {
                    params: color,
                    space,
                },
                &[calibrated],
            )?
        };
        finish!();
        if settings.color_dq_selected() {
            let gain = if let Some(gain) = settings.color_dq_gain {
                gain
            } else {
                let recipe = match couplings {
                    Some(recipe) => recipe,
                    None => profile.measure("prepare noise couplings", || {
                        crate::noise::couplings(
                            &source.metadata,
                            &settings.white_balance,
                            settings.reconstruction.unwrap_or_default(),
                        )
                    })?,
                };
                crate::noise::processing_gain(&source.metadata)?
                    .1
                    .min(recipe.color_dq_limit)
            };
            let table = profile.measure("prepare ColorDQ", || {
                color_dq::prepare(&source.metadata, gain)
            })?;
            linear = if let Some((params, _, _, _)) = reconstruction {
                graph.add(
                    Operation::TiledColorDq {
                        params,
                        table,
                        space,
                    },
                    &[linear],
                )?
            } else {
                graph.add(Operation::ColorDq { table, space }, &[linear])?
            };
            finish!();
        }
        if let Some((params, _, detail, ratio)) = reconstruction {
            linear = graph.add(
                Operation::Reconstruct { params, space },
                &[linear, detail, ratio],
            )?;
            finish!();
        }
        if settings.sharpen_amount > 0. {
            let signal = if reconstruction.is_some() {
                linear + 1
            } else {
                let signal = graph.add(Operation::SharpSignal { space }, &[linear])?;
                finish!();
                signal
            };
            linear = graph.add(
                Operation::Sharpen {
                    amount: settings.sharpen_amount,
                    space,
                },
                &[linear, signal],
            )?;
            finish!();
        }
        Ok(Prepared::Linear(
            BuiltRecipe {
                graph,
                final_id: linear,
                params,
            },
            mode,
        ))
    }
}
