//! Coordinates Merrill native layers, tiled reconstruction and technical RGB finishing.

use super::super::{requested, BuiltRecipe};
use super::{dng_base, native_layers, preview, reconstruction, region};
use crate::color_space::ColorSpace;
use crate::nodes::{
    color::{color_dq, color_mode},
    sensor::calibration,
};
use crate::{
    graph::Operation,
    interactive::Execution,
    profile::Profile,
    render::{Engine, Settings},
    source::{Result, Source},
};

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
            !settings.adaptive_tone_active()
                && mode.is_some()
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
    let policy = native_layers::Policy::new(
        settings,
        preview.is_some(),
        mode.is_some()
            && (names == ["final"] || names == ["linear.base"])
            && !settings.is_monochrome()
            && preview::available(&source.metadata),
    );
    let (recipe, couplings) = match native_layers::build(
        engine, source, settings, params, &policy, names, profile, execution,
    )? {
        native_layers::Output::Requested(recipe) => return Ok(Prepared::Requested(recipe)),
        native_layers::Output::Ready { recipe, couplings } => (recipe, couplings),
    };
    let mut graph = recipe.graph;
    let mut calibrated = recipe.final_id;
    let directional_banding_selected = policy.directional_banding;
    macro_rules! finish {
        () => {
            execution.check()?;
            if requested(&graph, names) {
                return Ok(Prepared::Requested(BuiltRecipe {
                    graph,
                    final_id: calibrated,
                    params,
                    rgb_extent: [params.source[0], params.source[1]],
                    rgb_bounds: params.crop,
                }));
            }
        };
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
                rgb_bounds: params.crop,
            },
            names,
            profile,
            execution,
        );
    }
    let native_calibrated = calibrated;
    if let Some(grid) = &preview {
        calibrated = graph.add(Operation::ReduceLayers(grid.reduction), &[calibrated])?;
    }
    let noise_controls = settings.merrill_reconstruction();
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
    let reconstruction = if let Some(controls) = noise_controls {
        let result = reconstruction::append(
            reconstruction::Request {
                source,
                settings,
                names,
                color,
                space,
                preview: preview.as_ref(),
                region: region.as_ref(),
                directional_banding: directional_banding_selected,
                native_calibrated,
            },
            &mut graph,
            calibrated,
            controls,
            profile,
            execution,
        )?;
        match result {
            None => {
                return Ok(Prepared::Requested(BuiltRecipe {
                    graph,
                    final_id: calibrated,
                    params,
                    rgb_extent: [params.source[0], params.source[1]],
                    rgb_bounds: params.crop,
                }));
            }
            Some(reconstruction::Reconstructed { params, tiles }) => Some((params, tiles)),
        }
    } else {
        None
    };
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
    if !settings.neutralize_shadows.is_neutral() {
        linear = graph.add(
            Operation::ImageAlias {
                dehaze_basis: None,
                data: crate::graph::DataType::LinearRgb(space),
                output: "shadow-neutralize.input",
            },
            &[linear],
        )?;
        finish!();
        let prepared = crate::nodes::finish::shadow_neutralize::Params::new(
            settings.neutralize_shadows,
            space,
            if region.is_some() {
                1.
            } else {
                params.source[0] as f32 / source.metadata.width as f32
            },
        );
        let guide = graph.add(
            Operation::ShadowNeutralGuide {
                params: prepared,
                space,
            },
            &[linear],
        )?;
        linear = graph.add(
            Operation::ShadowNeutralize {
                params: prepared,
                space,
            },
            &[linear, guide],
        )?;
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
            rgb_bounds: params.crop,
        },
        mode,
    ))
}
