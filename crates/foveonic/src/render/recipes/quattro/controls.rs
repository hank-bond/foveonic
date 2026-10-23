//! Resolves saved Quattro adjustments, preset choices and supported control domains.

use super::appearance;
use crate::{
    render::{Rendering, Settings},
    source::{Metadata, Result},
};

/// The native Smooth/Crisp value selects seven discrete restoration levels.
pub(super) fn smooth_crisp_level(value: f32) -> Result<u8> {
    if !value.is_finite() || !(0. ..=1.).contains(&value) {
        return Err("Smooth/Crisp must be between Smooth (0) and Crisp (1)".into());
    }
    let scaled = 6. * value;
    Ok((f64::from(scaled) + 0.1) as u8)
}

/// Validate the supported photographic and reconstruction controls.
pub(in crate::render) fn validate_photographic(settings: &Settings) -> Result<()> {
    if let Some(controls) = settings.reconstruction {
        smooth_crisp_level(controls.detail)?;
    }
    if !(-2. ..=2.).contains(&settings.exposure) {
        return Err("Quattro exposure must be in -2..2 EV".into());
    }
    if !settings.color_mode.is_empty() && !appearance::MODES.contains(&settings.color_mode.as_str())
    {
        return Err("this Quattro color mode is not implemented".into());
    }
    let mut selected = settings.clone();
    normalize_photographic(&mut selected);
    // Preset availability is resolved separately against the camera catalogue.
    selected.white_balance.clone_from(&settings.white_balance);
    selected.color_mode.clone_from(&settings.color_mode);
    if selected != *settings {
        return Err("unsupported Quattro processing controls; use supported ranges and camera correction defaults".into());
    }
    Ok(())
}

pub(in crate::render) fn normalize_photographic(settings: &mut Settings) -> bool {
    let mut supported = Settings::new(&settings.white_balance, "Standard");
    if settings.color_mode.is_empty() || appearance::MODES.contains(&settings.color_mode.as_str()) {
        supported.color_mode.clone_from(&settings.color_mode);
    }
    supported.preview_quality = settings.preview_quality;
    supported.preview_region = settings.preview_region;
    supported.rendering = settings.rendering;
    supported.monochrome.noise = settings.monochrome.noise;
    supported.monochrome.mix = settings.monochrome.mix;
    supported.monochrome_smoothness = settings.monochrome_smoothness;
    supported.exposure = settings.exposure.clamp(-2., 2.);
    supported.tone = settings.tone;
    supported.saturation = settings.saturation;
    supported.sharpen_amount = settings.sharpen_amount.map(|v| v.clamp(-2., 2.));
    supported.highlight_amount = settings.highlight_amount;
    supported.reconstruction = settings
        .reconstruction
        .map(|controls| crate::noise::Controls {
            chroma: controls.chroma,
            luminance: controls.luminance,
            detail: controls.detail.clamp(0., 1.),
            ..default_noise_controls()
        });
    supported.tiff_color_space = settings.tiff_color_space;
    supported.output_size = settings.output_size;
    if supported == *settings {
        return false;
    }
    *settings = supported;
    true
}

/// Camera order is retained while incomplete mode routes stay unavailable.
pub(in crate::render) fn color_mode_presets(metadata: &Metadata) -> Vec<&str> {
    metadata
        .color_mode_presets()
        .into_iter()
        .filter(|mode| appearance::MODES.contains(mode))
        .collect()
}

/// Missing file adjustments use the reference file-loader defaults.
pub(in crate::render) fn default_noise_controls() -> crate::noise::Controls {
    crate::noise::Controls {
        chroma: 2,
        luminance: 2,
        banding: 2,
        detail: 0.5,
        overrides: Default::default(),
    }
}

/// Saved normalized controls take precedence over the file-loader fallbacks.
pub(in crate::render) fn captured_noise_controls(
    metadata: &Metadata,
) -> Result<crate::noise::Controls> {
    let captured = metadata.capture_adjustments()?;
    let baseline = default_noise_controls();
    let step = |value: Option<f32>, fallback: u32| -> Result<u32> {
        match value {
            Some(value) if (0. ..=1.).contains(&value) => Ok((value * 4.) as u32),
            Some(_) => Err("recorded Quattro noise must be normalized to 0..1".into()),
            None => Ok(fallback),
        }
    };
    let detail = captured.smoothness.unwrap_or(baseline.detail);
    smooth_crisp_level(detail)?;
    Ok(crate::noise::Controls {
        chroma: step(captured.chroma_noise, baseline.chroma)?,
        luminance: step(captured.luminance_noise, baseline.luminance)?,
        banding: step(captured.banding_noise, baseline.banding)?,
        detail,
        overrides: Default::default(),
    })
}

/// Resolve saved presets through this camera's actual metadata catalogue.
pub(in crate::render) fn default_white_balance(metadata: &Metadata) -> Result<String> {
    let Some(code) = metadata.capture_adjustments()?.white_balance else {
        return metadata.recorded_white_balance();
    };
    let aliases: &[&str] = match code {
        1 => &["Auto"],
        2 => &["Sunlight", "Daylight"],
        3 => &["Shade"],
        4 => &["Overcast"],
        5 => &["Incandescent"],
        6 => &["Fluorescent"],
        7 => &["Flash"],
        8 => &["Custom"],
        10 => &["AutoLSP"],
        11 => &["ColorTemp"],
        _ => return Err(format!("unsupported saved Quattro white balance {code}").into()),
    };
    metadata
        .white_balance_presets()
        .into_iter()
        .find(|name| aliases.iter().any(|alias| name.eq_ignore_ascii_case(alias)))
        .map(str::to_owned)
        .ok_or_else(|| "saved Quattro white balance is absent from camera calibration".into())
}

pub(in crate::render) fn default_color_mode(metadata: &Metadata) -> Result<String> {
    let Some(code) = metadata.capture_adjustments()?.color_mode else {
        return metadata.recorded_color_mode();
    };
    let aliases: &[&str] = match code {
        3 => &["Standard"],
        4 => &["Vivid"],
        5 => &["Neutral"],
        6 => &["Portrait"],
        7 => &["Landscape"],
        8 => &["FCBlue", "FOVClassicBlue"],
        9 => &["SunsetRed"],
        10 => &["ForestGreen", "FOVClassicGreen"],
        11 => &["FCYellow", "FOVClassicYellow"],
        12 => &["Cinema"],
        13 => &["TealOrange"],
        _ => return Err(format!("unsupported saved Quattro color mode {code}").into()),
    };
    metadata
        .color_mode_presets()
        .into_iter()
        .find(|name| aliases.iter().any(|alias| name.eq_ignore_ascii_case(alias)))
        .map(str::to_owned)
        .ok_or_else(|| "saved Quattro color mode is absent from camera calibration".into())
}

/// Initialize a new photo before explicit edits are applied.
pub(in crate::render) fn apply_capture_defaults(
    settings: &mut Settings,
    metadata: &Metadata,
) -> Result<()> {
    let captured = metadata.capture_adjustments()?;
    let (exposure, contrast, shadows, highlights) = if settings.is_monochrome() {
        (
            captured.mono_exposure,
            captured.mono_contrast,
            captured.mono_shadows,
            captured.mono_highlights,
        )
    } else {
        (
            captured.exposure,
            captured.contrast,
            captured.shadows,
            captured.highlights,
        )
    };
    settings.exposure = exposure.unwrap_or(0.);
    settings.tone.contrast = f64::from(contrast.unwrap_or(0.));
    settings.tone.shadows = f64::from(shadows.unwrap_or(0.));
    settings.tone.highlights = f64::from(highlights.unwrap_or(0.));
    settings.saturation = 1. + captured.saturation.unwrap_or(0.) * 0.5;
    // Leave source-derived sharpening unresolved so a rendering-mode switch
    // selects the matching color or monochrome header value.
    settings.sharpen_amount = None;
    settings.monochrome.mix = captured
        .mono_mix
        .unwrap_or(crate::render::MonochromeControls::default().mix);
    settings.monochrome.noise =
        (captured.mono_luminance_noise.unwrap_or(0.5).clamp(0., 1.) * 4.) as u32;
    settings.validate_values()
}

/// Header mark interpretation belongs beside the family's saved adjustments.
pub(in crate::render) fn default_rendering(metadata: &Metadata) -> Result<Rendering> {
    let marks = metadata
        .properties
        .get("X3F_MARK_BITS")
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(0);
    Ok(if marks & 2 != 0 {
        Rendering::Monochrome
    } else {
        Rendering::Color
    })
}

/// Resolve the values displayed and restored by the family's editing controls.
pub(in crate::render) fn editing_defaults(
    metadata: &Metadata,
    rendering: Rendering,
) -> Result<Settings> {
    let mut defaults = Settings {
        rendering,
        ..Settings::default()
    };
    apply_capture_defaults(&mut defaults, metadata)?;
    let captured = metadata.capture_adjustments()?;
    defaults.reconstruction = Some(captured_noise_controls(metadata)?);
    defaults.monochrome_smoothness = Some(captured.mono_smoothness.unwrap_or(0.5));
    defaults.sharpen_amount = Some(defaults.effective_sharpen_amount(metadata)?);
    defaults.highlight_amount = Some(captured.highlight_correction.unwrap_or(1.));
    Ok(defaults)
}

/// Lightweight properties expose ordinary panels; full metadata validates the variant.
pub(in crate::render) fn editing_available(
    metadata: Option<&Metadata>,
    camera_model: Option<&str>,
) -> bool {
    if let Some(metadata) = metadata {
        return super::inspection::outputs(Some(metadata))
            .iter()
            .any(|output| output.name == "final");
    }
    camera_model.is_some_and(|model| {
        matches!(
            model.trim().to_ascii_uppercase().as_str(),
            "SIGMA DP0 QUATTRO"
                | "SIGMA DP1 QUATTRO"
                | "SIGMA DP2 QUATTRO"
                | "SIGMA DP3 QUATTRO"
                | "SIGMA SD QUATTRO"
                | "SD QUATTRO"
                | "SIGMA SD QUATTRO H"
                | "SD QUATTRO H"
        )
    })
}

/// Optional channel sharpening follows the saved color or monochrome adjustment.
pub(in crate::render) fn default_sharpen_amount(
    settings: &Settings,
    metadata: &Metadata,
) -> Result<f32> {
    let captured = metadata.capture_adjustments()?;
    Ok(if settings.is_monochrome() {
        captured.mono_sharpness
    } else {
        captured.sharpness
    }
    .unwrap_or(0.))
}
