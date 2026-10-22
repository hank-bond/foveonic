//! Checks immutable RGB outlier filtering against independent equations.
use super::*;
#[test]
fn rejects_invalid_controls() {
    assert!(Prepared::new([4, 9], [1.; 9], 1., [0.; 9]).is_err());
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
