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
mod region;
mod repair;
pub(crate) mod resources;
mod sd;
mod tiger;

#[cfg(test)]
#[path = "quattro/portability_test.rs"]
mod portability_tests;

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
/// Full source metadata remains authoritative for camera calibration.
/// Working geometry is selected separately for the preview recipe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum WorkingGrid {
    #[default]
    Full,
    Half,
}
impl WorkingGrid {
    fn is_half(self) -> bool {
        self == Self::Half
    }
    fn divisor(self) -> u32 {
        match self {
            Self::Full => 1,
            Self::Half => 2,
        }
    }
    fn extent(self, native: [u32; 2]) -> [u32; 2] {
        native.map(|v| v / self.divisor())
    }
}
pub(in crate::render) fn fast_available(metadata: &crate::source::Metadata) -> bool {
    calibration::is_ordinary_dp(metadata) || calibration::is_interchangeable(metadata)
}

pub(super) fn build(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    names: &[&str],
    profile: &mut crate::profile::Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let mut regional = settings.clone();
    let crop = region::expand_shared_edits(&source.metadata, &mut regional, names)?;
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
        source,
        settings,
        &["final"],
        profile,
        execution,
        true,
    )
}

fn compose(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    names: &[&str],
    profile: &mut crate::profile::Profile,
    execution: &mut Execution<'_>,
    dng: bool,
) -> Result<BuiltRecipe> {
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
    if source.storage() != SourceStorage::PackedRgbBluePhases {
        return Err("sensor plane inspection requires six-sample cells".into());
    }
    let fast = settings.preview_quality.is_reduced()
        && !dng
        && !settings.is_monochrome()
        && (names == ["final"] || names == ["linear.base"])
        && settings.preview_region.is_none()
        && fast_available(&source.metadata);
    let grid = if fast {
        WorkingGrid::Half
    } else {
        WorkingGrid::Full
    };
    let available = outputs(Some(&source.metadata));
    if names.iter().any(|name| {
        !super::linear_edits::requested_output(name)
            && !available.iter().any(|output| output.name == *name)
    }) {
        return Err("requested output is unavailable for this Quattro input".into());
    }
    let [width, height] = source.storage_extent().map(|v| v as u32);
    let mut graph = Graph::default();
    let input = graph.add(Operation::SourcePhaseCells, &[])?;
    // Fast skips general high-blue repairs. A later Full request needs the original
    // cells, even when its reconstruction inputs differ from the Fast graph.
    if names == ["final"] && fast_available(&source.metadata) {
        graph.retain_for_session(input);
    }
    let raw = graph.add(
        Operation::PhaseExtraction(phase_extraction::Params::new(width, height)?),
        &[input],
    )?;
    let mut params = EncodeParams {
        source: [width * 2, height * 2, source.metadata.rotation, 0],
        crop: [0, 0, width * 2, height * 2],
        output: [width * 2, height * 2, 0, 0],
    };
    macro_rules! finish {
        ($output:expr) => {
            execution.check()?;
            if requested(&graph, names) {
                return Ok(BuiltRecipe {
                    graph,
                    final_id: $output,
                    params,
                    rgb_extent: [params.output[0], params.output[1]],
                });
            }
        };
    }
    finish!(raw);

    let calibrated_input = if let Some(prepared) = calibration::column_gain(&source.metadata)? {
        graph.add(
            Operation::ColumnGain(std::sync::Arc::new(prepared)),
            &[input],
        )?
    } else {
        input
    };
    let extracted = graph.add(
        Operation::PhaseExtraction(calibration::extraction(&source.metadata)?),
        &[calibrated_input],
    )?;
    finish!(extracted);

    let is_tiger = tiger::supported(&source.metadata);
    let is_interchangeable = calibration::is_interchangeable(&source.metadata);
    let (sparse, sparse_blue) = if is_interchangeable {
        let rgb = graph.add(
            Operation::Fppd(std::sync::Arc::new(if is_tiger {
                tiger::chroma(&source.metadata)?
            } else {
                sd::chroma(&source.metadata)?
            })),
            &[extracted],
        )?;
        finish!(rgb);
        (rgb, graph.named_output(extracted, "extraction.blue")?)
    } else {
        let blue = graph.named_output(extracted, "extraction.blue")?;
        let gain =
            calibration::phase_gain(&source.metadata, engine.calibration_resources.as_deref())?;
        let corrected_blue = graph.add(Operation::PhaseGain(std::sync::Arc::new(gain)), &[blue])?;
        finish!(corrected_blue);

        let refreshed = graph.add(
            Operation::BlueRefresh(crate::nodes::sensor::blue_refresh::Params::new([
                width, height,
            ])?),
            &[extracted, corrected_blue],
        )?;
        finish!(refreshed);

        let prepared = hlp::prepare(&source.metadata)?;
        let calibrated = graph.add(
            Operation::HlpCalibration(std::sync::Arc::new(prepared)),
            &[refreshed, corrected_blue],
        )?;
        finish!(calibrated);
        let blue = graph.named_output(calibrated, "hlp.blue")?;
        let prepared = repair::prepare(&source.metadata, COLOR_HP_CORRECTION)?;
        let sparse = graph.add(
            Operation::SparseRepair(std::sync::Arc::new(prepared)),
            &[calibrated, blue],
        )?;
        finish!(sparse);
        (sparse, graph.named_output(sparse, "hlp-repair.blue")?)
    };
    // Factory chroma repair receives a separate initially empty mask. Sparse
    // repair writes another surface. Sharing those flags skips valid repairs.
    let mask = graph.add(Operation::EmptyDefectFlags([width, height]), &[])?;
    let prepared = std::sync::Arc::new(defects::prepare(&source.metadata)?);
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
        Operation::RgbDespeckle(std::sync::Arc::new(defects::prepare_despeckle(
            &source.metadata,
        )?)),
        &[general, mask],
    )?;
    finish!(despeckled);
    let chroma = graph.add(
        Operation::ChromaClusters(std::sync::Arc::new(defects::prepare_clusters(
            &source.metadata,
        )?)),
        &[despeckled, mask],
    )?;
    finish!(chroma);
    use crate::nodes::sensor::{
        blue_despeckle::Stage as Filter, ordered_blue_repair::Stage as Repair,
    };
    // Tiger's phase-detection pairs remain visible after a 2x2 mean. Correct
    // those sensor sites before reducing blue, even when general repairs wait
    // for Full. Preserve the reviewed ordinary Quattro Fast path.
    let phase_corrected_blue = if is_interchangeable && (!fast || is_tiger) {
        let corrected = graph.add(
            Operation::Fppd(std::sync::Arc::new(if is_tiger {
                tiger::luma(&source.metadata)?
            } else {
                sd::luma(&source.metadata)?
            })),
            &[sparse_blue],
        )?;
        finish!(corrected);
        corrected
    } else {
        sparse_blue
    };
    let mismatch = if fast {
        phase_corrected_blue
    } else {
        let blue = if is_interchangeable {
            if let Some(prepared) = sd::pattern(&source.metadata)? {
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
            Operation::OrderedBlueRepair(std::sync::Arc::new(luma_repair::prepare_records(
                &source.metadata,
                Repair::Directional,
            )?)),
            &[blue],
        )?;
        finish!(directional);
        let directional_mask = graph.named_output(directional, "luma-repair.defects")?;
        // The selected reference SuperBlue worker has no reachable pixel writes.
        let offsets = graph.add(
            Operation::OrderedBlueRepair(std::sync::Arc::new(luma_repair::prepare_records(
                &source.metadata,
                Repair::DarkOffsets,
            )?)),
            &[directional],
        )?;
        finish!(offsets);
        let dark_filtered = graph.add(
            Operation::BlueDespeckle(std::sync::Arc::new(luma_repair::prepare_despeckle(
                &source.metadata,
                Filter::DarkPairs,
            )?)),
            &[offsets],
        )?;
        finish!(dark_filtered);
        let repaired = graph.add(
            Operation::OrderedBlueRepair(std::sync::Arc::new(luma_repair::prepare_records(
                &source.metadata,
                Repair::General,
            )?)),
            &[dark_filtered, directional_mask],
        )?;
        finish!(repaired);
        let filtered = graph.add(
            Operation::BlueDespeckle(std::sync::Arc::new(luma_repair::prepare_despeckle(
                &source.metadata,
                Filter::General,
            )?)),
            &[repaired],
        )?;
        finish!(filtered);
        let clusters = graph.add(
            Operation::LumaClusters(std::sync::Arc::new(luma_repair::prepare_clusters(
                &source.metadata,
            )?)),
            &[filtered],
        )?;
        finish!(clusters);
        if !is_tiger && (calibration::normalization(&source.metadata)? - 1.).abs() < f32::EPSILON {
            graph.add(
                Operation::BlueMismatch(std::sync::Arc::new(highlights::prepare_blue(
                    &source.metadata,
                )?)),
                &[clusters],
            )?
        } else {
            clusters
        }
    };
    finish!(mismatch);
    // The DP1 optical correction belongs after factory repairs and before
    // highlight restoration. Other DP recipes keep the same RGB history.
    let chroma = if let Some(prepared) = optical::prepare(&source.metadata)? {
        graph.add(
            Operation::PurpleFringe(std::sync::Arc::new(prepared)),
            &[chroma],
        )?
    } else {
        chroma
    };
    finish!(chroma);
    let restored = graph.add(
        Operation::HighlightRestoration(std::sync::Arc::new(highlights::prepare_restoration(
            &source.metadata,
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
    let recorded_color_mode;
    let color_mode = if settings.is_monochrome() || dng {
        "Standard"
    } else if settings.color_mode.is_empty() {
        recorded_color_mode = default_color_mode(&source.metadata)?;
        &recorded_color_mode
    } else {
        &settings.color_mode
    };
    let mut controls = match settings.reconstruction {
        Some(controls) => controls,
        None => captured_noise_controls(&source.metadata)?,
    };
    if settings.is_monochrome() {
        controls.luminance = settings.monochrome.noise;
        controls.detail = settings.monochrome_smoothness.unwrap_or(
            source
                .metadata
                .capture_adjustments()?
                .mono_smoothness
                .unwrap_or(0.5),
        );
    }
    let noise = if settings.is_monochrome() {
        graph.add(
            Operation::ChannelNoise(std::sync::Arc::new(monochrome::prepare_stage1(
                &source.metadata,
            )?)),
            &[restored],
        )?
    } else {
        graph.add(
            Operation::TwoScaleNoise(std::sync::Arc::new(stage1::prepare_filter(
                &source.metadata,
                controls.chroma as u8,
            )?)),
            &[restored],
        )?
    };
    finish!(noise);
    let recorded_white_balance;
    let white_balance = if settings.is_monochrome() {
        "Auto"
    } else if settings.white_balance.is_empty() {
        recorded_white_balance = default_white_balance(&source.metadata)?;
        &recorded_white_balance
    } else {
        &settings.white_balance
    };
    let calibrated = graph.named_output(noise, "noise.source-after")?;
    let highlight_amount = settings.highlight_amount.unwrap_or(
        source
            .metadata
            .capture_adjustments()?
            .highlight_correction
            .unwrap_or(1.),
    );
    let response = if settings.is_monochrome() {
        monochrome::prepare_stage2(&source.metadata, highlight_amount, color_mode)?
    } else {
        stage2::prepare(
            &source.metadata,
            highlight_amount,
            white_balance,
            color_mode,
        )?
    };
    let recovery_mask = if is_tiger {
        Some(graph.named_output(restored, "highlight-restoration.recovery-flags")?)
    } else {
        None
    };
    let signals = stage2::build_signals(
        &mut graph,
        &source.metadata,
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
    let highlight_mask = graph.named_output(restored, "highlight-restoration.mask")?;
    if fast && is_tiger {
        // Smart detail shares these whole-frame color histories with Fast.
        // Retain both masks so a changed WB or recovery control can reuse them too.
        // Admission remains subject to the existing active-photo cache budget.
        for input in [
            signals.filtered,
            signals.calibrated,
            signals.processed,
            highlight_mask,
        ]
        .into_iter()
        .chain(recovery_mask)
        {
            graph.retain_for_session(input);
        }
    }
    params = crate::nodes::sensor::calibration::encode(
        &source.metadata,
        crate::color_space::ColorSpace::Srgb,
    )?;
    let physical_extent = [params.source[0], params.source[1]];
    params.source[..2].copy_from_slice(&grid.extent(physical_extent));
    let output_extent = grid.extent([params.output[0], params.output[1]]);
    params.output[..2].copy_from_slice(&output_extent);
    if let Some(size) = settings.output_size {
        params.output[..2].copy_from_slice(&size);
    }
    let region = settings
        .preview_region
        .filter(|_| {
            !dng && !settings.is_monochrome()
                && (names == ["final"] || names == ["linear.base"])
                && fast_available(&source.metadata)
        })
        .map(|rect| {
            region::Prepared::new(&source.metadata, [params.output[0], params.output[1]], rect)
        })
        .transpose()?;
    let (signals, highlight_mask) = if let Some(region) = &region {
        params.source[..2].copy_from_slice(&region.extent());
        params.output[..2].copy_from_slice(&region.crop.extent());
        region.build(&mut graph, &source.metadata, &signals, highlight_mask)?
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
        monochrome::prepare_stage2b(&source.metadata, reconstruction_controls, color_mode)?
    } else {
        stage2b::prepare(&source.metadata, reconstruction_controls, color_mode)?
    };
    let reconstruction = if let Some(region) = &region {
        reconstruction.with_extent(region.extent())?
    } else {
        reconstruction
    };
    let reconstructed = stage2b::build(&mut graph, &signals, highlight_mask, reconstruction)?;
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
        monochrome::stage3_controls(
            usize::from(residual_level),
            usize::from(residual_level),
            settings.effective_sharpen_amount(&source.metadata)?,
        )?
    } else {
        stage3::Controls {
            contrast_level: usize::from(residual_level),
            sharpen_level: usize::from(residual_level),
            sharpen_base: settings.effective_sharpen_amount(&source.metadata)?,
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
            &source.metadata,
            spatial_input,
            color_mode,
            spatial_controls,
        )?
    } else if settings.is_monochrome() {
        monochrome::build_stage3(
            &mut graph,
            &source.metadata,
            spatial_input,
            detail,
            color_mode,
            spatial_controls,
        )?
    } else {
        stage3::build(
            &mut graph,
            &source.metadata,
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
        &source.metadata,
        color_mode,
        settings.effective_sharpen_amount(&source.metadata)?,
    )?;
    let amount = if fast {
        amount * stage3::HALF_GRID_SHARPEN_SCALE
    } else {
        amount
    };
    let sharpened = if amount > 0.
        && !settings.is_monochrome()
        && settings.photographic.sharpening.method == crate::render::PhotographicMethod::Reference
    {
        graph.add(
            Operation::ColorSharpen(crate::nodes::finish::color_sharpen::Params::new(
                [params.source[0], params.source[1]],
                (source.metadata.scalar("CaptureISO")? / 100.) as f32,
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
    let crop = source
        .metadata
        .fixed::<4>("ActiveImageArea")?
        .map(|v| v as u32);
    // This crop's raw right/bottom are exclusive in the reference operation.
    // Nominal output dimensions still include that final recorded pixel.
    let crop = if let Some(region) = region {
        region.crop
    } else {
        let crop = crate::nodes::finish::code_crop::Prepared::new(
            physical_extent,
            crop,
            [params.output[0], params.output[1]],
            source.metadata.rotation,
            true,
        )?;
        if grid.is_half() {
            crop.on_half_grid()?
        } else {
            crop
        }
    };
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
            rgb_extent: [params.output[0], params.output[1]],
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
            rgb_extent: [params.output[0], params.output[1]],
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
                    &source.metadata,
                    color_mode,
                    f64::from(settings.exposure),
                    settings.tone,
                )
            }
        };
        let mut response = None;
        let context = super::linear_edits::Context {
            space,
            white: appearance::LINEAR_CODE_WHITE as f32,
            guide_white: appearance::LINEAR_CODE_WHITE as f32,
            scale: if settings.preview_region.is_some() {
                1.
            } else {
                let native = crate::nodes::sensor::calibration::encode(&source.metadata, space)?;
                (params.output[0] as f32 / native.output[0] as f32)
                    .min(params.output[1] as f32 / native.output[1] as f32)
            },
        };
        super::linear_edits::build(
            engine,
            source,
            settings,
            &mut recipe,
            names,
            context,
            |_| {
                let prepared = prepare_response()?;
                let pivot = super::linear_edits::pivot_from_response(context.white, |code| {
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
        if settings.saturation != 1. && !settings.is_monochrome() {
            recipe.final_id = recipe.graph.add(
                Operation::Saturation {
                    params: super::saturation(
                        [params.output[0], params.output[1], 0, 0],
                        settings.saturation,
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
            &source.metadata,
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
    if settings.saturation != 1. && !settings.is_monochrome() {
        final_id = graph.add(
            Operation::Saturation {
                params: super::saturation(
                    [params.output[0], params.output[1], 0, 0],
                    settings.saturation,
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
        rgb_extent: [params.output[0], params.output[1]],
    })
}
