//! Checks preparation tables independently of the pixel implementation.

use super::*;

#[test]
fn response_tables_have_the_selected_finite_domain() {
    let (brightness, response, adaptive) = tables(4.);
    assert_eq!(
        (brightness.len(), response.len(), adaptive.len()),
        (901, 3072, 4096)
    );
    assert!(brightness
        .iter()
        .chain(&response)
        .chain(&adaptive)
        .all(|v| v.is_finite()));
    assert_eq!(brightness[900], 0.);
    assert_eq!(response[0], 0.);
    assert_eq!(adaptive[0], 15.);
    assert_eq!(adaptive[4095], 4.);
}

#[test]
#[ignore = "requires TIGER_X3F and RAW_COLOR_BLEND_REFERENCE original Tiger helper fixture"]
fn tiger_metadata_preparation_matches_original_native_helper() {
    let source = crate::source::Source::open(std::env::var_os("TIGER_X3F").unwrap()).unwrap();
    let root = std::path::PathBuf::from(std::env::var_os("RAW_COLOR_BLEND_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let case = &manifest["cases"][0];
    assert_eq!(case["name"], "original-tiger");
    let scalar = |name: &str| case[name].as_f64().unwrap() as f32;
    let floats = |name: &str| -> Vec<f32> {
        let bytes = std::fs::read(root.join(case[name].as_str().unwrap())).unwrap();
        bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
    };
    let controls = Controls {
        extent: super::super::calibration::full_extent(&source.metadata).unwrap(),
        white_balance: std::array::from_fn(|i| case["white_balance"][i].as_f64().unwrap() as f32),
        normalization: scalar("normalization"),
        neutral_low: scalar("neutral_low"),
        neutral_high: scalar("neutral_high"),
        matrix: std::array::from_fn(|i| case["matrix"][i].as_f64().unwrap()),
        input_divisor: scalar("input_divisor"),
        output_gain: scalar("output_gain"),
        brightness_factor: scalar("brightness_factor"),
        brightness_offset: scalar("brightness_offset"),
        brightness_gain: scalar("brightness_gain"),
        second_exponent: scalar("second_exponent"),
        residual_scale: scalar("residual_scale"),
        hue_strength: std::array::from_fn(|i| case["hue_strength"][i].as_f64().unwrap() as f32),
    };
    let expected = Prepared::new(
        controls,
        floats("brightness"),
        floats("signed_response"),
        floats("adaptive"),
    )
    .unwrap()
    .with_masked_highlights([22000., 24000., 14000.], scalar("highlight_strength"))
    .unwrap();
    let actual = prepare(
        &source.metadata,
        scalar("highlight_strength"),
        "Sunlight",
        "Standard",
    )
    .unwrap();
    assert!(
        actual.key() == expected.key(),
        "metadata-derived controls differ from native helper"
    );
}

#[test]
#[ignore = "requires RAW_COLOR_BLEND_REFERENCE native table fixtures and QUATTRO_X3F metadata"]
fn generated_raw_color_tables_match_reference_bytes() {
    let directory =
        std::path::PathBuf::from(std::env::var_os("RAW_COLOR_BLEND_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let case = &manifest["cases"][0];
    let (brightness, response, adaptive) = tables(4.);
    for (name, values) in [
        ("brightness", brightness),
        ("signed_response", response),
        ("adaptive", adaptive),
    ] {
        assert_eq!(
            bytemuck::cast_slice::<f32, u8>(&values),
            std::fs::read(directory.join(case[name].as_str().unwrap())).unwrap(),
            "{name}"
        );
    }
    let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let first = prepare(&source.metadata, 0., "Sunlight", "Standard").unwrap();
    let second = prepare(&source.metadata, 1., "Sunlight", "Standard").unwrap();
    assert_eq!(first.extent(), [2944, 1836]);
    assert!(first.key() != second.key());
}

#[test]
#[ignore = "requires GPU, QUATTRO_X3F and QUATTRO_STAGE2_BOUNDARY successful full native Stage2 return"]
fn complete_native_outer_stage2_from_stage1_return() {
    use crate::{
        gpu::Gpu,
        nodes::color::{raw_color_blend, sector_color, spatial_color_gain},
    };
    let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let directory = std::path::PathBuf::from(std::env::var_os("QUATTRO_STAGE2_BOUNDARY").unwrap());
    let gpu = Gpu::new().unwrap();
    let bytes = std::fs::read(directory.join("input-rgb.f32")).unwrap();
    let input = gpu
        .upload(
            "Immutable complete native Stage1 return",
            bytemuck::cast_slice::<u8, f32>(&bytes),
        )
        .unwrap();
    let started = std::time::Instant::now();
    let mut profile = Default::default();
    let shading = prepare_shading(&source.metadata, "Sunlight").unwrap();
    let shaded = if let Some(p) = shading {
        spatial_color_gain::apply(&gpu, &input, &p, &mut profile).unwrap()
    } else {
        input
    };
    let raw = raw_color_blend::apply(
        &gpu,
        &shaded,
        None,
        &prepare(&source.metadata, 1., "Sunlight", "Standard").unwrap(),
        &mut profile,
    )
    .unwrap();
    let output = if directory.join("mcc-controls.bin").is_file() {
        sector_color::apply(
            &gpu,
            &raw.rgb,
            &super::super::mcc::prepare(&source.metadata, "Standard").unwrap(),
            &mut profile,
        )
        .unwrap()
    } else {
        raw.rgb
    };
    let actual = gpu.readback(&output).unwrap();
    let actual: &[f32] = bytemuck::cast_slice(&actual);
    let bytes = std::fs::read(directory.join("output-rgb.f32")).unwrap();
    let expected: &[f32] = bytemuck::cast_slice(&bytes);
    assert_eq!(actual.len(), expected.len());
    let mut maximum = 0_f32;
    let mut sum = 0_f64;
    let mut changed = 0;
    let mut worst = (0, 0_f32, 0_f32);
    for (i, (&a, &b)) in actual.iter().zip(expected).enumerate() {
        assert!(a.is_finite() && b.is_finite());
        let d = (a - b).abs();
        if d > maximum {
            maximum = d;
            worst = (i, a, b);
        }
        sum += f64::from(d);
        changed += usize::from(a.to_bits() != b.to_bits());
    }
    eprintln!(
        "Full Stage2 {changed}/{} changed, max {maximum}, mean {}, worst {worst:?}, completed {:?}",
        actual.len(),
        sum / actual.len() as f64,
        started.elapsed()
    );
    assert!(maximum <= 0.02);
}

#[test]
#[ignore = "requires GPU, QUATTRO_X3F, QUATTRO_STAGE1_BOUNDARY and STAGE2B_NATIVE_BOUNDARY immutable histories"]
fn retained_calibration_history_matches_stage2b_entry() {
    use crate::{gpu::Gpu, nodes::color::spatial_color_gain};
    let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let stage1 = std::path::PathBuf::from(std::env::var_os("QUATTRO_STAGE1_BOUNDARY").unwrap());
    let stage2b = std::path::PathBuf::from(std::env::var_os("STAGE2B_NATIVE_BOUNDARY").unwrap());
    let original = std::fs::read(stage1.join("source-after-rgb.f32")).unwrap();
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload(
            "Retained native Stage1 source history",
            bytemuck::cast_slice::<u8, f32>(&original),
        )
        .unwrap();
    let prepared = prepare_shading(&source.metadata, "Sunlight")
        .unwrap()
        .unwrap();
    let output =
        spatial_color_gain::apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
    let actual = gpu.readback(&output).unwrap();
    let expected = std::fs::read(stage2b.join("calibrated-input.f32")).unwrap();
    assert_eq!(actual.len(), expected.len());
    let actual: &[f32] = bytemuck::cast_slice(&actual);
    let expected: &[f32] = bytemuck::cast_slice(&expected);
    let mut changed = 0;
    let mut maximum = 0_f32;
    let mut sum = 0_f64;
    for (&a, &b) in actual.iter().zip(expected) {
        let d = (a - b).abs();
        changed += usize::from(a.to_bits() != b.to_bits());
        maximum = maximum.max(d);
        sum += f64::from(d);
    }
    eprintln!(
        "Retained Stage2B calibration history {changed}/{} changed, max {maximum}, mean {}",
        actual.len(),
        sum / actual.len() as f64
    );
    assert!(maximum <= 0.00025);
    assert!(sum / actual.len() as f64 <= 1e-8);
}

#[test]
#[ignore = "requires QUATTRO_X3F pointing to an ordinary DP Quattro original"]
fn original_dp_metadata_prepares_all_shared_color_stages() {
    let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let metadata = &source.metadata;
    let expected = [(metadata.width / 2) as u32, (metadata.height / 2) as u32];
    let noise = super::super::stage1::prepare_filter(metadata, 0).unwrap();
    assert_eq!(noise.extent(), expected);
    assert_eq!(
        super::super::repair::prepare(metadata, false)
            .unwrap()
            .extent(),
        expected
    );
    assert_eq!(
        prepare(metadata, 1., "Sunlight", "Standard")
            .unwrap()
            .extent(),
        expected
    );
    assert_eq!(
        super::super::mcc::prepare(metadata, "Standard")
            .unwrap()
            .extent(),
        expected
    );
    prepare_shading(metadata, "Sunlight").unwrap();
}

#[test]
#[ignore = "requires QUATTRO_BRIGHTNESS_TABLE containing native sd brightness entries"]
fn interchangeable_lens_brightness_fit_matches_native_table() {
    let bytes = std::fs::read(std::env::var_os("QUATTRO_BRIGHTNESS_TABLE").unwrap()).unwrap();
    let (brightness, _, _) = tables(8.);
    assert_eq!(bytemuck::cast_slice::<f32, u8>(&brightness), bytes);
}

#[test]
#[ignore = "requires QUATTRO_X3F ordinary original"]
fn every_camera_balance_prepares_gain_matrix_and_recorded_shading_compensation() {
    let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let metadata = &source.metadata;
    let recorded = super::super::hlp::recorded_balance(metadata).unwrap();
    let gains = &metadata.lists["WhiteBalanceGains"];
    let mut keys = Vec::new();
    for selected in gains.names() {
        let gain = super::super::calibration::white_balance(metadata, selected).unwrap();
        assert_eq!(gain[1], 1.);
        for mode in super::super::appearance::MODES {
            let prepared = prepare(metadata, 1., selected, mode).unwrap();
            keys.push(prepared.key());
        }
        let shading = prepare_shading(metadata, selected).unwrap();
        if selected == recorded && !super::super::calibration::is_ordinary_sd(metadata) {
            assert!(shading.is_some());
            for channel in ["R", "B"] {
                assert!(compensation_grid(metadata, selected, channel)
                    .unwrap()
                    .values
                    .iter()
                    .all(|gain| *gain == 1.));
            }
        }
        assert_eq!(
            super::super::hlp::recorded_balance(metadata).unwrap(),
            recorded
        );
    }
    assert!(keys.len() >= 7);
    assert!(keys.windows(2).any(|pair| pair[0] != pair[1]));
    assert!(
        prepare(metadata, 1., "ColorTemp", "Standard").is_err(),
        "temperature editing needs its own preparation"
    );
    assert!(
        prepare_shading(metadata, "unsupported-preset").is_err()
            || super::super::calibration::is_ordinary_sd(metadata)
    );
}

#[test]
#[ignore = "requires a GPU"]
fn identity_shading_keeps_rgb_history_and_blue_samples_exact() {
    use crate::{
        gpu::Gpu,
        nodes::color::{cell_gain, spatial_color_gain},
    };
    let gpu = Gpu::new().unwrap();
    let grid = || spatial_color_gain::Grid {
        extent: [3, 3],
        values: vec![1.; 9],
    };
    let rgb: Vec<f32> = (0..17 * 13 * 3)
        .map(|index| (index as f32 - 71.25) * 1.125)
        .collect();
    let input = gpu.upload("Unchanged shading history", &rgb).unwrap();
    let output = spatial_color_gain::apply(
        &gpu,
        &input,
        &spatial_color_gain::Prepared::new([17, 13], grid(), grid()).unwrap(),
        &mut Default::default(),
    )
    .unwrap();
    assert_eq!(
        gpu.readback(&output).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&rgb)
    );
    let blue: Vec<f32> = (0..34 * 26)
        .map(|index| (index as f32 - 39.75) * 0.625)
        .collect();
    let input = gpu.upload("Unchanged high-resolution blue", &blue).unwrap();
    let output = cell_gain::apply(
        &gpu,
        &input,
        &cell_gain::Prepared::new([34, 26], grid()).unwrap(),
        &mut Default::default(),
    )
    .unwrap();
    assert_eq!(
        gpu.readback(&output).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&blue)
    );
}
