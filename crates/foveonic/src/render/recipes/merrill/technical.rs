//! Builds Merrill sensor corrections and technical color with lazy inspection preparation.

use super::super::{requested, BuiltRecipe};
use super::{dng_base, preview, region};
use crate::color_space::ColorSpace;
use crate::nodes::{
    color::{color_dq, color_mode},
    sensor::{banding, calibration, camera_repair, color_field, despeckle, highlight},
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

pub(super) fn build(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    names: &[&str],
    mode: Option<&str>,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<Prepared> {
    let space = ColorSpace::from_metadata(&source.metadata)?;
    let mut params = calibration::encode(&source.metadata, space)?;
    // Preview policy changes the working grid. Inspection and DNG keep native geometry.
    let preview = (settings.preview_quality.is_reduced()
        && mode.is_some()
        && !settings.is_monochrome()
        && (names == ["final"] || names == ["linear.base"])
        && settings.preview_region.is_none())
    .then(|| preview::Grid::new(params, settings.output_size))
    .transpose()?;
    if let Some(grid) = &preview {
        params = grid.encode;
    }
    if let Some(size) = settings.output_size {
        params.output[..2].copy_from_slice(&size);
    }
    let region = settings
        .preview_region
        .filter(|_| {
            mode.is_some()
                && !settings.is_monochrome()
                && (names == ["final"] || names == ["linear.base"])
        })
        .map(|rect| {
            let shared = super::super::linear_edits::support(settings);
            if shared > 0 {
                let (tonal, alignment) = if settings.tonal_ranges.is_neutral() {
                    (0, 1)
                } else {
                    (
                        crate::nodes::finish::tone_equalizer::support(),
                        crate::nodes::finish::tone_equalizer::GUIDE_ALIGNMENT,
                    )
                };
                region::Prepared::with_finish_support(
                    params,
                    rect,
                    shared + tonal + region::FINISH_SUPPORT,
                    alignment,
                )
            } else if !settings.tonal_ranges.is_neutral() {
                region::Prepared::with_tonal_support(params, rect)
            } else {
                region::Prepared::new(params, rect)
            }
        })
        .transpose()?;
    if let Some(region) = &region {
        params = region.encode;
    }
    engine.check_extent([params.output[0], params.output[1]])?;
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
                    rgb_extent: [params.source[0], params.source[1]],
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
    let mut calibrated = graph.add(Operation::Calibrate(calibration), &[input, black])?;
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
            despeckle::prepare(&source.metadata, calibration_gains, recipe.despeckle_scale)
        })?;
        calibrated = graph.add(Operation::Despeckle(prepared), &[calibrated])?;
        finish!();
    }
    // Fast and Full share this native boundary. Keep it through refinement so
    // a later edit's Fast response does not repeat sensor repair and highlights.
    if mode.is_some()
        && (names == ["final"] || names == ["linear.base"])
        && !settings.is_monochrome()
        && preview::available(&source.metadata)
    {
        graph.retain_for_session(calibrated);
    }
    // Reduced browsing omits the expensive native row-bias estimate. Keep the
    // requested edit intact so Full and inspection still apply its correction.
    if settings.row_correction && settings.processing_sections.corrections && preview.is_none() {
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
                rgb_extent: [params.source[0], params.source[1]],
            },
            names,
            profile,
            execution,
        );
    }
    if let Some(grid) = &preview {
        calibrated = graph.add(Operation::ReduceLayers(grid.reduction), &[calibrated])?;
    }
    // Smart's draft omits optional reconstruction. Standalone Fast applies the
    // requested controls on its reduced grid without changing the saved edits.
    let draft =
        preview.is_some() && settings.preview_quality == crate::render::PreviewQuality::Draft;
    let noise_controls = settings.merrill_reconstruction().filter(|_| !draft);
    if let Some(region) = &region {
        graph.retain_for_session(calibrated);
        if noise_controls.is_none() {
            calibrated = graph.add(
                Operation::ImageRegion {
                    prepared: crate::nodes::finish::image_region::Prepared::new(
                        [source.metadata.width as u32, source.metadata.height as u32],
                        region.physical,
                        3,
                    )?,
                    data: crate::graph::DataType::CalibratedLayers,
                    output: "region.calibrated",
                },
                &[calibrated],
            )?;
        }
    }
    let reconstruction = if let Some(controls) = noise_controls {
        let effects = controls.merrill_effects();
        let native_params = profile.measure("prepare reference noise", || {
            crate::noise::prepare(&source.metadata, &settings.white_balance, controls)
        })?;
        let params = if let Some(grid) = &preview {
            grid.noise(native_params)?
        } else if let Some(region) = &region {
            native_params.for_region(region.physical)?
        } else {
            native_params
        };
        let base_inputs = if params.uses_global_context() {
            let context = graph.add(Operation::NoiseContext(params.full_frame()), &[calibrated])?;
            let edges = graph.named_output(context, "noise.global-edges")?;
            if region.is_some() {
                graph.retain_for_session(context);
                graph.retain_for_session(edges);
            }
            vec![context, edges]
        } else {
            vec![calibrated]
        };
        let base = graph.add(Operation::NoiseBase(params), &base_inputs)?;
        finish!();
        let column_params = if effects.noise {
            profile.measure("prepare column cuts", || {
                if let Some(grid) = &preview {
                    grid.noise(crate::noise::column_parameters(
                        &source.metadata,
                        native_params,
                    )?)
                } else {
                    crate::noise::column_parameters(&source.metadata, params)
                }
            })?
        } else {
            params
        };
        let conditioned = graph.add(
            Operation::Column {
                params: column_params,
                enabled: effects.noise,
            },
            &[calibrated],
        )?;
        finish!();
        let prepared = crate::nodes::color::texture::Prepared::new(
            controls,
            params.source[0] as f32 / source.metadata.width as f32,
        )?;
        let inspect_ratio = names
            .iter()
            .any(|name| matches!(*name, "detail.raw-ratio" | "detail.ratio" | "pattern.mask"));
        let ratio = if prepared.needs_ratio() || inspect_ratio {
            let ratio = graph.add(Operation::Ratio(params), &[conditioned, base])?;
            finish!();
            let pattern_recipe = profile.measure("prepare pattern recipe", || {
                crate::noise::pattern_parameters(&source.metadata, params)
            })?;
            let seed = graph.named_output(base, "noise.pattern-seed-tiles")?;
            let pattern = graph.add(
                Operation::Pattern {
                    params,
                    prepared: pattern_recipe,
                },
                &[ratio, seed],
            )?;
            finish!();
            Some(pattern)
        } else {
            None
        };
        let mut texture_inputs = vec![conditioned];
        if prepared.needs_base() {
            texture_inputs.push(base);
        }
        if prepared.needs_ratio() {
            texture_inputs.push(ratio.ok_or("missing luminance conditioning")?);
        }
        let restored = graph.add(Operation::Texture { params, prepared }, &texture_inputs)?;
        finish!();
        Some((params, restored))
    } else {
        None
    };
    let mode = match mode {
        Some(name) => profile.measure("prepare color mode", || {
            color_mode::prepare(&source.metadata, name)
        })?,
        None => dng_base::unselected_mode(),
    };
    let mut color = profile.measure("prepare color matrix", || {
        calibration::color(&source.metadata, &settings.white_balance, &mode)
    })?;
    if preview.is_some() || region.is_some() {
        color.source[..2].copy_from_slice(&params.source[..2]);
    }
    let mut linear = if let Some((params, restored)) = reconstruction {
        graph.add(
            Operation::TiledColor {
                params,
                color,
                space,
            },
            &[restored],
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
    if !draft && settings.color_dq_selected() {
        let gain = if let Some(gain) = settings.color_dq_gain {
            gain
        } else {
            let recipe = match couplings {
                Some(recipe) => recipe,
                None => profile.measure("prepare noise couplings", || {
                    crate::noise::couplings(
                        &source.metadata,
                        &settings.white_balance,
                        settings.sensor_noise_controls(),
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
        linear = if let Some((params, _)) = reconstruction {
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
    if let Some((params, _)) = reconstruction {
        linear = graph.add(Operation::ProjectColorTiles { params, space }, &[linear])?;
        finish!();
    }
    // Final sharpening uses the actual reconstructed RGB, independently of
    // denoising and texture restoration. Inspection can request it without sharpening.
    let signal = if settings.sharpen_selected() || names.contains(&"detail.signal") {
        let signal = graph.add(Operation::SharpSignal { space }, &[linear])?;
        finish!();
        Some(signal)
    } else {
        None
    };
    if settings.sharpen_selected() {
        let amount = profile.measure("prepare sharpening amount", || {
            settings.effective_sharpen_amount(&source.metadata)
        })?;
        let signal = signal.ok_or("missing sharpening signal")?;
        linear = graph.add(Operation::Sharpen { amount, space }, &[linear, signal])?;
        finish!();
    }
    Ok(Prepared::Linear(
        BuiltRecipe {
            graph,
            final_id: linear,
            params,
            rgb_extent: [params.source[0], params.source[1]],
        },
        mode,
    ))
}
