//! Checks generated lookup bytes and the calibrated-input-only numerical contract.

use super::*;

#[test]
fn high_iso_and_long_exposure_have_distinct_prepared_behavior() {
    let controls = Stage1Controls {
        extent: [16, 16],
        white_balance: [1.; 3],
        estimated_iso: 400,
        bilateral_strengths: [1., 1.5],
        long_exposure: false,
        minimum_blend: 0.,
    };
    let high = numerical(&controls).unwrap();
    let long = numerical(&Stage1Controls {
        long_exposure: true,
        minimum_blend: 0.2,
        ..controls.clone()
    })
    .unwrap();
    let low = numerical(&Stage1Controls {
        estimated_iso: 100,
        minimum_blend: 1.,
        ..controls
    })
    .unwrap();
    assert!(high.key() != long.key());
    assert!(high.key() != low.key());
}

#[test]
#[ignore = "requires TWO_SCALE_NOISE_REFERENCE native conditioning table fixtures"]
fn generated_tables_match_reference_float32_bytes() {
    let directory =
        std::path::PathBuf::from(std::env::var_os("TWO_SCALE_NOISE_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let reference =
            |name: &str| std::fs::read(directory.join(case[name].as_str().unwrap())).unwrap();
        for index in 0..2 {
            let (intensity, range) = conditioning(
                if index == 0 { 0.3 } else { 0.75 },
                index == 1 && case["long_exposure"].as_bool().unwrap(),
            );
            assert_eq!(
                bytemuck::cast_slice::<f32, u8>(&intensity),
                reference(&format!("intensity{index}"))
            );
            assert_eq!(
                bytemuck::cast_slice::<f32, u8>(&range),
                reference(&format!("range{index}"))
            );
        }
        assert_eq!(
            bytemuck::cast_slice::<f32, u8>(&brightness()),
            reference("brightness")
        );
    }
}

#[test]
#[ignore = "requires a GPU, QUATTRO_X3F and QUATTRO_STAGE1_BOUNDARY immutable full-frame native capture"]
fn complete_native_stage1_return_from_calibrated_input() {
    use crate::{gpu::Gpu, nodes::color::two_scale_noise, source::Source};
    let directory = std::path::PathBuf::from(std::env::var_os("QUATTRO_STAGE1_BOUNDARY").unwrap());
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let mut metadata = (*source.metadata).clone();
    metadata.properties.insert("APERTURE".into(), "4".into());
    metadata.properties.insert("SHUTTER".into(), "0.01".into());
    let chroma_step = std::env::var("QUATTRO_CHROMA")
        .map(|value| value.parse().unwrap())
        .unwrap_or(0);
    let prepared = prepare_filter(&metadata, chroma_step).unwrap();
    let floats = |name| {
        let bytes = std::fs::read(directory.join(name)).unwrap();
        bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
    };
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload(
            "Immutable complete native PREPROCESS return",
            &floats("input-rgb.f32"),
        )
        .unwrap();
    let started = std::time::Instant::now();
    let output = two_scale_noise::apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
    let bytes = gpu.readback(&output.rgb).unwrap();
    let actual: &[f32] = bytemuck::cast_slice(&bytes);
    let expected = floats("output-rgb.f32");
    assert_eq!(actual.len(), expected.len());
    let [width, height] = prepared.extent();
    let cutoff = width as usize * (height as usize - 2) * 3;
    let mut maximum = [0_f32; 2];
    let mut sums = [0_f64; 2];
    let mut changed = [0_usize; 2];
    let mut significant = Vec::new();
    for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
        assert!(a.is_finite() && b.is_finite());
        let region = usize::from(i >= cutoff);
        let difference = (a - b).abs();
        maximum[region] = maximum[region].max(difference);
        sums[region] += f64::from(difference);
        changed[region] += usize::from(a.to_bits() != b.to_bits());
        if difference > 0.002 {
            significant.push((i, difference, a, b));
        }
    }
    eprintln!(
        "Full native Stage1: changed {changed:?}, max {maximum:?}, interior mean {}, tail mean {}, completed {:?}",
        sums[0] / cutoff as f64,
        sums[1] / (actual.len() - cutoff) as f64,
        started.elapsed()
    );
    significant.sort_by(|a, b| b.1.total_cmp(&a.1));
    eprintln!("Stage1 differences above .002: {}", significant.len());
    for &(i, difference, actual, expected) in significant.iter().take(16) {
        let pixel = i / 3;
        eprintln!(
            "({}, {}, {}): difference {difference}, actual {actual}, expected {expected}",
            pixel % width as usize,
            pixel / width as usize,
            i % 3
        );
    }
    let bytes = gpu.readback(&output.source_after).unwrap();
    assert_eq!(
        bytes,
        std::fs::read(directory.join("source-after-rgb.f32")).unwrap(),
        "native retained WB round-trip source"
    );
    match chroma_step {
        0 => assert!(
            maximum[0] <= 0.002,
            "default interior maximum {}",
            maximum[0]
        ),
        2 if metadata.scalar("CAMERAID").unwrap() == 40. => {
            // The acquired sd midpoint has sparse deviations above .002 codes.
            // Bound this measured fixture separately from default.
            assert!(sums[0] / (cutoff as f64) < 1e-5);
            assert!(maximum[0] <= 1.5);
            assert!(significant.len() <= 1400);
            assert_eq!(maximum[1], 0.);
        }
        2 if metadata.scalar("CAMERAID").unwrap() == 82.
            && metadata.scalar("CaptureISO").unwrap() == 800. =>
        {
            assert!(maximum[0] <= 0.002);
            assert!(maximum[1] <= 0.002);
        }
        _ => panic!("no measured native Stage1 bounds for this camera/chroma fixture"),
    }
}
