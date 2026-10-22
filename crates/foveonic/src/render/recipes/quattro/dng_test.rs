//! Checks the approximate camera basis and deferred Quattro response controls.
use super::*;
use crate::source::{Matrix as MetadataMatrix, MatrixEncoding, PropertyList};

fn metadata() -> Metadata {
    let matrix = |values: Vec<f64>| MetadataMatrix {
        encoding: MatrixEncoding::Float32,
        representation: "f64",
        dimensions: vec![values.len()],
        values,
    };
    Metadata {
        width: 5888,
        height: 3672,
        rotation: 0,
        processing_version: Some([3, 1, 2, 0]),
        properties: [("CAMMODEL".into(), "SIGMA dp2 Quattro".into())].into(),
        matrices: [
            ("CaptureISO".into(), matrix(vec![800.])),
            ("daylight".into(), matrix(vec![1.2, 1., 1.4])),
            ("TempGainFact".into(), matrix(vec![1.01, 1., 0.99])),
            (
                "ccm".into(),
                matrix(vec![0.8, 0.15, 0.05, 0.1, 0.8, 0.1, 0.05, 0.15, 0.8]),
            ),
            (
                "standard".into(),
                matrix(vec![1., 0., 0., 0., 1., 0., 0., 0., 1.]),
            ),
            (
                "vivid".into(),
                matrix(vec![
                    1.1, -0.05, -0.05, -0.05, 1.1, -0.05, -0.05, -0.05, 1.1,
                ]),
            ),
            ("contrast".into(), matrix(vec![0.])),
        ]
        .into(),
        lists: [
            (
                "WhiteBalanceGains".into(),
                PropertyList::new(vec![("Sunlight".into(), "daylight".into())]).unwrap(),
            ),
            (
                "WhiteBalanceColorCorrections".into(),
                PropertyList::new(vec![("Sunlight".into(), "ccm".into())]).unwrap(),
            ),
            (
                "ColorModeCompensations".into(),
                PropertyList::new(vec![
                    ("Standard".into(), "standard".into()),
                    ("Vivid".into(), "vivid".into()),
                ])
                .unwrap(),
            ),
            (
                "ColorModeContrastCompensations".into(),
                PropertyList::new(vec![
                    ("Standard".into(), "contrast".into()),
                    ("Vivid".into(), "contrast".into()),
                ])
                .unwrap(),
            ),
        ]
        .into(),
    }
}

#[test]
fn photographic_and_dng_matrix_selection_share_validation_and_aliases() {
    let mut metadata = metadata();
    metadata.matrices.get_mut("CaptureISO").unwrap().values[0] = 100.;
    let expected = [0.8, 0.15, 0.05, 0.1, 0.8, 0.1, 0.05, 0.15, 0.8];
    assert_eq!(
        super::super::calibration::color_matrix(&metadata, "Daylight", "Standard").unwrap(),
        expected
    );
    let settings = Settings::new("Daylight", "Standard");
    assert!(prepare_appearance(&metadata, &settings).is_ok());
    metadata.matrices.get_mut("ccm").unwrap().values[0] = 0.;
    assert!(super::super::calibration::color_matrix(&metadata, "Daylight", "Standard").is_err());
    assert!(prepare_appearance(&metadata, &settings).is_err());
    metadata.matrices.get_mut("ccm").unwrap().values[0] = expected[0];
    for invalid_iso in [0., -100., f64::NAN] {
        metadata.matrices.get_mut("CaptureISO").unwrap().values[0] = invalid_iso;
        assert!(
            super::super::calibration::color_matrix(&metadata, "Daylight", "Standard").is_err()
        );
        assert!(prepare_appearance(&metadata, &settings).is_err());
    }
}

#[test]
fn appearance_edits_preserve_camera_sample_basis() {
    let m = metadata();
    let settings = Settings::new("Sunlight", "Standard");
    let base = prepare_appearance(&m, &settings).unwrap();
    let mut edited = settings.clone();
    edited.color_mode = "Vivid".into();
    edited.exposure = 0.75;
    edited.saturation = 0.5;
    edited.tone.contrast = 0.3;
    let selected = prepare_appearance(&m, &edited).unwrap();
    assert_eq!(base.basis.mapping, selected.basis.mapping);
    assert_eq!(base.basis.neutral, selected.basis.neutral);
    assert_ne!(
        base.basis.selected_from_base,
        selected.basis.selected_from_base
    );
    assert_ne!(base.after_response, selected.after_response);
    let signal = [0.1, 0.2, 0.3];
    let camera = Matrix(base.basis.mapping).apply(signal.map(|v| v * RESPONSE_WHITE_CODE));
    let restored = Matrix(base.basis.camera_to_linear).apply(camera);
    for (a, b) in restored.into_iter().zip(signal) {
        assert!((a - b).abs() < 1e-12);
    }
    assert!(base.basis.neutral.iter().all(|v| *v > 0. && *v <= 1.));
}

#[test]
fn monochrome_basis_requires_no_color_metadata() {
    let mut m = metadata();
    m.matrices.clear();
    m.lists.clear();
    let mut settings = Settings::new("", "");
    settings.rendering = crate::render::Rendering::Monochrome;
    let prepared = prepare_appearance(&m, &settings).unwrap();
    assert_eq!(prepared.basis.neutral, [1.; 3]);
    assert_eq!(
        Matrix(prepared.basis.mapping).apply([RESPONSE_WHITE_CODE; 3]),
        [1.; 3]
    );
}

#[test]
#[ignore = "requires QUATTRO_X3F; prepares camera bases without using the GPU"]
fn original_camera_bases_prepare_for_every_balance() {
    let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    for balance in source.metadata.white_balance_presets() {
        for mode in ["Standard", "Vivid"] {
            let settings = Settings::new(balance, mode);
            let appearance = prepare_appearance(&source.metadata, &settings)
                .unwrap_or_else(|error| panic!("{balance}/{mode}: {error}"));
            crate::dng::appearance::Prepared::sampled(appearance)
                .unwrap_or_else(|error| panic!("{balance}/{mode}: {error}"));
        }
    }
}
