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
mod repair;
pub(crate) mod resources;
mod sd;

pub(crate) use inspection::{comparison_input, outputs};
mod stage1;
mod stage2;
mod stage2b;
mod stage3;

use super::{requested, BuiltRecipe};

// The reference color route leaves optional sparse HP correction disabled.
// Photographic highlight reconstruction remains a separate operation.
const COLOR_HP_CORRECTION: bool = false;
use crate::{
    graph::{Graph, Operation},
    interactive::Execution,
    nodes::sensor::{calibration::EncodeParams, phase_extraction},
    render::{Engine, Settings},
    source::{Result, Source, SourceStorage},
};

pub(super) fn build(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    names: &[&str],
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    compose(engine, source, settings, names, execution, false)
}

pub(super) fn build_dng(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    compose(engine, source, settings, &["final"], execution, true)
}

fn compose(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    names: &[&str],
    execution: &mut Execution<'_>,
    dng: bool,
) -> Result<BuiltRecipe> {
    if source.storage() != SourceStorage::PackedRgbBluePhases {
        return Err("sensor plane inspection requires six-sample cells".into());
    }
    let available = outputs(Some(&source.metadata));
    if names
        .iter()
        .any(|name| !available.iter().any(|output| output.name == *name))
    {
        return Err("requested output is unavailable for this Quattro input".into());
    }
    let [width, height] = source.storage_extent().map(|v| v as u32);
    let mut graph = Graph::default();
    let input = graph.add(Operation::SourcePhaseCells, &[])?;
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

    let is_sd = calibration::is_ordinary_sd(&source.metadata);
    let (sparse, sparse_blue) = if is_sd {
        let rgb = graph.add(
            Operation::Fppd(std::sync::Arc::new(sd::chroma(&source.metadata)?)),
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
    let blue = if is_sd {
        let corrected = graph.add(
            Operation::Fppd(std::sync::Arc::new(sd::luma(&source.metadata)?)),
            &[sparse_blue],
        )?;
        finish!(corrected);
        if let Some(prepared) = sd::pattern(&source.metadata)? {
            let corrected = graph.add(
                Operation::BluePattern(std::sync::Arc::new(prepared)),
                &[corrected],
            )?;
            finish!(corrected);
            corrected
        } else {
            corrected
        }
    } else {
        sparse_blue
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
    let mismatch = if (calibration::normalization(&source.metadata)? - 1.).abs() < f32::EPSILON {
        graph.add(
            Operation::BlueMismatch(std::sync::Arc::new(highlights::prepare_blue(
                &source.metadata,
            )?)),
            &[clusters],
        )?
    } else {
        clusters
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
        },
    )?;
    finish!(signals.processed);
    let highlight_mask = graph.named_output(restored, "highlight-restoration.mask")?;
    params = crate::nodes::sensor::calibration::encode(
        &source.metadata,
        crate::color_space::ColorSpace::Srgb,
    )?;
    if let Some(size) = settings.output_size {
        params.output[..2].copy_from_slice(&size);
    }
    engine.check_extent([params.output[0], params.output[1]])?;
    let residual_level = smooth_crisp_level(controls.detail)?;
    let reconstruction_controls = stage2b::Controls {
        luma: controls.luminance as u8,
        detail: 1.,
        residual_level,
    };
    let reconstruction = if settings.is_monochrome() {
        monochrome::prepare_stage2b(&source.metadata, reconstruction_controls, color_mode)?
    } else {
        stage2b::prepare(&source.metadata, reconstruction_controls, color_mode)?
    };
    let reconstructed = stage2b::build(
        &mut graph,
        &source.metadata,
        &signals,
        highlight_mask,
        reconstruction,
    )?;
    finish!(reconstructed);
    let detail = graph.named_output(reconstructed, "reconstruction.detail")?;
    let spatial_input = if settings.is_monochrome() {
        graph.add(
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
        }
    };
    let sharpened = if settings.is_monochrome() {
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
    let sharpened = if amount > 0. && !settings.is_monochrome() {
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
        graph.add(
            Operation::CodeChannelMap(std::sync::Arc::new(
                crate::nodes::finish::code_channel_map::Prepared::copy_red_to_blue([
                    params.source[0],
                    params.source[1],
                ])?,
            )),
            &[sharpened],
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
    let cropped = graph.add(
        Operation::CodeCrop(std::sync::Arc::new(
            crate::nodes::finish::code_crop::Prepared::new(
                [params.source[0], params.source[1]],
                crop,
                [params.output[0], params.output[1]],
                source.metadata.rotation,
                true,
            )?,
        )),
        &[sharpened],
    )?;
    finish!(cropped);
    validate_photographic(settings)?;
    let response_input = if settings.is_monochrome() {
        graph.add(
            Operation::CodeChannelMap(std::sync::Arc::new(
                crate::nodes::finish::code_channel_map::Prepared::mix(
                    [params.output[0], params.output[1]],
                    [1., 0., 0.],
                )?,
            )),
            &[cropped],
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
        });
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
    })
}
