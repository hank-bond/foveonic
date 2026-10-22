//! Checks bounded residual gain and signed-detail controls.
use super::*;
// Float32 word selected by the ordinary unit-scale residual LUT.
const SELECTED_ADAPTIVE: f32 = 1.600_000_1;
#[test]
fn rejects_invalid_scale() {
    assert!(Params::new([8, 8], 0., 16., 0, 1.6, 0.).is_err());
}
#[test]
#[ignore = "requires BLUE_GUIDED_REFERENCE and GPU"]
fn independent_sharp_and_gain_fixtures_match() {
    let directory = std::path::PathBuf::from(std::env::var_os("BLUE_GUIDED_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("finish-cases.json")).unwrap())
            .unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mode = case["mode"].as_u64().unwrap() as u32;
        let read = |key: &str| -> Vec<f32> {
            std::fs::read(directory.join(format!("{name}-{key}.f32")))
                .unwrap()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| f32::from_le_bytes(*v))
                .collect()
        };
        let rgb = gpu.upload("Residual RGB", &read("rgb-input")).unwrap();
        let residual = gpu
            .upload("Residual gains", &read("residual-input"))
            .unwrap();
        let mask = gpu
            .upload(
                "Saturation mask",
                &read("mask")
                    .into_iter()
                    .map(|v| v as u32)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let output = apply(
            &gpu,
            &rgb,
            &residual,
            Some(&mask),
            &Params::new([32, 24], 1., 16., mode, SELECTED_ADAPTIVE, 0.7).unwrap(),
            &mut Profile::default(),
        )
        .unwrap();
        for (key, buffer) in [("rgb-output", output.rgb), ("detail-output", output.detail)] {
            let bytes = gpu.readback(&buffer).unwrap();
            let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
            let expected = read(key);
            let bad: Vec<_> = actual
                .iter()
                .zip(&expected)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| (i, *a, *b))
                .take(12)
                .collect();
            assert!(bad.is_empty(), "{name}/{key}: {bad:?}");
        }
    }
}
#[test]
#[ignore = "requires STAGE2B_NATIVE_BOUNDARY with apply_inputs and GPU"]
fn full_actual_residual_inputs_match_rgb_and_detail() {
    let directory = std::path::PathBuf::from(std::env::var_os("STAGE2B_NATIVE_BOUNDARY").unwrap());
    let raw = |n: &str| std::fs::read(directory.join(n)).unwrap();
    let input = raw("apply-rgb.bin");
    let input = input
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| f32::from(u16::from_le_bytes(*v)))
        .collect::<Vec<_>>();
    let gain = raw("apply-residual.bin");
    let gain = bytemuck::cast_slice::<u8, f32>(&gain);
    let mask = raw("apply-mask.bin")
        .into_iter()
        .map(u32::from)
        .collect::<Vec<_>>();
    let table = raw("apply-adaptive.f32");
    let adaptive = f32::from_le_bytes(table[..4].try_into().unwrap());
    assert!(table
        .as_chunks::<4>()
        .0
        .iter()
        .all(|v| f32::from_le_bytes(*v) == adaptive));
    let data = raw("apply-data.bin");
    let field = |o| f32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    let parameters = raw("parameters-entry.bin");
    let sharp_threshold = f32::from_le_bytes(parameters[0xf0..0xf4].try_into().unwrap());
    let gpu = Gpu::new().unwrap();
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
    let result = apply(
        &gpu,
        &gpu.upload("Native reconstructed RGB", &input).unwrap(),
        &gpu.upload("Native residual gains", gain).unwrap(),
        Some(&gpu.upload("Native saturation mask", &mask).unwrap()),
        &Params::new(
            [5888, 3672],
            field(0x38),
            sharp_threshold,
            0,
            adaptive,
            field(0x28),
        )
        .unwrap(),
        &mut profile,
    )
    .unwrap();
    for (name, buffer, signed, channels) in [
        ("output-rgb.u16", result.rgb, false, 3usize),
        ("output-detail.i16", result.detail, true, 1),
    ] {
        let b = gpu.readback(&buffer).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&b);
        let expected = raw(name);
        let words = expected.as_chunks::<2>().0;
        let mut changed = 0;
        let mut max = 0_f32;
        let mut first = vec![];
        for y in 28..=3643 {
            for x in 216..=5639 {
                for c in 0..channels {
                    let i = (y * 5888 + x) * channels + c;
                    let value = if signed {
                        f32::from(i16::from_le_bytes(words[i]))
                    } else {
                        f32::from(u16::from_le_bytes(words[i]))
                    };
                    let error = (actual[i] - value).abs();
                    if error != 0. {
                        changed += 1;
                        if first.len() < 12 {
                            first.push((x, y, c, actual[i], value));
                        }
                    }
                    max = max.max(error);
                }
            }
        }
        eprintln!(
            "{name}: changed{changed} max{max} {first:?}; {:?}",
            profile.gpu
        );
        assert_eq!(changed, 0);
    }
}
