//! Checks source-derived fixed-WB Stage1 controls without requiring image processing.

use super::*;
use crate::source::{Matrix, PropertyList, Source};

fn metadata() -> Metadata {
    let matrix = |values: Vec<f64>| Matrix {
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
        properties: [
            ("APERTURE".into(), "2.8".into()),
            ("SHUTTER".into(), (1. / 60.).to_string()),
        ]
        .into(),
        matrices: [
            ("CAMERAID".into(), matrix(vec![81.])),
            ("CamsysVersion".into(), matrix(vec![0., 4., 3.])),
            ("CaptureISO".into(), matrix(vec![100.])),
            ("CaptureShutter".into(), matrix(vec![15625.])),
            ("SensorTemperature".into(), matrix(vec![38.])),
            (
                "SunlightWBGain".into(),
                matrix(vec![1.2452239990234375, 1., 1.3231201171875]),
            ),
            (
                "TempGainFact".into(),
                matrix(vec![1.0379999876022339, 1., 0.9658000469207764]),
            ),
        ]
        .into(),
        lists: [(
            "WhiteBalanceGains".into(),
            PropertyList::new(vec![("Sunlight".into(), "SunlightWBGain".into())]).unwrap(),
        )]
        .into(),
    }
}

#[test]
fn explicit_midpoint_selects_two_pass_strengths_and_source_white_balance() {
    let controls = prepare(
        &metadata(),
        NoiseSteps {
            chroma: 2,
            luma: 2,
            residual: 2,
        },
    )
    .unwrap();
    assert_eq!(controls.extent, [2944, 1836]);
    assert_eq!(
        controls.white_balance,
        [1.2925424955483322, 1., 1.2778694712615106]
    );
    assert_eq!(controls.estimated_iso, 100);
    assert_eq!(controls.bilateral_strengths, [0.1, 0.5]);
    assert!(!controls.long_exposure);
    assert_eq!(controls.minimum_blend, 1.);
}

#[test]
fn inactive_descriptor_sliders_do_not_change_selected_helper() {
    for chroma in 0..=4 {
        for residual in 0..=4 {
            let controls = prepare(
                &metadata(),
                NoiseSteps {
                    chroma,
                    luma: 0,
                    residual,
                },
            )
            .unwrap();
            assert_eq!(controls.bilateral_strengths, [1., 1.5]);
        }
    }
}

#[test]
fn rejects_unproved_routes_and_missing_photographic_inputs() {
    let steps = NoiseSteps {
        chroma: 2,
        luma: 2,
        residual: 2,
    };
    let mut m = metadata();
    m.processing_version = Some([4, 0, 1, 0]);
    assert!(prepare(&m, steps).is_err());
    let mut m = metadata();
    m.properties.remove("SHUTTER");
    assert!(prepare(&m, steps).is_err());
    assert!(prepare(&metadata(), NoiseSteps { chroma: 5, ..steps }).is_err());
    let mut m = metadata();
    m.matrices.get_mut("CaptureShutter").unwrap().values[0] = 1e9;
    let controls = prepare(&m, steps).unwrap();
    assert!(controls.long_exposure);
    assert_eq!(controls.bilateral_strengths, [0.05, 0.007]);
    assert_eq!(controls.minimum_blend, 1.);
}

#[test]
#[ignore = "requires QUATTRO_X3F pointing to a DP2 Quattro original"]
fn prepares_original_dp2_controls_without_captured_request_fields() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").expect("set QUATTRO_X3F")).unwrap();
    let controls = prepare(
        &source.metadata,
        NoiseSteps {
            chroma: 2,
            luma: 2,
            residual: 2,
        },
    )
    .unwrap();
    assert_eq!(
        controls.white_balance,
        [1.2925424955483322, 1., 1.2778694712615106]
    );
    assert_eq!(controls.estimated_iso, 100);
    assert_eq!(controls.bilateral_strengths, [0.1, 0.5]);
}
