//! Checks that family control validation and saved-setting normalization agree.

use super::*;

fn capabilities(model: &str) -> ControlCapabilities {
    ControlCapabilities::from_metadata(&Metadata {
        processing_version: None,
        width: 1,
        height: 1,
        rotation: 0,
        properties: [("CAMMODEL".into(), model.into())].into(),
        matrices: Default::default(),
        lists: Default::default(),
    })
    .unwrap()
}

#[test]
fn classic_models_share_supported_controls_and_preserve_effect_selections() {
    for model in [
        "SIGMA SD9",
        "SIGMA SD10",
        "SIGMA SD14",
        "SIGMA SD15",
        "SIGMA DP1",
        "SIGMA DP1S",
        "SIGMA DP1X",
        "SIGMA DP2",
        "SIGMA DP2S",
        "SIGMA DP2X",
    ] {
        let capabilities = capabilities(model);
        assert_eq!(capabilities.family, ProcessingFamily::Classic);
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.rendering = Rendering::Monochrome;
        settings.row_correction = false;
        settings.highlight_restoration = false;
        settings.highlight_threshold_ev = Some(1.);
        settings.color_dq_gain = Some(2.);
        settings.exposure = 0.7;
        assert!(capabilities.validate(&settings).is_err());
        assert!(capabilities.normalize(&mut settings));
        assert!(capabilities.validate(&settings).is_ok());
        assert!(!capabilities.normalize(&mut settings));
        assert!(!settings.highlight_restoration);
        assert!(settings.reconstruction.is_none());
        assert_eq!(settings.exposure, 0.7);
    }
}

#[test]
fn classic_resets_noise_adjustments_without_disabling_reconstruction() {
    let capabilities = capabilities("SIGMA SD14");
    let mut settings = Settings::new("Sunlight", "Standard");
    let noise = crate::noise::Controls {
        chroma: 4,
        ..Default::default()
    };
    settings.reconstruction = Some(noise);
    assert!(capabilities.validate(&settings).is_err());
    assert!(capabilities.normalize(&mut settings));
    assert_eq!(settings.reconstruction, Some(Default::default()));
    assert!(capabilities.validate(&settings).is_ok());
}

#[test]
fn merrill_retains_its_supported_adjustments() {
    let capabilities = capabilities("SIGMA DP2 Merrill");
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.rendering = Rendering::Monochrome;
    settings.row_correction = false;
    settings.highlight_threshold_ev = Some(1.);
    settings.color_dq_gain = Some(2.);
    assert!(capabilities.monochrome && capabilities.noise_adjustments);
    assert!(!capabilities.normalize(&mut settings));
    assert!(capabilities.validate(&settings).is_ok());
}

#[test]
fn every_unsupported_classic_adjustment_is_rejected_and_normalized() {
    let capabilities = capabilities("SIGMA SD14");
    let base = Settings::new("Sunlight", "Standard");
    let mut requests = vec![base.clone(); 7];
    requests[0].row_correction = false;
    requests[1].highlight_threshold_ev = Some(1.);
    requests[2].color_dq_gain = Some(1.);
    requests[3].color_field.selectivity = 0.5;
    requests[4].highlight_mode = HighlightMode::ColorField;
    requests[5].repair = Some(RepairThreshold::Manual(1));
    let noise = crate::noise::Controls {
        chroma: 4,
        ..Default::default()
    };
    requests[6].reconstruction = Some(noise);
    for mut settings in requests {
        assert!(capabilities.validate(&settings).is_err(), "{settings:?}");
        assert!(capabilities.normalize(&mut settings));
        assert!(capabilities.validate(&settings).is_ok());
    }
}

#[test]
fn classic_monochrome_preserves_color_preferences_and_effect_selections() {
    let capabilities = capabilities("SIGMA DP2");
    let mut settings = Settings::new("Shade", "Vivid");
    assert!(capabilities.white_balance_active(&settings));
    assert!(capabilities.highlights_active(&settings));
    settings.rendering = Rendering::Monochrome;
    let before = settings.clone();
    assert!(capabilities.monochrome);
    assert!(!capabilities.white_balance_active(&settings));
    assert!(capabilities.color_mode_active(&settings));
    assert!(!capabilities.highlights_active(&settings));
    assert!(!capabilities.normalize(&mut settings));
    assert!(capabilities.validate(&settings).is_ok());
    assert_eq!(settings, before);
}

#[test]
fn classic_normalizes_saved_merrill_monochrome_adjustments() {
    let capabilities = capabilities("SIGMA DP2");
    for adjustment in 0..3 {
        let mut settings = Settings::new("Shade", "Vivid");
        settings.rendering = Rendering::Monochrome;
        match adjustment {
            0 => settings.monochrome.mix = [0., 0., 1.],
            1 => settings.monochrome.noise = 4,
            _ => settings.monochrome.banding = 0,
        }
        let saved = crate::sidecar::encode(&settings).unwrap();
        let mut restored = crate::sidecar::decode(&saved).unwrap();
        assert!(capabilities.validate(&restored).is_err());
        assert!(capabilities.normalize(&mut restored));
        let mut expected = settings;
        expected.monochrome = Default::default();
        assert_eq!(restored, expected);
        assert!(capabilities.validate(&restored).is_ok());
        assert!(!capabilities.normalize(&mut restored));
    }
}

#[test]
fn quattro_preserves_photographic_edits_and_resets_unsupported_controls() {
    let capabilities = capabilities("SIGMA dp2 Quattro");
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.tiff_color_space = Some(crate::render::TiffColorSpace::ProPhotoRgb);
    settings.output_size = Some([1356, 904]);
    assert!(capabilities.validate(&settings).is_ok());
    settings.white_balance = "Auto".into();
    settings.exposure = 1.;
    settings.tone.contrast = 0.5;
    settings.tone.shadows = -0.5;
    settings.tone.highlights = 0.25;
    settings.saturation = 0.7;
    settings.sharpen_amount = Some(1.);
    assert!(capabilities.white_balance_active(&settings));
    assert!(capabilities.validate(&settings).is_ok());
    settings.reconstruction = Some(crate::noise::Controls {
        chroma: 2,
        luminance: 3,
        detail: 0.5,
        ..capabilities.default_noise_controls()
    });
    assert!(capabilities.validate(&settings).is_ok());
    let supported = settings.clone();
    settings.despeckle = false;
    settings.reconstruction.as_mut().unwrap().banding = 4;
    assert!(capabilities.validate(&settings).is_err());
    assert!(capabilities.normalize(&mut settings));
    assert_eq!(settings, supported);
    settings.despeckle = false;
    assert!(capabilities.validate(&settings).is_err());
    settings = supported;
    settings.exposure = -3.;
    assert!(capabilities.validate(&settings).is_err());
    assert!(capabilities.normalize(&mut settings));
    assert_eq!(settings.exposure, -2.);
    assert!(capabilities.validate(&settings).is_ok());
}

#[test]
fn quattro_color_modes_preserve_camera_order_and_hide_incomplete_routes() {
    use crate::source::{Metadata, PropertyList};
    let metadata = Metadata {
        processing_version: Some([3, 1, 2, 0]),
        width: 5888,
        height: 3672,
        rotation: 0,
        properties: [("CAMMODEL".into(), "SIGMA dp2 Quattro".into())].into(),
        matrices: Default::default(),
        lists: [(
            "ColorModeCompensations".into(),
            PropertyList::new(vec![
                ("Portrait".into(), "portrait".into()),
                ("Neutral".into(), "neutral".into()),
                ("Standard".into(), "standard".into()),
                ("Vivid".into(), "vivid".into()),
            ])
            .unwrap(),
        )]
        .into(),
    };
    let capabilities = super::ControlCapabilities::from_metadata(&metadata).unwrap();
    assert_eq!(
        capabilities.color_mode_presets(&metadata),
        ["Portrait", "Neutral", "Standard", "Vivid"]
    );
    let mut settings = Settings::new("Shade", "Vivid");
    assert!(!capabilities.normalize(&mut settings));
    assert!(capabilities.validate(&settings).is_ok());
    settings.color_mode = "Unknown".into();
    assert!(capabilities.validate(&settings).is_err());
    assert!(capabilities.normalize(&mut settings));
    assert_eq!(settings.color_mode, "Standard");
    assert_eq!(settings.white_balance, "Shade");
}

fn quattro_capture_metadata(saved: &[(&str, &str)]) -> Metadata {
    use crate::source::PropertyList;
    let mut metadata = Metadata {
        processing_version: Some([3, 1, 2, 0]),
        width: 5888,
        height: 3672,
        rotation: 0,
        properties: [
            ("CAMMODEL".into(), "SIGMA dp2 Quattro".into()),
            ("WB_DESC".into(), "Daylight".into()),
            ("CM_DESC".into(), "Standard".into()),
        ]
        .into(),
        matrices: Default::default(),
        lists: [
            (
                "WhiteBalanceGains".into(),
                PropertyList::new(vec![
                    ("Sunlight".into(), "day".into()),
                    ("Auto".into(), "auto".into()),
                    ("Shade".into(), "shade".into()),
                ])
                .unwrap(),
            ),
            (
                "ColorModeCompensations".into(),
                PropertyList::new(vec![
                    ("Standard".into(), "standard".into()),
                    ("Vivid".into(), "vivid".into()),
                ])
                .unwrap(),
            ),
        ]
        .into(),
    };
    for &(key, value) in saved {
        metadata
            .properties
            .insert(format!("X3F_ADJUSTMENT_{key}"), value.into());
    }
    metadata
}

#[test]
fn quattro_saved_presets_override_capture_labels_with_checked_fallbacks() {
    let mut metadata = quattro_capture_metadata(&[]);
    let capabilities = ControlCapabilities::from_metadata(&metadata).unwrap();
    assert_eq!(
        capabilities.default_white_balance(&metadata).unwrap(),
        "Sunlight"
    );
    assert_eq!(
        capabilities.default_color_mode(&metadata).unwrap(),
        "Standard"
    );
    metadata
        .properties
        .insert("X3F_ADJUSTMENT_WHITE_BALANCE".into(), "1".into());
    metadata
        .properties
        .insert("X3F_ADJUSTMENT_COLOR_MODE".into(), "4".into());
    assert_eq!(
        capabilities.default_white_balance(&metadata).unwrap(),
        "Auto"
    );
    assert_eq!(capabilities.default_color_mode(&metadata).unwrap(), "Vivid");
    metadata
        .properties
        .insert("X3F_ADJUSTMENT_WHITE_BALANCE".into(), "7".into());
    assert!(capabilities.default_white_balance(&metadata).is_err());
    metadata
        .properties
        .insert("X3F_ADJUSTMENT_COLOR_MODE".into(), "13".into());
    assert!(capabilities.default_color_mode(&metadata).is_err());
}

#[test]
fn quattro_saved_noise_uses_native_steps_and_preserves_explicit_edits() {
    let metadata = quattro_capture_metadata(&[
        ("CHROMA_NOISE", "0.74"),
        ("LUMINANCE_NOISE", "0.25"),
        ("SMOOTHNESS", "0.6666667"),
    ]);
    let capabilities = ControlCapabilities::from_metadata(&metadata).unwrap();
    let missing = capabilities
        .default_noise_controls_for_metadata(&quattro_capture_metadata(&[]))
        .unwrap();
    assert_eq!(
        (
            missing.chroma,
            missing.luminance,
            missing.banding,
            missing.detail
        ),
        (2, 2, 2, 0.5)
    );
    let selected = capabilities
        .default_noise_controls_for_metadata(&metadata)
        .unwrap();
    // Native noise steps truncate instead of rounding to the nearest quarter.
    assert_eq!(
        (selected.chroma, selected.luminance, selected.banding),
        (2, 1, 2)
    );
    assert_eq!(selected.detail, 0.6666667);
    let mut settings = Settings::new("Shade", "Vivid");
    settings.exposure = 0.75;
    settings.sharpen_amount = Some(-1.);
    settings.highlight_amount = Some(0.25);
    settings.reconstruction = Some(crate::noise::Controls {
        chroma: 4,
        luminance: 0,
        detail: 1.,
        ..missing
    });
    let edits = settings.clone();
    let _ = capabilities.default_white_balance(&metadata).unwrap();
    let _ = capabilities.default_color_mode(&metadata).unwrap();
    let _ = capabilities
        .default_noise_controls_for_metadata(&metadata)
        .unwrap();
    assert!(!capabilities.normalize(&mut settings));
    assert_eq!(settings, edits);
    let invalid = quattro_capture_metadata(&[("CHROMA_NOISE", "1.1")]);
    assert!(capabilities
        .default_noise_controls_for_metadata(&invalid)
        .is_err());
}

#[test]
fn quattro_capture_initializer_maps_signed_photographic_controls() {
    let metadata = quattro_capture_metadata(&[
        ("EXPOSURE", "-0.5"),
        ("CONTRAST", "0.25"),
        ("SHADOWS", "-0.75"),
        ("HIGHLIGHTS", "0.5"),
        ("SATURATION", "-1"),
        ("SHARPNESS", "-1.5"),
        ("MONO_SHARPNESS", "0.75"),
        ("MONO_LUMINANCE_NOISE", "0.75"),
    ]);
    let capabilities = ControlCapabilities::from_metadata(&metadata).unwrap();
    let mut settings = Settings::new("Auto", "Vivid");
    capabilities
        .apply_capture_defaults(&mut settings, &metadata)
        .unwrap();
    assert_eq!(settings.white_balance, "Auto");
    assert_eq!(settings.color_mode, "Vivid");
    assert_eq!(settings.exposure, -0.5);
    assert_eq!(
        (
            settings.tone.contrast,
            settings.tone.shadows,
            settings.tone.highlights
        ),
        (0.25, -0.75, 0.5)
    );
    assert_eq!(settings.saturation, 0.5);
    assert_eq!(settings.sharpen_amount, None);
    assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), -1.5);
    settings.rendering = Rendering::Monochrome;
    assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 0.75);
    settings.rendering = Rendering::Color;
    assert_eq!(settings.monochrome.noise, 3);
    assert!(settings.validate_values().is_ok());
    assert!(capabilities.validate(&settings).is_ok());
}

#[test]
fn quattro_signed_sharpness_and_highlight_domains_are_checked() {
    let capabilities = capabilities("SIGMA dp2 Quattro");
    for amount in [-2., -0.5, 0., 2.] {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.sharpen_amount = Some(amount);
        assert!(settings.validate_values().is_ok());
        assert!(capabilities.validate(&settings).is_ok());
    }
    for amount in [-2.01, 2.01] {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.sharpen_amount = Some(amount);
        assert!(capabilities.validate(&settings).is_err());
    }
    for amount in [0., 0.5, 1.] {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.highlight_amount = Some(amount);
        assert!(settings.validate_values().is_ok());
        assert!(capabilities.validate(&settings).is_ok());
    }
    for amount in [-0.01, 1.01, f32::NAN] {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.highlight_amount = Some(amount);
        assert!(settings.validate_values().is_err());
    }
}

#[test]
fn quattro_color_noise_and_monochrome_smoothing_remain_independent() {
    let metadata = quattro_capture_metadata(&[("SMOOTHNESS", "0.25"), ("MONO_SMOOTHNESS", "0.75")]);
    let capabilities = ControlCapabilities::from_metadata(&metadata).unwrap();
    let mut settings = Settings::new("Auto", "Standard");
    settings.reconstruction = Some(crate::noise::Controls {
        chroma: 4,
        ..capabilities
            .default_noise_controls_for_metadata(&metadata)
            .unwrap()
    });
    assert_eq!(settings.reconstruction.unwrap().detail, 0.25);
    assert_eq!(settings.monochrome_smoothness, None);
    settings.rendering = Rendering::Monochrome;
    settings.monochrome_smoothness = Some(1.);
    assert!(!capabilities.normalize(&mut settings));
    assert!(capabilities.validate(&settings).is_ok());
    assert_eq!(settings.reconstruction.unwrap().detail, 0.25);
    assert_eq!(settings.monochrome_smoothness, Some(1.));
    settings.rendering = Rendering::Color;
    assert!(!capabilities.normalize(&mut settings));
    assert_eq!(settings.reconstruction.unwrap().chroma, 4);
    assert_eq!(settings.reconstruction.unwrap().detail, 0.25);
    assert_eq!(settings.monochrome_smoothness, Some(1.));
}

#[test]
fn quattro_saved_rendering_initializes_matching_tonal_values() {
    let mut metadata = quattro_capture_metadata(&[
        ("EXPOSURE", "0.5"),
        ("CONTRAST", "0.25"),
        ("MONO_EXPOSURE", "-0.5"),
        ("MONO_CONTRAST", "-0.25"),
        ("MONO_SHADOWS", "0.75"),
        ("MONO_HIGHLIGHTS", "-0.75"),
    ]);
    let capabilities = ControlCapabilities::from_metadata(&metadata).unwrap();
    assert_eq!(
        capabilities.default_rendering(&metadata).unwrap(),
        Rendering::Color
    );
    metadata
        .properties
        .insert("X3F_MARK_BITS".into(), "3".into());
    assert_eq!(
        capabilities.default_rendering(&metadata).unwrap(),
        Rendering::Monochrome
    );
    let mut settings = Settings::new("Auto", "Standard");
    settings.rendering = capabilities.default_rendering(&metadata).unwrap();
    capabilities
        .apply_capture_defaults(&mut settings, &metadata)
        .unwrap();
    assert_eq!(settings.exposure, -0.5);
    assert_eq!(
        (
            settings.tone.contrast,
            settings.tone.shadows,
            settings.tone.highlights
        ),
        (-0.25, 0.75, -0.75)
    );
    settings.rendering = Rendering::Color;
    capabilities
        .apply_capture_defaults(&mut settings, &metadata)
        .unwrap();
    assert_eq!(settings.rendering, Rendering::Color);
    assert_eq!(settings.exposure, 0.5);
    assert_eq!(settings.tone.contrast, 0.25);
    metadata
        .properties
        .insert("X3F_MARK_BITS".into(), "invalid".into());
    assert!(capabilities.default_rendering(&metadata).is_err());
}

#[test]
fn quattro_saved_monochrome_mix_initializes_before_explicit_edits() {
    let metadata = quattro_capture_metadata(&[
        ("MONO_MIX_RED", "10813"),
        ("MONO_MIX_GREEN", "10813"),
        ("MONO_MIX_BLUE", "11142"),
    ]);
    let capabilities = ControlCapabilities::from_metadata(&metadata).unwrap();
    let mut settings = Settings::default();
    capabilities
        .apply_capture_defaults(&mut settings, &metadata)
        .unwrap();
    assert_eq!(
        settings.monochrome.mix,
        [10813. / 32768., 10813. / 32768., 11142. / 32768.]
    );
    settings.monochrome.mix = [0., 0., 1.];
    let edited = settings.clone();
    assert!(!capabilities.normalize(&mut settings));
    assert_eq!(settings, edited);
}

#[test]
fn quattro_editing_defaults_resolve_saved_controls_for_each_rendering() {
    let metadata = quattro_capture_metadata(&[
        ("CHROMA_NOISE", "0.75"),
        ("LUMINANCE_NOISE", "0.25"),
        ("SMOOTHNESS", "0.6666667"),
        ("MONO_LUMINANCE_NOISE", "1"),
        ("MONO_SMOOTHNESS", "0.16666667"),
        ("SHARPNESS", "-1"),
        ("MONO_SHARPNESS", "0.8"),
        ("HIGHLIGHT_CORRECTION", "0.4"),
    ]);
    let capabilities = ControlCapabilities::from_metadata(&metadata).unwrap();
    let color = capabilities
        .editing_defaults(&metadata, Rendering::Color)
        .unwrap();
    let mono = capabilities
        .editing_defaults(&metadata, Rendering::Monochrome)
        .unwrap();
    let noise = color.reconstruction.unwrap();
    assert_eq!(
        (noise.chroma, noise.luminance, noise.detail),
        (3, 1, 0.6666667)
    );
    assert_eq!(mono.monochrome.noise, 4);
    assert_eq!(mono.monochrome_smoothness, Some(0.16666667));
    assert_eq!(color.sharpen_amount, Some(-1.));
    assert_eq!(mono.sharpen_amount, Some(0.8));
    assert_eq!(color.highlight_amount, Some(0.4));
    // Resolving UI values does not change initialization's source-following choices.
    let mut fresh = Settings::default();
    capabilities
        .apply_capture_defaults(&mut fresh, &metadata)
        .unwrap();
    assert_eq!(fresh.reconstruction, None);
    assert_eq!(fresh.sharpen_amount, None);
}

#[test]
fn quattro_capture_panel_support_includes_camera_aliases_but_not_tiger() {
    for (model, expected) in [
        ("SIGMA dp2 Quattro", true),
        ("SD QUATTRO", true),
        ("SIGMA sd Quattro H", false),
        ("SD QUATTRO H", false),
    ] {
        let mut metadata = quattro_capture_metadata(&[]);
        metadata.width = 0;
        metadata.height = 0;
        metadata.properties.insert("CAMMODEL".into(), model.into());
        let capabilities = ControlCapabilities::from_metadata(&metadata).unwrap();
        assert_eq!(
            capabilities.editing_available(None, Some(model)),
            expected,
            "{model}"
        );
        // Once full metadata is supplied, the early model hint cannot bypass
        // the recipe's geometry and processing-version requirements.
        assert!(!capabilities.editing_available(Some(&metadata), Some(model)));
    }
}
