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
    source::Result,
};

// This transient recipe handoff stays on the stack rather than adding a heap owner.
#[allow(clippy::large_enum_variant)]
pub(super) enum Prepared {
    Requested(BuiltRecipe),
    Linear(BuiltRecipe, color_mode::Settings, Option<preview::Grid>),
}

pub(super) fn build(
    engine: &Engine,
    source: super::super::RecipeInput<'_>,
    settings: &Settings,
    names: &[&str],
    mode: Option<&str>,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<Prepared> {
    let space = ColorSpace::from_metadata(source.metadata())?;
    let mut params = calibration::encode(source.metadata(), space)?;
    let native_params = params;
    // Working scale sets sensor sampling. Native DNG requests select identity sampling.
    let preview = (settings.working_scale.is_reduced()
        && mode.is_some()
        && settings.preview_region.is_none())
    .then(|| {
        preview::Grid::new(
            params,
            settings.processing_output_size(),
            settings.working_scale,
        )
    })
    .transpose()?;
    if let Some(grid) = &preview {
        params = grid.encode;
    }
    if let Some(size) = settings.processing_output_size() {
        params.output[..2].copy_from_slice(&size);
    }
    let region = settings
        .preview_region
        .filter(|_| mode.is_some() && (names == ["final"] || names == ["linear.base"]))
        .map(|rect| {
            let shared = super::super::linear_edits::support(settings);
            if shared > 0 || settings.is_monochrome() {
                let alignment =
                    if settings.adaptive_tone_active() || !settings.tonal_ranges.is_neutral() {
                        crate::nodes::finish::tone_equalizer::GUIDE_ALIGNMENT
                    } else {
                        1
                    };
                region::Prepared::with_finish_support(
                    params,
                    rect,
                    shared + region::FINISH_SUPPORT + super::monochrome::support(settings),
                    alignment,
                )
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
        mode.is_some()
            && (names == ["final"] || names == ["linear.base"])
            && settings.working_scale.is_native(),
    );
    let (recipe, highlights, calibration_gains, sensor_codes, multiscale, guarded_rows) =
        match native_layers::build(
            engine,
            source.metadata(),
            settings,
            native_params,
            &policy,
            preview.as_ref().map(|grid| grid.grid),
            names,
            profile,
            execution,
        )? {
            native_layers::Output::Requested(recipe) => return Ok(Prepared::Requested(recipe)),
            native_layers::Output::Ready {
                recipe,
                highlights,
                calibration_gains,
                sensor_codes,
                multiscale,
                rows,
            } => (
                recipe,
                highlights,
                calibration_gains,
                sensor_codes,
                multiscale,
                rows,
            ),
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
                    rgb_to_native: [[1., 0., 0.], [0., 1., 0.]],
                    rgb_extent: [params.source[0], params.source[1]],
                    rgb_bounds: params.crop,
                }));
            }
        };
    }
    if let Some(grid) = &preview {
        calibrated = grid.append(&mut graph, calibrated)?;
        if let Some(rows) = guarded_rows {
            calibrated = rows.append(&mut graph, calibrated, grid)?;
            finish!();
        }
    }
    if let Some(filter) = multiscale {
        calibrated = graph.add(Operation::Multiscale(filter), &[calibrated])?;
        finish!();
    }
    if let Some(highlights) = highlights {
        calibrated = highlights.append(&mut graph, calibrated, preview.as_ref())?;
        finish!();
    }
    let (couplings, rows, speckles) = native_layers::prepare_working(
        source.metadata(),
        settings,
        calibration_gains,
        sensor_codes,
        policy.reference_rows,
        profile,
    )?;
    if preview.is_none() {
        graph.retain_as_sensor_data(calibrated);
    }
    if policy.retain_calibrated {
        graph.retain_for_session(calibrated);
    }
    if settings.is_monochrome() {
        return match super::monochrome::build(
            source.metadata(),
            settings,
            BuiltRecipe {
                graph,
                final_id: calibrated,
                params,
                rgb_to_native: [[1., 0., 0.], [0., 1., 0.]],
                rgb_extent: [params.source[0], params.source[1]],
                rgb_bounds: params.crop,
            },
            names,
            profile,
            execution,
            preview.as_ref(),
            rows,
            speckles,
            region.as_ref(),
        )? {
            Prepared::Linear(recipe, mode, _) => Ok(Prepared::Linear(recipe, mode, preview)),
            requested => Ok(requested),
        };
    }
    if let Some(speckles) = speckles {
        calibrated = speckles.append(&mut graph, calibrated, preview.as_ref())?;
        finish!();
    }
    if let Some(rows) = rows {
        calibrated = graph.add(
            Operation::Banding(rows.on_grid(preview.as_ref().map(|grid| grid.grid))),
            &[calibrated],
        )?;
        finish!();
    }
    let noise_controls = settings.merrill_reconstruction();
    if let Some(region) = &region {
        graph.retain_for_session(calibrated);
        if noise_controls.is_none() {
            calibrated = graph.add(
                Operation::ImageRegion {
                    prepared: crate::nodes::finish::image_region::Prepared::new(
                        [
                            source.metadata().width as u32,
                            source.metadata().height as u32,
                        ],
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
    let prepare_color =
        |profile: &mut Profile| -> Result<(calibration::ColorParams, color_mode::Settings)> {
            let mode = match mode {
                Some(name) => profile.measure("prepare color mode", || {
                    color_mode::prepare(source.metadata(), name)
                })?,
                None => dng_base::unselected_mode(),
            };
            let mut color = profile.measure("prepare color matrix", || {
                calibration::color(source.metadata(), &settings.white_balance, &mode)
            })?;
            if preview.is_some() || region.is_some() {
                color.source[..2].copy_from_slice(&params.source[..2]);
            }
            Ok((color, mode))
        };
    let (reconstruction, color, mode) = if let Some(controls) = noise_controls {
        let result = reconstruction::append(
            reconstruction::Request {
                source,
                settings,
                names,
                space,
                preview: preview.as_ref(),
                region: region.as_ref(),
                directional_banding: directional_banding_selected,
                view: super::super::camera_view(params),
            },
            &mut graph,
            calibrated,
            controls,
            prepare_color,
            profile,
            execution,
        )?;
        match result {
            None => {
                return Ok(Prepared::Requested(BuiltRecipe {
                    graph,
                    final_id: calibrated,
                    params,
                    rgb_to_native: [[1., 0., 0.], [0., 1., 0.]],
                    rgb_extent: [params.source[0], params.source[1]],
                    rgb_bounds: params.crop,
                }));
            }
            Some(reconstruction::Reconstructed {
                params,
                tiles,
                color,
                mode,
            }) => (Some((params, tiles)), color, mode),
        }
    } else {
        let (color, mode) = prepare_color(profile)?;
        (None, color, mode)
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
                        source.metadata(),
                        &settings.white_balance,
                        settings.sensor_noise_controls(),
                    )
                })?,
            };
            crate::noise::processing_gain(source.metadata())?
                .1
                .min(recipe.color_dq_limit)
        };
        let table = profile.measure("prepare ColorDQ", || {
            color_dq::prepare(source.metadata(), gain)
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
                settings.working_scale.factor()
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
        let scale = settings.working_scale.factor();
        let signal = graph.add(Operation::SharpSignal { space, scale }, &[linear])?;
        finish!();
        Some(signal)
    } else {
        None
    };
    if settings.sharpen_selected() {
        let amount = profile.measure("prepare sharpening amount", || {
            super::applied_sharpen_amount(settings, source.metadata())
        })?;
        if amount != 0. || names.contains(&"sharpen.rgb") {
            let signal = signal.ok_or("missing sharpening signal")?;
            linear = graph.add(Operation::Sharpen { amount, space }, &[linear, signal])?;
            finish!();
        }
    }
    let recipe = BuiltRecipe {
        graph,
        final_id: linear,
        params,
        rgb_to_native: preview.as_ref().map_or_else(
            || {
                let origin = region
                    .as_ref()
                    .map_or([0, 0], |r| [r.physical[0], r.physical[1]]);
                [
                    [1., 0., f64::from(origin[0])],
                    [0., 1., f64::from(origin[1])],
                ]
            },
            |grid| grid.grid.native_mapping(),
        ),
        rgb_extent: [params.source[0], params.source[1]],
        rgb_bounds: region
            .as_ref()
            .map_or(params.crop, |region| region.rgb_bounds),
    };
    Ok(Prepared::Linear(recipe, mode, preview))
}
