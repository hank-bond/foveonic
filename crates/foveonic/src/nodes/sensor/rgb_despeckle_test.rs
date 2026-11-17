//! Checks immutable RGB outlier filtering against independent equations.
use super::*;
#[test]
fn rejects_invalid_controls() {
    assert!(Prepared::new([4, 9], [1.; 9], 1., [0.; 9]).is_err());
}

#[test]
#[ignore = "requires GPU"]
fn reduced_detection_repairs_bright_outliers_and_keeps_dark_details() {
    let gpu = Gpu::test_executor();
    for divisor in [2, 4, 8] {
        let grid = crate::render::WorkingGrid::new(
            [72, 72],
            crate::render::Scale::new(divisor).unwrap(),
            8,
        )
        .unwrap()
        .with_halo(4)
        .unwrap();
        let prepared = Prepared::new([72, 72], [0.; 9], 1., [1., 0., 0., 0., 1., 0., 0., 0., 1.])
            .unwrap()
            .on_grid(grid, 1. / f64::from(divisor * divisor))
            .unwrap();
        let [w, h] = grid.extent;
        let center = ((h / 2 * w + w / 2) * 3) as usize;
        for value in [0., 1000.] {
            let mut values = vec![100_f32; (w * h * 3) as usize];
            values[center..center + 3].fill(value);
            let input = gpu
                .upload("Reduced RGB detection fixture", &values)
                .unwrap();
            let flags = gpu
                .upload("Reduced RGB detection mask", &vec![0_u32; (w * h) as usize])
                .unwrap();
            let output = apply(&gpu, &input, &flags, &prepared, &mut Profile::default()).unwrap();
            let bytes = gpu.readback(&output).unwrap();
            let result: &[f32] = bytemuck::cast_slice(&bytes);
            let expected = if value == 0. { 0. } else { 100. };
            assert!(result[center..center + 3]
                .iter()
                .all(|v| (*v - expected).abs() < 0.01));
            assert_eq!(
                gpu.readback(&input).unwrap(),
                bytemuck::cast_slice::<f32, u8>(&values)
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn isolated_rgb_outlier_is_repaired_unless_masked() {
    let gpu = Gpu::test_executor();
    let extent = [9, 9];
    let prepared =
        Prepared::new(extent, [0.; 9], 1., [1., 0., 0., 0., 1., 0., 0., 0., 1.]).unwrap();
    let mut values = vec![100_f32; 9 * 9 * 3];
    let center = (4 * 9 + 4) * 3;
    values[center..center + 3].fill(1000.);
    let input = gpu.upload("RGB outlier fixture", &values).unwrap();
    for masked in [false, true] {
        let mut flags = vec![0_u32; 9 * 9];
        flags[4 * 9 + 4] = u32::from(masked);
        let mask = gpu.upload("RGB outlier mask", &flags).unwrap();
        let output = apply(&gpu, &input, &mask, &prepared, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let expected = if masked { 1000. } else { 100. };
        for value in &actual[center..center + 3] {
            assert!((value - expected).abs() < 0.01);
        }
    }
}

#[test]
#[ignore = "requires GPU and RGB_NATIVE_BOUNDARY"]
fn full_immutable_pass_matches_native() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("RGB_NATIVE_BOUNDARY").expect("set RGB_NATIVE_BOUNDARY"),
    );
    let c: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("rgb-boundaries.json")).unwrap())
            .unwrap();
    let c = &c["despeckle"];
    let floats = |name: &str| -> Vec<f32> {
        let b = std::fs::read(directory.join(name)).unwrap();
        b.as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect()
    };
    let noise = c["noise"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let matrix = c["matrix"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let prepared = Prepared::new([2944, 1836], noise, 1., matrix).unwrap();
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload(
            "Native RGB despeckle input",
            &floats("rgb-despeckle-input.f32"),
        )
        .unwrap();
    let mask = std::fs::read(directory.join("rgb-despeckle-mask.u8"))
        .unwrap()
        .into_iter()
        .map(u32::from)
        .collect::<Vec<_>>();
    let mask = gpu.upload("Native RGB mask", &mask).unwrap();
    let output = apply(&gpu, &input, &mask, &prepared, &mut Default::default()).unwrap();
    let b = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&b);
    let expected = floats("rgb-despeckle-output.f32");
    let mut max = 0_f32;
    let mut sum = 0_f64;
    let mut changed = 0;
    for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
        let e = (a - b).abs();
        assert!(a.is_finite());
        max = max.max(e);
        sum += f64::from(e);
        changed += usize::from(a.to_bits() != b.to_bits());
        assert!(
            e <= 0.008_f32.max(b.abs() * f32::EPSILON * 8.),
            "sample{i} {a} vs {b}"
        );
    }
    eprintln!(
        "RGB despeckle changed{changed}/{} mean{} max{max}",
        actual.len(),
        sum / actual.len() as f64
    );
}
