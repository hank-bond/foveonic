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
fn automatic_sharpening_uses_iso_interpolation_and_luminance_not_detail_or_export_size() {
    let metadata = metadata();
    let mut settings = Settings::new("Sunlight", "Standard");
    assert_eq!(settings.sharpen_amount, Some(0.));
    assert!(!settings.sharpen_selected());
    assert!(settings.tiff_color_space.is_none());
    // Manual and Off do not require camera coefficients.
    assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 0.);
    settings.sharpen_amount = Some(0.75);
    assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 0.75);
    settings.sharpen_amount = None;
    assert!(settings.sharpen_selected());
    assert!(crate::inspection::output_names(&settings, None).contains(&"sharpen.rgb"));
    // ISO 200 lies one third between the two source knots: base 1.75.
    assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 1.75);
    for (choice, multiplier) in [0.5, 0.75, 1., 1.25, 1.5].into_iter().enumerate() {
        settings.reconstruction = Some(crate::noise::Controls {
            luminance: choice as u32,
            chroma: 4 - choice as u32,
            detail: 0.,
            ..Default::default()
        });
        assert_eq!(
            settings.effective_sharpen_amount(&metadata).unwrap(),
            1.75 * multiplier
        );
        settings.reconstruction.as_mut().unwrap().detail = 2.;
        settings.output_size = Some([9, 17]);
        settings.tiff_color_space = Some(TiffColorSpace::ProPhotoRgb);
        assert_eq!(
            settings.effective_sharpen_amount(&metadata).unwrap(),
            1.75 * multiplier
        );
    }
    settings.rendering = Rendering::Monochrome;
    assert_eq!(settings.effective_sharpen_amount(&metadata).unwrap(), 1.75);
    assert!(crate::inspection::output_names(&settings, None).contains(&"sharpen.rgb"));
    assert_eq!(
        crate::inspection::comparison_input(&settings, "sharpen.rgb", None),
        Some("monochrome.rgb")
    );
    let mut zero = metadata.clone();
    let table = zero.matrices.get_mut("ISONoiseSettings").unwrap();
    table.values[12] = -1.;
    table.values[25] = -2.;
    assert_eq!(settings.effective_sharpen_amount(&zero).unwrap(), 0.);
    let mut invalid = metadata;
    invalid.matrices.remove("Histogram");
    assert!(settings.effective_sharpen_amount(&invalid).is_err());
}

#[test]
fn automatic_manual_and_export_choices_round_trip_without_changing_numeric_recipes() {
    for amount in [None, Some(0.), Some(0.75)] {
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
    for invalid in ["-1", "\"auto\"", "[]"] {
        let json = format!(
            r#"{{"format":"foveonic-editing","version":1,"settings":{{"sharpen_amount":{invalid}}}}}"#
        );
        assert!(crate::sidecar::decode(json.as_bytes()).is_err());
    }
    let settings = Settings::default();
    let mut edited = settings.clone();
    edited.sharpen_amount = None;
    assert_eq!(settings.changed_parameters(&edited), ["sharpen"]);
    edited = settings.clone();
    edited.tiff_color_space = Some(TiffColorSpace::ProPhotoRgb);
    assert_eq!(settings.changed_parameters(&edited), ["tiff-color-space"]);
}

#[test]
fn camera_validation_requires_only_active_photographic_choices() {
    let mut metadata = crate::source::Metadata {
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
