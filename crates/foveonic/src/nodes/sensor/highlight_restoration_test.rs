//! Checks source highlight controls, directional reconstruction and green masks.
use super::*;
#[test]
fn singular_source_inverse_is_rejected() {
    assert!(reference_inverse([0.; 9]).is_err());
}

#[test]
#[ignore = "requires GPU"]
fn partial_color_cells_keep_edge_recovery_and_original_samples() {
    let gpu = Gpu::new().unwrap();
    for [width, height] in [[35, 33], [50, 35]] {
        let mut source = [1000_f32, 2000., 3000.].repeat(width * height);
        for y in 0..height {
            source[(y * width + width - 1) * 3 + 2] = 14000.;
        }
        let input = gpu
            .upload("Partial-cell highlight fixture", &source)
            .unwrap();
        let prepared = Prepared::new(
            [width as u32, height as u32],
            [1, 1, width as u32 - 1, height as u32 - 1],
            1.,
            [1.; 3],
            [1., 0., 0., 0., 1., 0., 0., 0., 1.],
        )
        .unwrap()
        .with_channel_limits([14000.; 3], 13000., true)
        .unwrap();
        let output = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output.recovery_mask.unwrap()).unwrap();
        let masks: &[u32] = bytemuck::cast_slice(&bytes);
        for y in 0..height {
            for x in 0..width {
                assert_eq!(masks[y * width + x], if x == width - 1 { 4 } else { 0 });
            }
        }
        let bytes = gpu.readback(&output.rgb).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .all(|v| v.is_finite()));
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&source)
        );
    }
}
#[test]
#[ignore = "requires GPU and HIGHLIGHT_REFERENCE"]
fn enabled_restoration_matches_independent_equations() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("HIGHLIGHT_REFERENCE").expect("set HIGHLIGHT_REFERENCE"),
    );
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    let read = |name: String| -> Vec<f32> {
        let b = std::fs::read(directory.join(name)).unwrap();
        b.as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect()
    };
    let mut maximum = 0_f32;
    for c in cases.as_array().unwrap() {
        let n = c["n"].as_u64().unwrap();
        let array = |name: &str| {
            c[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<_>>()
        };
        let extent = array("extent");
        let [w, h] = [extent[0] as u32, extent[1] as u32];
        let prepared = Prepared::new(
            [w, h],
            [1, 1, w - 1, h - 1],
            c["normalization"].as_f64().unwrap() as f32,
            array("gains")
                .into_iter()
                .map(|v| v as f32)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            array("matrix").try_into().unwrap(),
        )
        .unwrap();
        let input = gpu
            .upload(
                "Independent highlight input",
                &read(format!("{n}-input.f32")),
            )
            .unwrap();
        let output = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        let b = gpu.readback(&output.rgb).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&b);
        let expected = read(format!("{n}-output.f32"));
        for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
            let e = (a - b).abs();
            maximum = maximum.max(e);
            assert!(
                e <= 0.01_f32.max(b.abs() * f32::EPSILON * 16.),
                "case{n} sample{i} {a} vs {b}"
            );
        }
        let bytes = gpu.readback(&output.mask).unwrap();
        let actual = bytemuck::cast_slice::<u8, u32>(&bytes);
        let expected = std::fs::read(directory.join(format!("{n}-mask.u8"))).unwrap();
        assert!(actual
            .iter()
            .copied()
            .eq(expected.into_iter().map(u32::from)));
    }
    eprintln!("Independent highlight max{maximum}");
}

#[test]
#[ignore = "requires GPU and TIGER_HIGHLIGHT_REFERENCE"]
fn channel_recovery_matches_independent_complete_highlight_equations() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("TIGER_HIGHLIGHT_REFERENCE").expect("set TIGER_HIGHLIGHT_REFERENCE"),
    );
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let n = case["n"].as_u64().unwrap();
        let extent = std::array::from_fn(|i| case["extent"][i].as_u64().unwrap() as u32);
        let gains = std::array::from_fn(|i| case["gains"][i].as_f64().unwrap() as f32);
        let scale = case["scale"].as_f64().unwrap() as f32;
        let active = case["active"].as_bool().unwrap();
        let ceilings = if active && scale == 1. {
            [22000., 24000., 14000.]
        } else {
            [14000. / scale; 3]
        };
        let green = if active && scale == 1. {
            24000.
        } else {
            13000. / scale
        };
        let prepared = Prepared::new(
            extent,
            [1, 1, extent[0] - 1, extent[1] - 1],
            scale,
            gains,
            std::array::from_fn(|i| case["matrix"][i].as_f64().unwrap()),
        )
        .unwrap()
        .with_channel_limits(ceilings, green, active)
        .unwrap();
        let bytes = std::fs::read(directory.join(format!("{n}-input.bin"))).unwrap();
        let input = gpu
            .upload(
                "Independent channel highlight input",
                bytemuck::cast_slice::<u8, f32>(&bytes),
            )
            .unwrap();
        let output = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output.rgb).unwrap();
        let expected = std::fs::read(directory.join(format!("{n}-output.bin"))).unwrap();
        let mut maximum = 0_f32;
        for (i, (&a, &b)) in bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .zip(bytemuck::cast_slice::<u8, f32>(&expected))
            .enumerate()
        {
            maximum = maximum.max((a - b).abs());
            assert!(
                a.is_finite() && (a - b).abs() <= 0.03_f32.max(b.abs() * f32::EPSILON * 32.),
                "case{n} sample{i}: {a} vs {b}"
            );
        }
        eprintln!("Channel highlight case{n}: maximum error {maximum}");
        for (tag, buffer) in [
            ("green", output.mask),
            ("recovery", output.recovery_mask.unwrap()),
        ] {
            let actual = gpu.readback(&buffer).unwrap();
            let expected = std::fs::read(directory.join(format!("{n}-{tag}.bin"))).unwrap();
            assert_eq!(actual, expected, "case{n} {tag} mask");
        }
    }
}
