//! Compares complete unsigned code conversions with owned native callback outputs.

use super::*;

#[test]
#[ignore = "requires a GPU and PACKED_LAB_REFERENCE fixtures"]
fn complete_native_callbacks() {
    let root = std::path::PathBuf::from(std::env::var("PACKED_LAB_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let extent = std::array::from_fn(|i| case["extent"][i].as_u64().unwrap() as u32);
        let matrix = std::array::from_fn(|i| case["matrix"][i].as_f64().unwrap());
        let direction = if case["inverse"].as_bool().unwrap() {
            Direction::Inverse
        } else {
            Direction::Forward
        };
        let input = std::fs::read(root.join(format!("{name}-input.f32"))).unwrap();
        let wanted = std::fs::read(root.join(format!("{name}-output.f32"))).unwrap();
        let source = gpu
            .upload("RGB/Lab codes", bytemuck::cast_slice::<u8, f32>(&input))
            .unwrap();
        let result = apply(
            &gpu,
            &source,
            &Prepared::new(extent, direction, matrix).unwrap(),
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&result).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        let expected: &[f32] = bytemuck::cast_slice(&wanted);
        let mut changed = 0;
        let mut maximum = 0_f32;
        for (&a, &b) in actual.iter().zip(expected) {
            if a != b {
                changed += 1;
                maximum = maximum.max((a - b).abs());
            }
        }
        eprintln!("{name}: changed {changed}/{} max {maximum}", actual.len());
        assert!(maximum <= 1., "{name} maximum integer difference {maximum}");
        assert!(gpu.readback(&source).unwrap() == input);
    }
}
