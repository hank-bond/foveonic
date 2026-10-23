//! Checks source-derived Stage1 noise controls and selected white balance without image processing.

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
    let controls = prepare(&metadata(), 2).unwrap();
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
fn chroma_steps_select_each_reference_strength_pair() {
    for (step, expected) in [[1., 1.5], [0.5, 1.], [0.1, 0.5], [0.05, 0.1], [0.02, 0.06]]
        .into_iter()
        .enumerate()
    {
        let controls = prepare(&metadata(), step as u8).unwrap();
        assert_eq!(controls.bilateral_strengths, expected);
    }
}

#[test]
fn rejects_unproved_routes_and_missing_photographic_inputs() {
    let chroma_step = 2;
    let mut m = metadata();
    m.processing_version = Some([4, 0, 1, 0]);
    assert!(prepare(&m, chroma_step).is_err());
    let mut m = metadata();
    m.properties.remove("SHUTTER");
    assert!(prepare(&m, chroma_step).is_err());
    assert!(prepare(&metadata(), 5).is_err());
    let mut m = metadata();
    m.matrices.get_mut("CaptureShutter").unwrap().values[0] = 1e9;
    let controls = prepare(&m, chroma_step).unwrap();
    assert!(controls.long_exposure);
    assert_eq!(controls.bilateral_strengths, [0.05, 0.007]);
    assert_eq!(controls.minimum_blend, 1.);
}

#[test]
#[ignore = "requires QUATTRO_X3F pointing to a DP2 Quattro original"]
fn prepares_original_dp2_controls_without_captured_request_fields() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").expect("set QUATTRO_X3F")).unwrap();
    let controls = prepare(&source.metadata, 2).unwrap();
    assert_eq!(
        controls.white_balance,
        [1.2925424955483322, 1., 1.2778694712615106]
    );
    assert_eq!(controls.estimated_iso, 100);
    assert_eq!(controls.bilateral_strengths, [0.1, 0.5]);
}

#[test]
fn complete_numerical_preparation_tracks_the_selected_chroma_control() {
    let metadata = metadata();
    let chroma_step = 2;
    let first = prepare_filter(&metadata, chroma_step).unwrap();
    assert_eq!(first.extent(), [2944, 1836]);
    let second = prepare_filter(&metadata, 3).unwrap();
    assert!(first.key() != second.key());
}

#[test]
fn ordinary_dp_models_use_their_own_white_balance_with_shared_noise_controls() {
    let chroma_step = 2;
    for camera in 80..=83 {
        let mut metadata = metadata();
        metadata.matrices.get_mut("CAMERAID").unwrap().values[0] = f64::from(camera);
        metadata.matrices.get_mut("SunlightWBGain").unwrap().values[0] = f64::from(camera) / 40.;
        let controls = prepare(&metadata, chroma_step).unwrap();
        assert_eq!(controls.extent, [2944, 1836]);
        assert_eq!(
            controls.white_balance[0],
            f64::from(camera) / 40. * 1.0379999876022339
        );
        assert_eq!(controls.bilateral_strengths, [0.1, 0.5]);
    }
}

#[test]
fn sd_ambient_map_uses_integer_sample_positions_and_versioned_sources() {
    let mut metadata = metadata();
    metadata.matrices.get_mut("CAMERAID").unwrap().values[0] = 40.;
    metadata.matrices.get_mut("CamsysVersion").unwrap().values = vec![1., 0., 1.];
    metadata.matrices.insert(
        "AESensorValueAmbient256".into(),
        Matrix {
            encoding: MatrixEncoding::Uint8,
            representation: "u8",
            dimensions: vec![16, 16],
            values: (0..256).map(f64::from).collect(),
        },
    );
    // Independent native fixed-map probe uses this ramp to exercise every row.
    assert_eq!(ambient_mean(&metadata).unwrap(), 2495. / 77.);
    metadata.matrices.get_mut("CamsysVersion").unwrap().values = vec![0., 9., 9.];
    assert_eq!(ambient_mean(&metadata).unwrap(), 120.);
    metadata.matrices.get_mut("CamsysVersion").unwrap().values = vec![1., 0., 0.];
    metadata.matrices.insert(
        "AESensorValueAmbient".into(),
        Matrix {
            encoding: MatrixEncoding::Int16,
            representation: "i16",
            dimensions: vec![77],
            values: vec![80.; 77],
        },
    );
    assert_eq!(ambient_mean(&metadata).unwrap(), 80.);
}

#[test]
fn selected_balance_adjusts_gains_without_changing_recorded_balance() {
    let mut metadata = metadata();
    let mut shade = metadata.matrices["SunlightWBGain"].clone();
    shade.values = vec![2., 2., 4.];
    metadata.matrices.insert("ShadeWBGain".into(), shade);
    metadata.lists.insert(
        "WhiteBalanceGains".into(),
        PropertyList::new(vec![
            ("Sunlight".into(), "SunlightWBGain".into()),
            ("Shade".into(), "ShadeWBGain".into()),
        ])
        .unwrap(),
    );
    let recorded = super::super::hlp::recorded_balance(&metadata).unwrap();
    let sunlight = super::super::calibration::white_balance(&metadata, "Sunlight").unwrap();
    let alias = super::super::calibration::white_balance(&metadata, "daylight").unwrap();
    let shade = super::super::calibration::white_balance(&metadata, "shade").unwrap();
    assert_eq!(sunlight, alias);
    assert_eq!(shade, [1.0379999876022339, 1., 1.9316000938415527]);
    assert_ne!(sunlight, shade);
    assert_eq!(
        super::super::hlp::recorded_balance(&metadata).unwrap(),
        recorded
    );
    assert!(super::super::calibration::white_balance(&metadata, "missing-preset").is_err());
}

#[test]
#[ignore = "requires GPU, TIGER_X3F and TIGER_STAGE1_REFERENCE"]
fn original_tiger_stage1_preparation_and_two_pass_boundary_match() {
    let source = crate::source::Source::open(std::env::var_os("TIGER_X3F").unwrap()).unwrap();
    let root = std::path::PathBuf::from(std::env::var_os("TIGER_STAGE1_REFERENCE").unwrap());
    let parameters = std::fs::read(root.join("parameters.bin")).unwrap();
    let float = |offset| f32::from_le_bytes(parameters[offset..offset + 4].try_into().unwrap());
    let double = |offset| f64::from_le_bytes(parameters[offset..offset + 8].try_into().unwrap());
    let mut controls = prepare(&source.metadata, 0).unwrap();
    assert_eq!(
        controls.estimated_iso,
        u32::from_le_bytes(parameters[..4].try_into().unwrap())
    );
    assert_eq!(controls.bilateral_strengths, [float(0x214), float(0x218)]);
    assert_eq!(
        controls.white_balance,
        std::array::from_fn(|i| double(0x240 + i * 8))
    );
    assert_eq!(
        controls.highlight_preservation,
        std::array::from_fn(|i| float(0x258 + i * 4))
    );
    let bytes = std::fs::read(root.join("input.f32")).unwrap();
    controls.extent = [160, 160];
    assert_eq!(bytes.len(), 160 * 160 * 3 * 4);
    let gpu = crate::gpu::Gpu::new().unwrap();
    let input = gpu
        .upload(
            "Native Tiger Stage1 original-data crop",
            bytemuck::cast_slice::<u8, f32>(&bytes),
        )
        .unwrap();
    let output = crate::nodes::color::two_scale_noise::apply(
        &gpu,
        &input,
        &numerical(&controls).unwrap(),
        &mut Default::default(),
    )
    .unwrap();
    let actual = gpu.readback(&output.rgb).unwrap();
    let expected = std::fs::read(root.join("output.native.f32")).unwrap();
    assert_eq!(actual.len(), expected.len());
    let mut maximum = 0_f32;
    for (&a, &b) in bytemuck::cast_slice::<u8, f32>(&actual)[..160 * 158 * 3]
        .iter()
        .zip(bytemuck::cast_slice::<u8, f32>(&expected))
    {
        assert!(a.is_finite() && b.is_finite());
        maximum = maximum.max((a - b).abs());
    }
    eprintln!("Native Tiger two-pass Stage1 max error {maximum}");
    assert_eq!(maximum, 0.);
}
