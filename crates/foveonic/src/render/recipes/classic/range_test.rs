//! Checks camera-derived range controls independently of GPU arithmetic.

use super::*;
use crate::source::{Matrix, MatrixEncoding};
use std::collections::BTreeMap;

fn metadata(iso: f64) -> Metadata {
    let mut m = Metadata {
        width: 32,
        height: 32,
        rotation: 0,
        properties: BTreeMap::from([("ISO".into(), iso.to_string())]),
        matrices: BTreeMap::new(),
        lists: BTreeMap::new(),
    };
    for (name, values) in [
        ("CaptureISO", vec![iso]),
        ("SensorISO", vec![100.]),
        (
            "ColorNoiseModel",
            vec![1., 2., 3., 4., 5., 6., 0.001, 0.002, 0.003],
        ),
        ("IntensityNoiseModel", vec![8., 0.02, 0.004]),
        ("ColorDQCamRGB", vec![1., 2., 3.]),
    ] {
        m.matrices.insert(
            name.into(),
            Matrix {
                encoding: MatrixEncoding::Float32,
                representation: "synthetic parameter fixture",
                dimensions: vec![values.len()],
                values,
            },
        );
    }
    m
}
fn input(iso: f32) -> ColorInput {
    let ratio = iso / 100.;
    let pre = ratio.min(2.);
    ColorInput {
        monochrome: false,
        camera: [1., 0., 0., 0., 1., 0., 0., 0., 1.],
        final_gain: [2. * f64::from(pre), f64::from(pre), 3. * f64::from(pre)],
        relative_gain: [2., 1., 3.],
        ratio,
        digital_gain: ratio / pre,
        extent: [32, 32],
        statistics_factor: 1.3,
        alternate_matrix: None,
    }
}
fn settings() -> Settings {
    Settings {
        reconstruction: Some(crate::noise::Controls::default()),
        ..Settings::new("Sunlight", "Standard")
    }
}

#[test]
fn neutral_controls_preserve_noise_model_and_diffusion_roles() {
    let m = metadata(400.);
    let c = controls(&m, &input(400.), &settings(), true).unwrap();
    assert_eq!(
        c.diffusion_iterations, 1,
        "signal-dependent diffusion always has its own iteration"
    );
    assert_eq!(
        c.constant_iterations, 0,
        "ISO400 does not default to constant-threshold diffusion"
    );
    assert_eq!(c.downsample, 2);
    assert_eq!(c.window, 11);
    assert!(c.subsample);
    assert!(c.enabled);
    assert_eq!(c.noise_scaling, 0.5, "neutral chroma factor is one");
    assert_eq!(c.residual_strength, 1., "neutral luma factor is one");
    assert_eq!(c.color_noise, [[16., 8., 108.], [16., 10., 36.]]);
    assert_eq!(
        c.intensity_noise,
        [2., 0.005],
        "ISO400 noise-model scale is .25"
    );
    assert_eq!(c.luma_weights, [1. / 12., 1. / 6., 1. / 18.]);
    assert_eq!(c.channel_noise, [5.2, 5.2, 23.400000000000002]);
    let mut off = settings();
    off.reconstruction = None;
    assert!(!controls(&m, &input(400.), &off, true).unwrap().enabled);
}

#[test]
fn iso_and_incandescent_defaults_keep_diffusion_families_distinct() {
    for (iso, downsample, constant, scale) in [
        (200., 2, 0, 1.),
        (400., 2, 0, 0.25),
        (800., 4, 1, 0.75),
        (1600., 4, 1, 1.),
        (3200., 4, 1, 1.),
    ] {
        let c = controls(&metadata(iso), &input(iso as f32), &settings(), true).unwrap();
        assert_eq!(
            (c.diffusion_iterations, c.constant_iterations, c.downsample),
            (1, constant, downsample),
            "ISO {iso}"
        );
        assert_eq!(c.intensity_noise, [8. * scale, 0.02 * scale]);
    }
    let mut incandescent = settings();
    incandescent.white_balance = "Incandescent".into();
    let c = controls(&metadata(400.), &input(400.), &incandescent, true).unwrap();
    assert_eq!(
        (c.diffusion_iterations, c.constant_iterations, c.downsample),
        (1, 1, 4)
    );
    assert_eq!(
        c.color_noise,
        [[4., 8., 12.], [8., 10., 12.]],
        "incandescent model omits WB channel gains"
    );
    assert_eq!(c.intensity_noise, [6., 0.015]);
}

#[test]
fn metadata_overrides_constant_iterations_without_replacing_noise_iteration() {
    let mut m = metadata(400.);
    m.matrices.insert(
        "ResidualAnisoDiffIters2".into(),
        Matrix {
            encoding: MatrixEncoding::Float32,
            representation: "synthetic parameter fixture",
            dimensions: vec![1],
            values: vec![3.],
        },
    );
    let c = controls(&m, &input(400.), &settings(), true).unwrap();
    assert_eq!((c.diffusion_iterations, c.constant_iterations), (1, 3));
}

#[test]
fn luminance_uses_pre_gain_when_output_gain_changes() {
    let lower = input(400.);
    let higher = input(3200.);
    assert_eq!(lower.ratio.min(2.), higher.ratio.min(2.));
    assert_ne!(lower.digital_gain, higher.digital_gain);
    let low = controls(&metadata(400.), &lower, &settings(), true).unwrap();
    let high = controls(&metadata(3200.), &higher, &settings(), true).unwrap();
    // The reference denoise ABI passes pre-gain and output gain separately.
    // Luminance uses pre-gain while the final color matrix uses output gain.
    assert_eq!(high.luma_weights, [1. / 12., 1. / 6., 1. / 18.]);
    assert_eq!(low.luma_weights, high.luma_weights);
    assert_ne!(low.matrix, high.matrix);
}

#[test]
fn dp2_channel_correction_attenuates_above_combined_gain_thresholds() {
    for camera in [
        "SIGMA DP2",
        "SIGMA DP2S",
        "SIGMA DP2X",
        "SIGMA DP1",
        "SIGMA SD15",
    ] {
        for (iso, expected) in [(1000., 1.), (1100., 0.5), (2000., 0.5), (2100., 0.25)] {
            let mut m = metadata(iso);
            m.properties.insert("CAMMODEL".into(), camera.into());
            let c = controls(&m, &input(iso as f32), &settings(), true).unwrap();
            let attenuation = if camera.starts_with("SIGMA DP2") {
                expected
            } else {
                1.
            };
            for channel in 0..3 {
                let baseline = 1.3 * input(iso as f32).final_gain[channel] * (channel + 1) as f64;
                assert_eq!(
                    c.channel_noise[channel],
                    baseline * attenuation,
                    "{camera} ISO{iso}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires CLASSIC_X3F and CLASSIC_MATRIX_REFERENCE original-file captures"]
fn original_mixed_camera_matrix_matches_reference() {
    let source = crate::source::Source::open(std::env::var("CLASSIC_X3F").unwrap()).unwrap();
    let engine = crate::render::Engine::new().unwrap();
    let wb = std::env::var("CLASSIC_WHITE_BALANCE").unwrap_or_else(|_| "Incandescent".into());
    let statistics = super::super::white_balance::estimate(
        &engine,
        &source,
        &wb,
        &mut crate::profile::Profile::default(),
    )
    .unwrap();
    let prepared = prepare::prepare_with_white_balance(&source.metadata, &wb, statistics).unwrap();
    println!(
        "reference fixture gains {:?}, stats {:?}",
        prepared.final_gain, statistics.mean
    );
    let (alternate, mixing) = prepared.color.alternate_matrix.unwrap();
    let actual: [f64; 9] =
        std::array::from_fn(|i| alternate[i] * (1. - mixing) + prepared.color.camera[i] * mixing);
    let bytes = std::fs::read(std::env::var("CLASSIC_MATRIX_REFERENCE").unwrap()).unwrap();
    assert_eq!(bytes.len(), 72);
    for (i, word) in bytes.as_chunks::<8>().0.iter().enumerate() {
        let expected = f64::from_le_bytes(*word);
        assert!(
            (actual[i] - expected).abs() < 1e-12,
            "camera matrix coefficient {i}: {} vs {expected}",
            actual[i]
        );
    }
}
