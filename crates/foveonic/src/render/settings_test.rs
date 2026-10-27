//! Checks Auto sharpening coefficients, defaults, persistence, and export-only settings.

use super::*;
use crate::source::{Matrix, MatrixEncoding, Metadata, PropertyList};

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
        br#"{"format":"foveonic-editing","version":1,"settings":{"refinement_enabled":false}}"#;
    let bypass = crate::sidecar::decode(bypass).unwrap();
    assert!(!bypass.sharpening_enabled);
    let saved: serde_json::Value =
        serde_json::from_slice(&crate::sidecar::encode(&bypass).unwrap()).unwrap();
    assert_eq!(saved["settings"]["refinement_enabled"], false);
    assert!(saved["settings"].get("sharpening_enabled").is_none());
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
    let effective = settings.processing_settings();
    assert_eq!(effective.exposure, 0.);
    assert_eq!(effective.saturation, 1.);
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
        settings.processing_settings(),
        std::borrow::Cow::Borrowed(_)
    ));
    assert_eq!(settings.exposure, saved.exposure);
    assert_eq!(settings.sharpen_amount, None);
}

#[test]
fn sensitivity_retains_saved_key_and_stays_separate_from_technical_dng_settings() {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.photographic.color_sensitivity = [0.2, 0., -0.3, 0., 0., 0.1];
    settings.photographic.local_contrast = 0.4;
    settings.photographic.tone_method = super::PhotographicMethod::Universal;
    settings.tone.shadows = 0.3;
    settings.tone.highlights = -0.2;
    settings.preview_quality = super::PreviewQuality::Draft;
    settings.preview_region = Some([10, 10, 200, 200]);
    let stored = serde_json::to_value(&settings).unwrap();
    assert_eq!(
        stored["photographic"]["monochrome_sensitivity"],
        serde_json::json!([0.2_f32, 0., -0.3_f32, 0., 0., 0.1_f32])
    );
    assert!(stored["photographic"].get("color_sensitivity").is_none());
    assert_eq!(
        serde_json::from_value::<Settings>(stored)
            .unwrap()
            .photographic,
        settings.photographic
    );
    let dng = settings.dng_settings();
    assert_eq!(dng.photographic.color_sensitivity, [0.; 6]);
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
    settings.photographic.color_sensitivity[2] = 0.8;
    settings.photographic.sharpening.method = PhotographicMethod::Universal;
    settings.photographic.sharpening.amount = 1.2;
    settings.reconstruction = Some(Default::default());
    let authored = serde_json::to_vec(&settings).unwrap();
    settings.processing_sections = ProcessingSections {
        tone: false,
        noise: false,
        detail: false,
        sharpening: false,
        corrections: false,
    };
    let effective = settings.processing_settings();
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
