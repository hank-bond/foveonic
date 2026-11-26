//! Shares Quattro technical reconstruction between photographic and editable DNG output.

mod appearance;
mod controls;
mod dng;
use controls::smooth_crisp_level;
pub(in crate::render) use controls::{
    apply_capture_defaults, captured_noise_controls, color_mode_presets, default_color_mode,
    default_noise_controls, default_rendering, default_sharpen_amount, default_white_balance,
    editing_available, editing_defaults, normalize_photographic, validate_photographic,
};
pub(crate) use dng::prepare_appearance as prepare_dng_appearance;
mod highlights;

mod calibration;
mod color_shading;
mod defects;
mod hlp;
mod inspection;
mod luma_repair;
mod mcc;
mod monochrome;
mod optical;
mod preview;
mod region;
mod repair;
pub(crate) mod resources;
mod sd;
mod tiger;

#[cfg(test)]
#[path = "quattro/portability_test.rs"]
mod portability_tests;

#[cfg(test)]
#[path = "quattro/preview_test.rs"]
mod preview_tests;

pub(crate) use inspection::{comparison_input, outputs};
mod stage1;
mod stage2;
mod stage2b;
mod stage3;

use super::{requested, BuiltRecipe};
use crate::{
    graph::{Graph, Operation},
    interactive::Execution,
    nodes::sensor::{calibration::EncodeParams, phase_extraction},
    render::{Engine, Settings},
    source::{Result, Source, SourceStorage},
};

// The reference color route leaves optional sparse HP correction disabled.
// Photographic highlight reconstruction remains a separate operation.
const COLOR_HP_CORRECTION: bool = false;
/// Camera component grids share one numerical scale without changing metadata.
pub(super) use crate::render::Scale as WorkingGrid;

/// Draft and smaller previews use the existing equal-resolution numerical route.
/// Source LOW still selects its own calibration independently of this geometry.
pub(super) fn same_resolution(metadata: &crate::source::Metadata, scale: WorkingGrid) -> bool {
    metadata.is_quattro_low() || scale.divisor() >= 4
}

pub(super) fn color_scale(metadata: &crate::source::Metadata, scale: WorkingGrid) -> WorkingGrid {
    if !metadata.is_quattro_low() && same_resolution(metadata, scale) {
        WorkingGrid::new(scale.divisor() / 2).expect("power-of-two component sampling")
    } else {
        scale
    }
}

pub(super) fn color_grid(
    metadata: &crate::source::Metadata,
    scale: WorkingGrid,
) -> Result<crate::render::WorkingGrid> {
    let component_ratio = if same_resolution(metadata, scale) {
        1
    } else {
        2
    };
    let alignment = if !metadata.is_quattro_low() && component_ratio == 1 {
        8
    } else {
        4
    };
    crate::render::WorkingGrid::new(
        calibration::color_extent(metadata)?,
        color_scale(metadata, scale),
        alignment,
    )?
    .with_halo(sensor_halo(scale) / component_ratio)
}

fn sensor_halo(scale: WorkingGrid) -> u32 {
    // The finite camera support composes noise, reconstruction and finishing.
    // Sixty-four working pixels also cover stencils whose minimum stays fixed.
    region::CAMERA_SUPPORT.div_ceil(scale.divisor()).max(64)
}
pub(super) fn output_grid(
    metadata: &crate::source::Metadata,
    scale: WorkingGrid,
) -> Result<crate::render::WorkingGrid> {
    if metadata.is_quattro_low() {
        return color_grid(metadata, scale);
    }
    crate::render::WorkingGrid::new([metadata.width as u32, metadata.height as u32], scale, 8)?
        .with_halo(sensor_halo(scale))
}
pub(super) fn pool_noise_energy(native: [u32; 2], scale: WorkingGrid) -> f64 {
    1. / (f64::from(native[0].min(scale.divisor())) * f64::from(native[1].min(scale.divisor())))
}

/// The aligned native regional adapter requires doubled component geometry.
pub(in crate::render) fn fast_available(metadata: &crate::source::Metadata) -> bool {
    !metadata.is_quattro_low()
        && (calibration::is_ordinary_dp(metadata) || calibration::is_interchangeable(metadata))
}

pub(super) fn build(
    engine: &Engine,
    source: super::RecipeInput<'_>,
    settings: &Settings,
    names: &[&str],
    profile: &mut crate::profile::Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let mut regional = settings.clone();
    let crop = region::expand_shared_edits(source.metadata(), &mut regional, names)?;
    let mut recipe = compose(engine, source, &regional, names, profile, execution, false)?;
    region::crop_shared_edits(&mut recipe, crop)?;
    Ok(recipe)
}

pub(super) fn build_dng(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    profile: &mut crate::profile::Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    compose(
        engine,
        super::RecipeInput::Original(source),
        settings,
        &["final"],
        profile,
        execution,
        true,
    )
}

fn compose(
    engine: &Engine,
    source: super::RecipeInput<'_>,
    settings: &Settings,
    names: &[&str],
    profile: &mut crate::profile::Profile,
    execution: &mut Execution<'_>,
    dng: bool,
) -> Result<BuiltRecipe> {
    let metadata = source.metadata();
    let mut selected = settings.clone();
    if !settings.processing_sections.corrections {
        selected.highlight_amount = Some(0.);
    }
    let noise_enabled = settings.processing_sections.noise
        && settings.reconstruction.is_none_or(|c| c.noise_enabled);
    let detail_enabled = settings.processing_sections.detail
        && settings.reconstruction.is_none_or(|c| c.detail_enabled);
    if !noise_enabled || !detail_enabled {
        let mut controls = selected
            .reconstruction
            .unwrap_or_else(controls::default_noise_controls);
        if !noise_enabled {
            controls.chroma = 0;
            controls.luminance = 0;
            selected.monochrome.noise = 0;
        }
        if !detail_enabled {
            controls.detail = 0.5;
            selected.monochrome_smoothness = Some(0.5);
        }
        selected.reconstruction = Some(controls);
    }
    let settings = &selected;
    let low = metadata.is_quattro_low();
    let storage = if low {
        SourceStorage::InterleavedRgb
    } else {
        SourceStorage::PackedRgbBluePhases
    };
    if source
        .original()
        .is_ok_and(|source| source.storage() != storage)
    {
        return Err("Quattro sensor storage does not match its resolution".into());
    }
    let grid = if dng {
        WorkingGrid::NATIVE
    } else {
        settings.working_scale
    };
    let reduced = grid.is_reduced();
    let early_reduction = grid.divisor() >= 4;
    let color_geometry = color_grid(metadata, grid)?;
    let output_geometry = output_grid(metadata, grid)?;
    let available = outputs(Some(metadata));
    if names.iter().any(|name| {
        !super::linear_edits::requested_output(name)
            && !available.iter().any(|output| output.name == *name)
    }) {
        return Err("requested output is unavailable for this Quattro input".into());
    }
    let [width, height] = calibration::color_extent(metadata)?;
    let mut graph = Graph::default();
    let input = graph.add(
        if low {
            Operation::Source
        } else {
            Operation::SourcePhaseCells
        },
        &[],
    )?;
    // Native regional requests can reuse the original sensor cells.
    if names == ["final"] && fast_available(metadata) && !reduced {
        graph.retain_for_session(input);
    }
    let raw_params = phase_extraction::Params::new(width, height)?;
    let raw = graph.add(
        Operation::PhaseExtraction(if low {
            raw_params.with_triplets()
        } else {
            raw_params
        }),
        &[input],
    )?;
    let sensor_view = |extent: [u32; 2]| crate::image::View {
        crop: [0, 0, extent[0], extent[1]],
        rotation: metadata.rotation,
        extent: if matches!(metadata.rotation, 90 | 270) {
            [extent[1], extent[0]]
        } else {
            extent
        },
    };
    let color_view = sensor_view([width, height]);
    let blue_view = sensor_view([metadata.width as u32, metadata.height as u32]);
    graph.set_view(raw, color_view);
    let raw_blue = graph.named_output(raw, "sensor.blue")?;
    graph.set_view(raw_blue, blue_view);
    let factor = if low { 1 } else { 2 };
    let mut params = EncodeParams {
        mapping: [[0.; 4]; 2],
        source: [width * factor, height * factor, metadata.rotation, 0],
        crop: [0, 0, width * factor, height * factor],
        output: [width * factor, height * factor, 0, 0],
    };
    graph.set_view(input, super::camera_view(params));
    macro_rules! finish {
        ($output:expr) => {
            execution.check()?;
            if requested(&graph, names) {
                return Ok(BuiltRecipe {
                    graph,
                    final_id: $output,
                    params,
                    rgb_to_native: [[1., 0., 0.], [0., 1., 0.]],
                    rgb_extent: [params.output[0], params.output[1]],
                    rgb_bounds: [0, 0, params.output[0], params.output[1]],
                });
            }
        };
    }
    finish!(raw);

    let calibrated_input = if let Some(prepared) = calibration::column_gain(metadata)? {
        graph.add(
            Operation::ColumnGain(std::sync::Arc::new(prepared)),
            &[input],
        )?
    } else {
        input
    };
    let extracted = graph.add(
        Operation::PhaseExtraction(calibration::extraction(metadata)?),
        &[calibrated_input],
    )?;
    graph.set_view(extracted, color_view);
    let extracted_blue = graph.named_output(extracted, "extraction.blue")?;
    graph.set_view(extracted_blue, blue_view);
    finish!(extracted);

    let is_tiger = tiger::supported(metadata);
    let is_interchangeable = calibration::is_interchangeable(metadata);
    let (sparse, sparse_blue) = if is_interchangeable {
        let rgb = graph.add(
            Operation::Fppd(std::sync::Arc::new(if is_tiger {
                tiger::chroma(metadata)?
            } else {
                sd::chroma(metadata)?
            })),
            &[extracted],
        )?;
        finish!(rgb);
        (rgb, graph.named_output(extracted, "extraction.blue")?)
    } else {
        let blue = graph.named_output(extracted, "extraction.blue")?;
        let (refreshed, corrected_blue) = if low {
            (extracted, blue)
        } else {
            let gain = calibration::phase_gain(metadata, engine.calibration_resources.as_deref())?;
            let corrected_blue =
                graph.add(Operation::PhaseGain(std::sync::Arc::new(gain)), &[blue])?;
            finish!(corrected_blue);

            let refreshed = graph.add(
                Operation::BlueRefresh(crate::nodes::sensor::blue_refresh::Params::new([
                    width, height,
                ])?),
                &[extracted, corrected_blue],
            )?;
            finish!(refreshed);
            (refreshed, corrected_blue)
        };

        let prepared = hlp::prepare(metadata)?;
        let calibrated = graph.add(
            Operation::HlpCalibration(std::sync::Arc::new(prepared)),
            &[refreshed, corrected_blue],
        )?;
        let blue = graph.named_output(calibrated, "hlp.blue")?;
        graph.set_view(blue, blue_view);
        finish!(calibrated);
        let prepared = repair::prepare(metadata, COLOR_HP_CORRECTION)?;
        let sparse = graph.add(
            Operation::SparseRepair(std::sync::Arc::new(prepared)),
            &[calibrated, blue],
        )?;
        let blue = graph.named_output(sparse, "hlp-repair.blue")?;
        graph.set_view(blue, blue_view);
        finish!(sparse);
        (sparse, blue)
    };
    // Factory chroma repair receives a separate initially empty mask. Sparse
    // repair writes another surface. Sharing those flags skips valid repairs.
    let mask = graph.add(Operation::EmptyDefectFlags([width, height]), &[])?;
    graph.set_view(mask, color_view);
    let prepared = defects::prepare(engine, metadata, profile)?;
    let single = graph.add(
        Operation::ChromaRepair {
            prepared: prepared.clone(),
            stage: crate::nodes::sensor::chroma_repair::Stage::Single,
        },
        &[sparse, mask],
    )?;
    finish!(single);
    let general = graph.add(
        Operation::ChromaRepair {
            prepared,
            stage: crate::nodes::sensor::chroma_repair::Stage::General,
        },
        &[single, mask],
    )?;
    finish!(general);
    let despeckled = graph.add(
        Operation::RgbDespeckle(std::sync::Arc::new(defects::prepare_despeckle(metadata)?)),
        &[general, mask],
    )?;
    finish!(despeckled);
    let cluster_controls = std::sync::Arc::new(defects::prepare_clusters(metadata)?);
    let chroma = graph.add(
        Operation::ChromaClusters(cluster_controls.clone()),
        &[despeckled, mask],
    )?;
    finish!(chroma);
    use crate::nodes::sensor::{
        blue_despeckle::Stage as Filter, ordered_blue_repair::Stage as Repair,
    };
    // Sparse phase-detection sites survive a 2x2 mean on both sd models.
    // Apply their own native-grid correction before any preview reduction.
    let phase_corrected_blue = if is_interchangeable && !low {
        let corrected = graph.add(
            Operation::Fppd(std::sync::Arc::new(if is_tiger {
                tiger::luma(metadata)?
            } else {
                sd::luma(metadata)?
            })),
            &[sparse_blue],
        )?;
        finish!(corrected);
        corrected
    } else {
        sparse_blue
    };
    let mismatch = if low {
        phase_corrected_blue
    } else {
        let blue = if is_interchangeable {
            if let Some(prepared) = sd::pattern(metadata)? {
                let corrected = graph.add(
                    Operation::BluePattern(std::sync::Arc::new(prepared)),
                    &[phase_corrected_blue],
                )?;
                finish!(corrected);
                corrected
            } else {
                phase_corrected_blue
            }
        } else {
            phase_corrected_blue
        };
        let directional = graph.add(
            Operation::OrderedBlueRepair(luma_repair::prepare_records(
                engine,
                metadata,
                Repair::Directional,
                profile,
            )?),
            &[blue],
        )?;
        finish!(directional);
        let directional_mask = graph.named_output(directional, "luma-repair.defects")?;
        // The selected reference SuperBlue worker has no reachable pixel writes.
        let offsets = graph.add(
            Operation::OrderedBlueRepair(luma_repair::prepare_records(
                engine,
                metadata,
                Repair::DarkOffsets,
                profile,
            )?),
            &[directional],
        )?;
        finish!(offsets);
        let dark_filtered = graph.add(
            Operation::BlueDespeckle(std::sync::Arc::new(luma_repair::prepare_despeckle(
                metadata,
                Filter::DarkPairs,
            )?)),
            &[offsets],
        )?;
        finish!(dark_filtered);
        let repaired = graph.add(
            Operation::OrderedBlueRepair(luma_repair::prepare_records(
                engine,
                metadata,
                Repair::General,
                profile,
            )?),
            &[dark_filtered, directional_mask],
        )?;
        finish!(repaired);
        let filtered = graph.add(
            Operation::BlueDespeckle(std::sync::Arc::new(luma_repair::prepare_despeckle(
                metadata,
                Filter::General,
            )?)),
            &[repaired],
        )?;
        finish!(filtered);
        let clusters = graph.add(
            Operation::LumaClusters(std::sync::Arc::new(luma_repair::prepare_clusters(
                metadata,
            )?)),
            &[filtered],
        )?;
        finish!(clusters);
        if !is_tiger && (calibration::normalization(metadata)? - 1.).abs() < f32::EPSILON {
            graph.add(
                Operation::BlueMismatch(std::sync::Arc::new(highlights::prepare_blue(metadata)?)),
                &[clusters],
            )?
        } else {
            clusters
        }
    };
    finish!(mismatch);
    // The DP1 optical correction belongs after factory repairs and before
    // highlight restoration. Other DP recipes keep the same RGB history.
    let optical_controls = optical::prepare(metadata)?.map(std::sync::Arc::new);
    let chroma = if let Some(prepared) = &optical_controls {
        graph.add(Operation::PurpleFringe(prepared.clone()), &[chroma])?
    } else {
        chroma
    };
    finish!(chroma);
    let restored = graph.add(
        Operation::HighlightRestoration(std::sync::Arc::new(highlights::prepare_restoration(
            metadata,
        )?)),
        &[chroma],
    )?;
    if is_tiger && names.contains(&"highlight-restoration.recovery-mask") {
        let flags = graph.named_output(restored, "highlight-restoration.recovery-flags")?;
        let mask = graph.add(
            Operation::MaskImage {
                params: crate::nodes::finish::mask_image::Params::new([width, height])?,
                output: "highlight-restoration.recovery-mask",
            },
            &[flags],
        )?;
        finish!(mask);
    }
    finish!(restored);
    let mismatch = if low {
        graph.add(Operation::ExtractBlue([width, height]), &[restored])?
    } else {
        mismatch
    };
    let color_mode = if settings.is_monochrome() || dng {
        "Standard"
    } else {
        &settings.color_mode
    };
    let mut controls = match settings.reconstruction {
        Some(controls) => controls,
        None => captured_noise_controls(metadata)?,
    };
    if settings.is_monochrome() {
        controls.luminance = settings.monochrome.noise;
        controls.detail = settings.monochrome_smoothness.unwrap_or(
            metadata
                .capture_adjustments()?
                .mono_smoothness
                .unwrap_or(0.5),
        );
    }
    let small_white_balance = if reduced {
        Some(if settings.is_monochrome() {
            "Auto".into()
        } else {
            settings.white_balance.clone()
        })
    } else {
        None
    };
    let native_highlight_mask = graph.named_output(restored, "highlight-restoration.mask")?;
    let native_recovery_mask = if is_tiger {
        Some(graph.named_output(restored, "highlight-restoration.recovery-flags")?)
    } else {
        None
    };
    let (noise_input, mismatch, highlight_mask, recovery_mask) = if early_reduction {
        let inputs = preview::build(
            &mut graph,
            metadata,
            grid,
            preview::NativeRepairs {
                rgb: general,
                defects: mask,
                clusters: cluster_controls,
                optical: optical_controls,
            },
            small_white_balance.as_deref().expect("reduced balance"),
        )?;
        (
            inputs.rgb,
            inputs.blue,
            inputs.highlight_mask,
            inputs.recovery_mask,
        )
    } else if reduced {
        use crate::nodes::sensor::pool::{Prepared, Signal};
        let (shaded, blue) = stage2::shade_preprocessing(
            &mut graph,
            metadata,
            restored,
            mismatch,
            small_white_balance.as_deref().expect("reduced balance"),
        )?;
        let mut pool =
            |input, native, geometry: crate::render::WorkingGrid, signal, scope| -> Result<usize> {
                let prepared = Prepared::new(native, geometry.scale.divisor(), signal)?
                    .with_extent(geometry.extent)?
                    .with_origin(geometry.origin)
                    .with_working_scale(grid);
                let output = graph.add_scoped(Operation::PoolSignal(prepared), &[input], scope)?;
                graph.retain_for_session(output);
                graph.retain_as_sensor_data(output);
                Ok(output)
            };
        let rgb = pool(
            shaded,
            [width, height],
            color_geometry,
            Signal::Rgb,
            "sensor-rgb-grid",
        )?;
        let blue_geometry = if low { color_geometry } else { output_geometry };
        let blue = pool(
            blue,
            [metadata.width as u32, metadata.height as u32],
            blue_geometry,
            Signal::Blue,
            "sensor-blue-grid",
        )?;
        let mask = pool(
            native_highlight_mask,
            [width, height],
            color_geometry,
            Signal::Mask,
            "sensor-highlight-grid",
        )?;
        let flags = native_recovery_mask
            .map(|flags| {
                pool(
                    flags,
                    [width, height],
                    color_geometry,
                    Signal::Flags,
                    "sensor-recovery-grid",
                )
            })
            .transpose()?;
        (rgb, blue, mask, flags)
    } else {
        for input in [restored, mismatch, native_highlight_mask]
            .into_iter()
            .chain(native_recovery_mask)
        {
            graph.retain_as_sensor_data(input);
        }
        (
            restored,
            mismatch,
            native_highlight_mask,
            native_recovery_mask,
        )
    };
    let noise = if settings.is_monochrome() {
        graph.add(
            Operation::ChannelNoise(std::sync::Arc::new(if grid.is_native() {
                monochrome::prepare_stage1(metadata)?
            } else {
                monochrome::prepare_stage1_on_grid(metadata, grid)?
            })),
            &[noise_input],
        )?
    } else {
        graph.add(
            Operation::TwoScaleNoise(std::sync::Arc::new(stage1::prepare_filter_on_grid(
                metadata,
                controls.chroma as u8,
                grid,
            )?)),
            &[noise_input],
        )?
    };
    finish!(noise);
    let white_balance = if settings.is_monochrome() {
        "Auto"
    } else {
        &settings.white_balance
    };

    let calibrated = graph.named_output(noise, "noise.source-after")?;
    let highlight_amount = settings.highlight_amount.unwrap_or(
        metadata
            .capture_adjustments()?
            .highlight_correction
            .unwrap_or(1.),
    );
    let response = if settings.is_monochrome() {
        monochrome::prepare_stage2(metadata, highlight_amount, color_mode)?
    } else {
        stage2::prepare(metadata, highlight_amount, white_balance, color_mode)?
    };
    let response = if reduced {
        response.with_extent(color_geometry.extent)?
    } else {
        response
    };
    let signals = (if reduced {
        stage2::build_reduced_signals
    } else {
        stage2::build_signals
    })(
        &mut graph,
        metadata,
        noise,
        calibrated,
        mismatch,
        stage2::Rendering {
            white_balance,
            color_mode,
            response,
            recovery_mask,
        },
    )?;
    finish!(signals.processed);
    params =
        crate::nodes::sensor::calibration::encode(metadata, crate::color_space::ColorSpace::Srgb)?;
    let physical_extent = [params.source[0], params.source[1]];
    params.source[..2].copy_from_slice(&output_geometry.extent);
    let output_extent = grid.extent([params.output[0], params.output[1]]);
    params.output[..2].copy_from_slice(&output_extent);
    if let Some(size) = settings.processing_output_size() {
        params.output[..2].copy_from_slice(&size);
    }
    let region = settings
        .preview_region
        .filter(|_| {
            !dng && (names == ["final"] || names == ["linear.base"]) && fast_available(metadata)
        })
        .map(|rect| region::Prepared::new(metadata, [params.output[0], params.output[1]], rect))
        .transpose()?;
    let (signals, highlight_mask) = if let Some(region) = &region {
        params.source[..2].copy_from_slice(&region.extent());
        params.output[..2].copy_from_slice(&region.crop.extent());
        region.build(&mut graph, metadata, &signals, highlight_mask)?
    } else {
        (signals, highlight_mask)
    };
    engine.check_extent([params.output[0], params.output[1]])?;
    let residual_level = smooth_crisp_level(controls.detail)?;
    let reconstruction_controls = stage2b::Controls {
        luma: controls.luminance as u8,
        detail: 1.,
        residual_level,
        grid,
    };
    let reconstruction = if settings.is_monochrome() {
        monochrome::prepare_stage2b(metadata, reconstruction_controls, color_mode)?
    } else {
        stage2b::prepare(metadata, reconstruction_controls, color_mode)?
    };
    let reconstruction = if let Some(region) = &region {
        reconstruction.with_extent(region.extent())?
    } else {
        reconstruction
    };
    let mut reconstruction_view = super::camera_view(params);
    if reduced {
        reconstruction_view.crop = output_geometry.bounds(reconstruction_view.crop);
    }
    let reconstructed = stage2b::build(
        &mut graph,
        &signals,
        highlight_mask,
        reconstruction,
        reconstruction_view,
    )?;
    finish!(reconstructed);
    let detail = graph.named_output(reconstructed, "reconstruction.detail")?;
    let spatial_input = if settings.is_monochrome() {
        graph.add_scoped(
            Operation::CodeChannelMap(std::sync::Arc::new(
                crate::nodes::finish::code_channel_map::Prepared::mix(
                    [params.source[0], params.source[1]],
                    if settings.monochrome.mix == crate::render::MonochromeControls::default().mix {
                        [1.; 3]
                    } else {
                        settings.monochrome.mix.map(f64::from)
                    },
                )?,
            )),
            &[reconstructed],
            "mono-mix",
        )?
    } else {
        reconstructed
    };
    let spatial_controls = if settings.is_monochrome() {
        let mut controls = monochrome::stage3_controls(
            usize::from(residual_level),
            usize::from(residual_level),
            settings.effective_sharpen_amount(metadata)?,
        )?;
        controls.grid = grid;
        controls.region = region.as_ref().map(|r| r.physical);
        controls
    } else {
        stage3::Controls {
            contrast_level: usize::from(residual_level),
            sharpen_level: usize::from(residual_level),
            sharpen_base: settings.effective_sharpen_amount(metadata)?,
            lowest_band_factor: None,
            preserve_dark_detail: true,
            grid,
            region: region.as_ref().map(|r| r.physical),
        }
    };
    let sharpened = if !settings.sharpening_enabled
        || settings.photographic.sharpening.method == crate::render::PhotographicMethod::Universal
    {
        stage3::build_contrast(
            &mut graph,
            metadata,
            spatial_input,
            color_mode,
            spatial_controls,
        )?
    } else if settings.is_monochrome() {
        monochrome::build_stage3(
            &mut graph,
            metadata,
            spatial_input,
            detail,
            color_mode,
            spatial_controls,
        )?
    } else {
        stage3::build(
            &mut graph,
            metadata,
            spatial_input,
            detail,
            color_mode,
            spatial_controls,
        )?
    };
    finish!(sharpened);
    // The same sharpening base selects the proportional table and the optional
    // per-channel amount. Residual restoration remains a separate control.
    let amount = stage3::color_sharpen_amount(
        metadata,
        color_mode,
        settings.effective_sharpen_amount(metadata)?,
    )?;
    let amount = amount * grid.factor().sqrt();
    let sharpened = if amount > 0.
        && !settings.is_monochrome()
        && settings.photographic.sharpening.method == crate::render::PhotographicMethod::Reference
    {
        graph.add(
            Operation::ColorSharpen(crate::nodes::finish::color_sharpen::Params::new(
                [params.source[0], params.source[1]],
                (metadata.scalar("CaptureISO")? / 100.) as f32,
                amount,
            )?),
            &[sharpened],
        )?
    } else {
        sharpened
    };
    finish!(sharpened);
    let sharpened = if settings.is_monochrome() {
        graph.add_scoped(
            Operation::CodeChannelMap(std::sync::Arc::new(
                crate::nodes::finish::code_channel_map::Prepared::copy_red_to_blue([
                    params.source[0],
                    params.source[1],
                ])?,
            )),
            &[sharpened],
            "mono-detail-channel",
        )?
    } else {
        sharpened
    };
    let crop = metadata.fixed::<4>("ActiveImageArea")?.map(|v| v as u32);
    // This crop's raw right/bottom are exclusive in the reference operation.
    // Nominal output dimensions still include that final recorded pixel.
    let regional_origin = region.as_ref().map(|r| [r.physical[0], r.physical[1]]);
    let crop = if let Some(region) = region {
        region.crop
    } else {
        let crop = crate::nodes::finish::code_crop::Prepared::new(
            physical_extent,
            crop,
            [params.output[0], params.output[1]],
            metadata.rotation,
            true,
        )?;
        if reduced {
            crop.on_working_grid(grid, output_geometry.extent, output_geometry.origin)?
        } else {
            crop
        }
    };
    let mut rgb_to_native = crop.input_mapping();
    for axis in 0..2 {
        if let Some(origin) = regional_origin {
            rgb_to_native[axis][2] += f64::from(origin[axis]);
        } else if reduced {
            let d = f64::from(grid.divisor());
            rgb_to_native[axis][2] += 0.5 - f64::from(output_geometry.origin[axis]);
            for value in &mut rgb_to_native[axis] {
                *value *= d;
            }
            rgb_to_native[axis][2] -= 0.5;
        }
    }
    let cropped = graph.add(Operation::CodeCrop(std::sync::Arc::new(crop)), &[sharpened])?;
    finish!(cropped);
    validate_photographic(settings)?;
    let response_input = if settings.is_monochrome() {
        graph.add_scoped(
            Operation::CodeChannelMap(std::sync::Arc::new(
                crate::nodes::finish::code_channel_map::Prepared::mix(
                    [params.output[0], params.output[1]],
                    [1., 0., 0.],
                )?,
            )),
            &[cropped],
            "mono-response-channel",
        )?
    } else {
        cropped
    };
    if dng {
        // The reconstruction and Lab conversion operate on linear-light codes.
        // Keep their 16384-code nominal white for the export basis to interpret.
        // The unsigned precision does not imply a photographic transfer curve.
        let final_id = graph.add(Operation::LinearCodeBase, &[response_input])?;
        return Ok(BuiltRecipe {
            graph,
            final_id,
            params,
            rgb_to_native,
            rgb_extent: [params.output[0], params.output[1]],
            rgb_bounds: [0, 0, params.output[0], params.output[1]],
        });
    }
    if super::linear_edits::active(settings)
        || names
            .iter()
            .any(|n| super::linear_edits::requested_output(n))
    {
        let codes = graph.add(Operation::LinearCodeBase, &[response_input])?;
        let space = crate::color_space::ColorSpace::Srgb;
        let id = graph.add(
            Operation::LinearCodeRgb {
                space,
                white: appearance::LINEAR_CODE_WHITE,
            },
            &[codes],
        )?;
        let mut recipe = BuiltRecipe {
            graph,
            final_id: id,
            params,
            rgb_to_native,
            rgb_extent: [params.output[0], params.output[1]],
            rgb_bounds: [0, 0, params.output[0], params.output[1]],
        };
        super::linear_edits::alias(&mut recipe, space, "linear.base")?;
        if requested(&recipe.graph, names) {
            return Ok(recipe);
        }
        let prepare_response = || {
            if settings.is_monochrome() {
                appearance::prepare_monochrome(f64::from(settings.exposure), settings.tone)
            } else {
                appearance::prepare(
                    metadata,
                    color_mode,
                    f64::from(settings.exposure),
                    settings.tone,
                )
            }
        };
        let mut response = None;
        let dehaze_basis = if super::linear_edits::needs_dehaze(settings, names) {
            use crate::nodes::finish::dehaze::Basis;
            Some(if settings.is_monochrome() {
                Basis::monochrome()
            } else {
                Basis::new(
                    space,
                    calibration::color_matrix(metadata, white_balance, color_mode)?,
                    [0.299, 0.587, 0.114],
                )?
            })
        } else {
            None
        };
        let adaptive = if settings.adaptive_tone_active() {
            let response = if settings.is_monochrome() {
                appearance::prepare_monochrome(0., Default::default())?
            } else {
                appearance::prepare(metadata, color_mode, 0., Default::default())?
            };
            let curve =
                super::linear_edits::response_curve(|code| response.evaluate_codes([code; 3])[1]);
            Some(super::linear_edits::Adaptive::new(
                settings,
                &recipe,
                space,
                appearance::LINEAR_CODE_WHITE as f32,
                curve,
                crate::render::ControlCapabilities::from_metadata(metadata)?.thumb_scale(),
            )?)
        } else {
            None
        };
        let context = super::linear_edits::Context {
            dehaze_basis,
            adaptive,

            space,
            white: appearance::LINEAR_CODE_WHITE as f32,
            guide_white: appearance::LINEAR_CODE_WHITE as f32,
            scale: if settings.preview_region.is_some() {
                1.
            } else {
                let native = crate::nodes::sensor::calibration::encode(metadata, space)?;
                (params.output[0] as f32 / native.output[0] as f32)
                    .min(params.output[1] as f32 / native.output[1] as f32)
            },
        };
        let context_white = context.white;
        super::linear_edits::build(
            engine,
            source,
            settings,
            &mut recipe,
            names,
            context,
            |_| {
                let prepared = prepare_response()?;
                let pivot = super::linear_edits::pivot_from_response(context_white, |code| {
                    prepared.evaluate_codes([code; 3])[1]
                })?;
                response = Some(prepared);
                Ok(pivot)
            },
            profile,
            execution,
        )?;
        if requested(&recipe.graph, names) {
            return Ok(recipe);
        }
        let response = match response {
            Some(response) => response,
            None => prepare_response()?,
        };
        recipe.final_id = recipe.graph.add(
            Operation::LinearCodeResponse {
                prepared: std::sync::Arc::new(response),
                space,
            },
            &[recipe.final_id],
        )?;
        if settings.reference_saturation() != 1. && !settings.is_monochrome() {
            recipe.final_id = recipe.graph.add(
                Operation::Saturation {
                    params: super::saturation(
                        [params.output[0], params.output[1], 0, 0],
                        settings.reference_saturation(),
                        settings.prevent_color_clipping,
                    ),
                    space,
                },
                &[recipe.final_id],
            )?;
        }
        return Ok(recipe);
    }
    let response = if settings.is_monochrome() {
        appearance::prepare_monochrome(f64::from(settings.exposure), settings.tone)?
    } else {
        appearance::prepare(
            metadata,
            color_mode,
            f64::from(settings.exposure),
            settings.tone,
        )?
    };
    let mut final_id = graph.add(
        Operation::CodeResponse {
            prepared: std::sync::Arc::new(response),
            space: crate::color_space::ColorSpace::Srgb,
        },
        &[response_input],
    )?;
    if settings.reference_saturation() != 1. && !settings.is_monochrome() {
        final_id = graph.add(
            Operation::Saturation {
                params: super::saturation(
                    [params.output[0], params.output[1], 0, 0],
                    settings.reference_saturation(),
                    settings.prevent_color_clipping,
                ),
                space: crate::color_space::ColorSpace::Srgb,
            },
            &[final_id],
        )?;
    }
    Ok(BuiltRecipe {
        graph,
        final_id,
        params,
        rgb_to_native,
        rgb_extent: [params.output[0], params.output[1]],
        rgb_bounds: [0, 0, params.output[0], params.output[1]],
    })
}
