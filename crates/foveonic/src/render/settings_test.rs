//! Checks Auto sharpening coefficients, defaults, persistence, and export-only settings.

use super::*;
use crate::source::{Matrix, MatrixEncoding, Metadata, PropertyList};

#[test]
fn directional_banding_defaults_round_trip_and_retain_inactive_values() {
    let old: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(old.merrill_banding, MerrillBandingControls::default());
    assert!(old.merrill_banding.enabled);
    assert_eq!(
        old.merrill_banding.method,
        crate::render::MerrillBandingMethod::Reference
    );
    assert_eq!(
        old.merrill_banding.rows.bands.map(|b| (b.size, b.strength)),
        [(27, 1.), (119, 0.)]
    );
    assert_eq!(old.merrill_banding.rows, old.merrill_banding.columns);
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.merrill_banding.enabled = false;
    settings.merrill_banding.method = crate::render::MerrillBandingMethod::Directional;
    settings.merrill_banding.columns.bands[1].size = 65;
    settings.merrill_banding.columns.bands[1].strength = 0.7;
    let restored = crate::sidecar::decode(&crate::sidecar::encode(&settings).unwrap()).unwrap();
    assert_eq!(settings, restored);
    assert_eq!(
        old.changed_parameters(&Settings {
            merrill_banding: settings.merrill_banding,
            ..old.clone()
        }),
        ["merrill-banding"]
    );
    assert_eq!(
        serde_json::to_value(settings.merrill_banding).unwrap()["method"],
        "directional"
    );
    assert!(
        serde_json::from_str::<Settings>(r#"{"merrill_banding":{"method":"unknown"}}"#).is_err()
    );
    assert!(serde_json::from_str::<Settings>(r#"{"merrill_banding":{"unknown":1}}"#).is_err());
}

#[test]
fn directional_banding_rejects_bad_sizes_order_and_strength_even_when_disabled() {
    let mut settings = Settings::new("Sunlight", "Standard");
    for size in [0, 1, 2, 4, 120, 121] {
        settings.merrill_banding = Default::default();
        settings.merrill_banding.enabled = false;
        settings.merrill_banding.rows.bands[0].size = size;
        assert!(settings.validate_values().is_err(), "size {size}");
    }
    for strength in [-0.01, 3.01, f32::NAN, f32::INFINITY] {
        settings.merrill_banding = Default::default();
        settings.merrill_banding.enabled = false;
        settings.merrill_banding.columns.bands[1].strength = strength;
        assert!(settings.validate_values().is_err(), "strength {strength}");
    }
    settings.merrill_banding = Default::default();
    settings.merrill_banding.rows.bands[0].size = 119;
    assert!(settings.validate_values().is_err());
    settings.merrill_banding.rows.bands[0].size = 3;
    settings.merrill_banding.rows.bands[1].size = 5;
    assert!(settings.validate_values().is_ok());
    assert_eq!(settings.merrill_banding.rows.size_range(0), 3..=3);
    assert_eq!(settings.merrill_banding.rows.size_range(1), 5..=119);
    settings.merrill_banding.columns.bands[0].strength = 3.;
    assert!(settings.validate_values().is_ok());
}

#[test]
fn merrill_noise_method_defaults_and_round_trips_without_changing_inactive_controls() {
    assert_eq!(
        serde_json::from_str::<Settings>("{}")
            .unwrap()
            .merrill_noise_method,
        MerrillNoiseMethod::Reference
    );
    for (method, stored) in [
        (MerrillNoiseMethod::Reference, "reference"),
        (MerrillNoiseMethod::MultiscaleRows, "multiscale_rows"),
        (
            MerrillNoiseMethod::MultiscaleRowsAndColumns,
            "multiscale_rows_and_columns",
        ),
    ] {
        let settings = Settings {
            merrill_noise_method: method,
            ..Default::default()
        };
        let encoded = serde_json::to_value(&settings).unwrap();
        assert_eq!(encoded["merrill_noise_method"], stored);
        let restored: Settings = serde_json::from_value(encoded).unwrap();
        assert_eq!(restored, settings);
        assert!(restored.reconstruction.is_none());
    }
    assert!(
        serde_json::from_str::<Settings>(r#"{"merrill_noise_method":"unrecognized"}"#).is_err()
    );
}

#[test]
fn merrill_noise_method_changes_have_a_distinct_interactive_identity() {
    let settings = Settings::default();
    let mut edited = settings.clone();
    edited.merrill_noise_method = MerrillNoiseMethod::MultiscaleRows;
    assert_eq!(
        settings.changed_parameters(&edited),
        ["merrill-noise-method"]
    );
    edited.merrill_noise_method = MerrillNoiseMethod::MultiscaleRowsAndColumns;
    assert_eq!(
        settings.changed_parameters(&edited),
        ["merrill-noise-method"]
    );
}

#[test]
fn monochrome_native_methods_follow_mono_noise_and_independent_banding_policy() {
    let mut settings = Settings {
        rendering: Rendering::Monochrome,
        merrill_noise_method: MerrillNoiseMethod::MultiscaleRows,
        ..Settings::default()
    };
    assert!(settings.reconstruction.is_none());
    assert!(settings.multiscale_noise_selected());
    assert!(settings.reference_banding_selected());
    assert_eq!(settings.reference_rows_selected(), settings.row_correction);
    settings.monochrome.noise = 0;
    assert!(
        settings.multiscale_noise_selected(),
        "the reference amount must not disable the native method"
    );
    settings.merrill_banding.method = crate::render::MerrillBandingMethod::Directional;
    settings.monochrome.banding = 4;
    assert!(settings.directional_banding_selected());
    assert!(!settings.reference_banding_selected());
    assert!(!settings.reference_rows_selected());
    assert_eq!(
        settings.sensor_noise_controls().banding,
        crate::noise::Controls::default().banding
    );
    settings.merrill_noise_method = MerrillNoiseMethod::Reference;
    assert!(settings.directional_banding_selected());
    assert!(!settings.multiscale_noise_selected());
    settings.merrill_banding.enabled = false;
    assert!(!settings.directional_banding_selected());
    settings.merrill_banding.enabled = true;
    settings.merrill_noise_method = MerrillNoiseMethod::MultiscaleRows;
    settings.processing_sections.noise = false;
    assert!(!settings.multiscale_noise_selected());
    assert!(!settings.directional_banding_selected());
    assert!(!settings.reference_rows_selected());
    settings.processing_sections.noise = true;
    settings.merrill_banding.method = crate::render::MerrillBandingMethod::Reference;
    assert_eq!(settings.sensor_noise_controls().banding, 4);
}

#[test]
fn saturation_is_independent_of_tone_method_and_section_bypass() {
    let mut settings = Settings::default();
    settings.photographic.saturation_method = PhotographicMethod::Universal;
    settings.photographic.perceptual.saturation = 0.3;
    settings.saturation = 1.8;
    settings.tone.shadows = 0.4;
    let authored = settings.clone();
    assert!(settings.universal_saturation_active());
    assert_eq!(settings.reference_saturation(), 1.);
    assert_eq!(settings.active_tone_settings().tone.shadows, 0.4);
    settings.processing_sections.tone = false;
    let active = settings.processing_settings(crate::source::ProcessingFamily::Classic);
    assert!(active.universal_saturation_active());
    assert_eq!(active.tone, Default::default());
    drop(active);
    settings.processing_sections.tone = true;
    settings.processing_sections.saturation = false;
    let active = settings.processing_settings(crate::source::ProcessingFamily::Classic);
    assert!(!active.universal_saturation_active());
    assert_eq!(active.tone.shadows, 0.4);
    drop(active);
    settings.processing_sections.saturation = true;
    settings.photographic.saturation_method = PhotographicMethod::Reference;
    assert_eq!(settings.reference_saturation(), 1.8);
    assert_eq!(
        settings.photographic.perceptual,
        authored.photographic.perceptual
    );
    let decoded: Settings =
        serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
    assert_eq!(decoded, settings);
    let old: Settings = serde_json::from_str(
        r#"{"photographic":{"perceptual":{"enabled":true,"saturation":0.4}}}"#,
    )
    .unwrap();
    assert_eq!(
        old.photographic.saturation_method,
        PhotographicMethod::default()
    );
}

fn metadata() -> Metadata {
    let matrix = |dimensions, values| Matrix {
        encoding: MatrixEncoding::Float32,
        representation: "f64",
        dimensions,
        values,
    };
    let mut rows = [[1_f64; 13]; 2];
    rows[0][0] = 100.;
    rows[0][12] = 1.25;
    rows[1][0] = 400.;
    rows[1][12] = 2.75;
    let sliders: [[f64; 13]; 5] = std::array::from_fn(|r| {
        let mut row = [1.; 13];
        row[12] = [0.5, 0.75, 1., 1.25, 1.5][r];
        row
    });
    Metadata {
        processing_version: None,
        width: 1,
        height: 1,
        rotation: 0,
        properties: [("ISO", "200"), ("APERTURE", "8"), ("SHUTTER", "0.01")]
            .map(|(k, v)| (k.into(), v.into()))
            .into(),
        matrices: [
            ("WB", matrix(vec![3], vec![1.; 3])),
            ("ISONoiseSettings", matrix(vec![2, 13], rows.concat())),
            (
                "DenoisingSlidersStep",
                matrix(vec![5, 13], sliders.concat()),
            ),
            ("HistPercent", matrix(vec![1], vec![0.])),
            ("HistBitDepth", matrix(vec![1], vec![8.])),
            (
                "EstimatedISOreference",
                matrix(vec![1, 3], vec![0., 0., 1.]),
            ),
            (
                "Histogram",
                Matrix {
                    encoding: MatrixEncoding::Uint8,
                    representation: "u32",
                    dimensions: vec![3, 1],
                    values: vec![0.; 3],
                },
            ),
        ]
        .map(|(k, v)| (k.into(), v))
        .into(),
        lists: [(
            "WhiteBalanceGains".into(),
            PropertyList::new(vec![("Sunlight".into(), "WB".into())]).unwrap(),
        )]
        .into(),
    }
}

#[test]
fn classic_auto_sharpening_matches_recipe_default_without_merrill_descriptors() {
    let mut metadata = metadata();
    metadata
        .properties
        .insert("CAMMODEL".into(), "SIGMA SD14".into());
    metadata.matrices.clear();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.sharpen_amount = None;
    assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 1.);
    settings.reconstruction = None;
    assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 1.);
    for amount in [0., 0.75, 2.] {
        settings.sharpen_amount = Some(amount);
        assert_eq!(
            settings.effective_sharpen_amount(&metadata).unwrap(),
            amount
        );
    }
}

#[test]
fn merrill_color_auto_sharpening_is_zero_and_monochrome_keeps_camera_coefficients() {
    let mut metadata = metadata();
    metadata
        .properties
        .insert("CAMMODEL".into(), "SIGMA DP2 Merrill".into());
    let mut settings = Settings::new("Sunlight", "Standard");
    assert_eq!(settings.sharpen_amount, None);
    for choice in 0..=4 {
        settings.reconstruction = Some(crate::noise::Controls {
            chroma: choice,
            luminance: 4 - choice,
            texture: crate::noise::TextureControls {
                fine: choice as f32 / 2.,
                coarse_size: 2.,
                ..Default::default()
            },
            ..Default::default()
        });
        assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 0.);
        settings.output_size = Some([9, 17]);
        settings.tiff_color_space = Some(TiffColorSpace::ProPhotoRgb);
        assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 0.);
    }
    settings.reconstruction = None;
    assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 0.);
    settings.sharpen_amount = Some(0.75);
    assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 0.75);
    settings.sharpen_amount = None;
    settings.rendering = Rendering::Monochrome;
    // ISO 200 lies one third between the source knots, giving the mono base 1.75.
    assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 1.75);
    assert_eq!(
        crate::inspection::comparison_input(&settings, "sharpen.rgb", None),
        Some("monochrome.rgb")
    );
    let mut zero = metadata.clone();
    let table = zero.matrices.get_mut("ISONoiseSettings").unwrap();
    table.values[12] = -1.;
    table.values[25] = -2.;
    assert_eq!(settings.effective_sharpen_amount(&zero).unwrap(), 0.);
    metadata.matrices.remove("Histogram");
    assert!(settings.effective_sharpen_amount(&metadata).is_err());
}

#[test]
fn automatic_manual_and_export_choices_round_trip_without_changing_numeric_recipes() {
    for amount in [None, Some(-1.), Some(0.), Some(0.75)] {
        for destination in [
            None,
            Some(TiffColorSpace::Srgb),
            Some(TiffColorSpace::AdobeRgb),
            Some(TiffColorSpace::ProPhotoRgb),
        ] {
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.sharpen_amount = amount;
            settings.tiff_color_space = destination;
            let bytes = crate::sidecar::encode(&settings).unwrap();
            assert_eq!(crate::sidecar::decode(&bytes).unwrap(), settings);
        }
    }
    let numeric =
        br#"{"format":"foveonic-editing","version":1,"settings":{"sharpen_amount":0.75}}"#;
    assert_eq!(
        crate::sidecar::decode(numeric).unwrap().sharpen_amount,
        Some(0.75)
    );
    let explicit_auto =
        br#"{"format":"foveonic-editing","version":1,"settings":{"sharpen_amount":null}}"#;
    assert_eq!(
        crate::sidecar::decode(explicit_auto)
            .unwrap()
            .sharpen_amount,
        None
    );
    for invalid in ["\"auto\"", "[]"] {
        let json = format!(
            r#"{{"format":"foveonic-editing","version":1,"settings":{{"sharpen_amount":{invalid}}}}}"#
        );
        assert!(crate::sidecar::decode(json.as_bytes()).is_err());
    }
    let unspecified = br#"{"format":"foveonic-editing","version":1,"settings":{}}"#;
    assert_eq!(
        crate::sidecar::decode(unspecified).unwrap().sharpen_amount,
        None
    );
    let manual_off =
        br#"{"format":"foveonic-editing","version":1,"settings":{"sharpen_amount":0}}"#;
    assert_eq!(
        crate::sidecar::decode(manual_off).unwrap().sharpen_amount,
        Some(0.)
    );
    let bypass =
        br#"{"format":"foveonic-editing","version":1,"settings":{"sharpening_enabled":false}}"#;
    let bypass = crate::sidecar::decode(bypass).unwrap();
    assert!(!bypass.sharpening_enabled);
    let saved: serde_json::Value =
        serde_json::from_slice(&crate::sidecar::encode(&bypass).unwrap()).unwrap();
    assert_eq!(saved["settings"]["sharpening_enabled"], false);
    assert!(saved["settings"].get("refinement_enabled").is_none());
    let settings = Settings::default();
    let mut edited = settings.clone();
    edited.sharpen_amount = Some(0.);
    assert_eq!(settings.changed_parameters(&edited), ["sharpen"]);
    edited = settings.clone();
    edited.tiff_color_space = Some(TiffColorSpace::ProPhotoRgb);
    assert_eq!(settings.changed_parameters(&edited), ["tiff-color-space"]);
}

#[test]
fn camera_validation_requires_only_active_photographic_choices() {
    let mut metadata = crate::source::Metadata {
        processing_version: None,
        width: 1,
        height: 1,
        rotation: 0,
        properties: [("CAMMODEL".into(), "SIGMA DP2".into())].into(),
        matrices: Default::default(),
        lists: Default::default(),
    };
    let mut settings = Settings::new("", "Standard");
    settings.rendering = Rendering::Monochrome;
    assert!(
        settings.validate_for_camera(&metadata).is_ok(),
        "Classic mono WB is inactive"
    );
    settings.color_mode.clear();
    assert!(
        settings.validate_for_camera(&metadata).is_err(),
        "Classic mono color mode is active"
    );
    metadata
        .properties
        .insert("CAMMODEL".into(), "SIGMA DP2 Merrill".into());
    assert!(
        settings.validate_for_camera(&metadata).is_err(),
        "Merrill mono needs WB"
    );
    settings.white_balance = "Sunlight".into();
    assert!(
        settings.validate_for_camera(&metadata).is_ok(),
        "Merrill mono color mode is inactive"
    );
}

#[test]
fn preview_quality_is_transient_and_invalidates_active_rendering() {
    let full = Settings::new("Sunlight", "Standard");
    let mut fast = full.clone();
    fast.preview_quality = PreviewQuality::Fast;
    assert_eq!(full.changed_parameters(&fast), ["preview-quality"]);
    assert_eq!(
        crate::sidecar::encode(&full).unwrap(),
        crate::sidecar::encode(&fast).unwrap()
    );
    assert_eq!(
        crate::sidecar::decode(&crate::sidecar::encode(&fast).unwrap())
            .unwrap()
            .preview_quality,
        PreviewQuality::Full
    );
}

#[test]
fn preview_region_is_transient_and_invalidates_only_regional_work() {
    let full = Settings::new("Sunlight", "Standard");
    let mut region = full.clone();
    region.preview_region = Some([100, 200, 500, 600]);
    assert_eq!(full.changed_parameters(&region), ["preview-region"]);
    assert_eq!(
        crate::sidecar::encode(&full).unwrap(),
        crate::sidecar::encode(&region).unwrap()
    );
}
#[test]
fn merrill_group_switches_keep_colordq_independent_of_detail() {
    let mut settings = super::Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    assert!(settings.color_dq_selected());
    settings.reconstruction.as_mut().unwrap().detail_enabled = false;
    assert!(settings.color_dq_selected());
    assert!(settings.merrill_reconstruction().is_some());
    settings.reconstruction.as_mut().unwrap().noise_enabled = false;
    assert!(!settings.color_dq_selected());
    assert!(settings.merrill_reconstruction().is_none());
    settings.reconstruction.as_mut().unwrap().detail_enabled = true;
    assert!(!settings.color_dq_selected());
    assert!(settings.merrill_reconstruction().is_none());
    settings.reconstruction.as_mut().unwrap().texture.fine = 0.8;
    assert!(settings.merrill_reconstruction().is_some());
    settings.color_dq_gain = Some(0.7);
    assert!(settings.color_dq_selected());
}

#[test]
fn section_bypasses_resolve_to_neutral_and_preserve_stored_edits() {
    let mut settings = Settings::new("Shade", "Vivid");
    settings.exposure = 0.7;
    settings.saturation = 1.3;
    settings.tone.contrast = 0.4;
    settings.tonal_ranges.shadows = 0.5;
    settings.sharpen_amount = None;
    settings.tone_enabled = false;
    settings.sharpening_enabled = false;
    let saved = settings.clone();
    let effective = settings.processing_settings(crate::source::ProcessingFamily::Classic);
    assert_eq!(effective.exposure, 0.);
    assert_eq!(effective.saturation, 1.3);
    assert_eq!(effective.tone, ToneControls::default());
    assert!(effective.tonal_ranges.is_neutral());
    assert_eq!(effective.sharpen_amount, Some(0.));
    assert_eq!(effective.color_mode, "Vivid");
    assert_eq!(settings, saved);
    assert!(!settings.sharpen_selected());
    assert_eq!(
        crate::sidecar::decode(&crate::sidecar::encode(&settings).unwrap()).unwrap(),
        saved
    );
    settings.tone_enabled = true;
    settings.sharpening_enabled = true;
    assert!(matches!(
        settings.processing_settings(crate::source::ProcessingFamily::Classic),
        std::borrow::Cow::Borrowed(_)
    ));
    assert_eq!(settings.exposure, saved.exposure);
    assert_eq!(settings.sharpen_amount, None);
}

#[test]
fn color_editor_roundtrips_and_stays_separate_from_technical_dng_settings() {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.photographic.color_editor = crate::render::ColorEditorControls::from_lightness([
        0.2,
        (0.2 + 0.) * 0.5,
        0.,
        -0.3,
        0.,
        0.,
        (0. + 0.1) * 0.5,
        0.1,
    ]);
    settings.photographic.local_contrast = 0.4;
    settings.photographic.tone_method = super::PhotographicMethod::Universal;
    settings.tone.shadows = 0.3;
    settings.tone.highlights = -0.2;
    settings.preview_quality = super::PreviewQuality::Draft;
    settings.preview_region = Some([10, 10, 200, 200]);
    let stored = serde_json::to_value(&settings).unwrap();
    assert!(stored["photographic"]
        .get("monochrome_sensitivity")
        .is_none());
    assert_eq!(
        serde_json::from_value::<Settings>(stored)
            .unwrap()
            .photographic,
        settings.photographic
    );
    let dng = settings.dng_settings();
    assert_eq!(dng.photographic.color_editor, Default::default());
    assert_eq!(dng.photographic.local_contrast, 0.);
    assert_eq!(dng.tone.shadows, 0.);
    assert_eq!(dng.tone.highlights, 0.);
    assert_eq!(settings.tone.shadows, 0.3);
    assert_eq!(settings.tone.highlights, -0.2);
    assert_eq!(dng.preview_quality, super::PreviewQuality::Full);
    assert!(dng.preview_region.is_none());
}

#[test]
fn batch_section_exclusions_leave_authored_sidecars_and_values_unchanged() {
    let mut settings = Settings::new("Sunlight", "Vivid");
    settings.exposure = 0.5;
    settings.tonal_ranges.shadows = 0.4;
    settings.photographic.local_contrast = 0.6;
    settings.photographic.color_editor.bands[3].lightness = 0.8;
    settings.photographic.sharpening.method = PhotographicMethod::Universal;
    settings.photographic.sharpening.amount = 1.2;
    settings.reconstruction = Some(Default::default());
    let authored = serde_json::to_vec(&settings).unwrap();
    settings.processing_sections = ProcessingSections {
        tone: false,
        saturation: false,
        color_editor: false,
        noise: false,
        detail: false,
        sharpening: false,
        corrections: false,
    };
    let effective = settings.processing_settings(crate::source::ProcessingFamily::Classic);
    assert!(!effective.tone_enabled && !effective.sharpening_enabled);
    assert_eq!(effective.exposure, 0.);
    assert!(effective.tonal_ranges.is_neutral());
    assert_eq!(effective.photographic.local_contrast, 0.);
    assert_eq!(effective.photographic.sharpening.amount, 0.);
    assert!(!effective.reconstruction.unwrap().noise_enabled);
    assert!(!effective.reconstruction.unwrap().detail_enabled);
    assert!(!effective.monochrome.detail_enabled);
    assert!(effective.repair.is_none());
    assert!(!effective.despeckle && !effective.highlight_restoration);
    assert_eq!(serde_json::to_vec(&settings).unwrap(), authored);
    assert_eq!(settings.exposure, 0.5);
    assert_eq!(settings.photographic.sharpening.amount, 1.2);
}

#[test]
fn color_editor_bypass_is_independent_and_retains_authored_values() {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.photographic.color_editor.bands[0].hue = 0.5;
    settings.tone_enabled = false;
    let effective = settings.processing_settings(crate::source::ProcessingFamily::Classic);
    assert!(effective.color_editor_enabled);
    assert_eq!(
        effective.photographic.color_editor,
        settings.photographic.color_editor
    );
    settings.processing_sections.color_editor = false;
    let effective = settings.processing_settings(crate::source::ProcessingFamily::Classic);
    assert!(!effective.color_editor_enabled);
    assert_eq!(
        effective.photographic.color_editor,
        settings.photographic.color_editor
    );
    settings.processing_sections = Default::default();
    settings.tone_enabled = true;
    settings.color_editor_enabled = false;
    let stored = serde_json::to_value(&settings).unwrap();
    let restored: Settings = serde_json::from_value(stored).unwrap();
    assert!(!restored.color_editor_enabled && restored.tone_enabled);
    assert_eq!(
        restored.photographic.color_editor,
        settings.photographic.color_editor
    );
    assert!(
        serde_json::from_str::<Settings>("{}")
            .unwrap()
            .color_editor_enabled
    );
}

#[test]
fn merrill_speckles_follow_noise_scope_and_validate_controls() {
    use crate::source::ProcessingFamily;
    let mut s = Settings::default();
    let original = s.clone();
    s.processing_sections.corrections = false;
    assert!(s.processing_settings(ProcessingFamily::Merrill).despeckle);
    assert!(!s.processing_settings(ProcessingFamily::Classic).despeckle);
    s.processing_sections.noise = false;
    assert!(!s.processing_settings(ProcessingFamily::Merrill).despeckle);
    assert!(s.despeckle);
    for threshold in [0., f32::NAN, 5.] {
        s.merrill_speckles.threshold = threshold;
        assert!(s.validate_values().is_err());
    }
    s = original;
    for size in [0, 2, 4, 9] {
        s.merrill_speckles.size = size;
        assert!(s.validate_values().is_err());
    }
}
