//! Compares native native two-by-two gain recurrence, including the complete blue plane.

use super::*;

#[test]
#[ignore = "requires a GPU and CELL_GAIN_REFERENCE native shading fixtures"]
fn complete_native_cell_gains() {
    let directory = std::path::PathBuf::from(std::env::var_os("CELL_GAIN_REFERENCE").unwrap());
    let m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for f in m["cases"].as_array().unwrap() {
        let floats = |k: &str| {
            let b = std::fs::read(directory.join(f[k].as_str().unwrap())).unwrap();
            bytemuck::cast_slice::<u8, f32>(&b).to_vec()
        };
        let extent = [
            f["extent"][0].as_u64().unwrap() as u32,
            f["extent"][1].as_u64().unwrap() as u32,
        ];
        let grid_extent = [
            f["grid_extent"][0].as_u64().unwrap() as u32,
            f["grid_extent"][1].as_u64().unwrap() as u32,
        ];
        let p = Prepared::new(
            extent,
            Grid {
                extent: grid_extent,
                values: floats("blue"),
            },
        )
        .unwrap();
        assert_eq!(p.extent(), extent);
        let _key = p.key();
        let input = gpu
            .upload("Native spatial gain input", &floats("input"))
            .unwrap();
        let output = apply(&gpu, &input, &p, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        let expected = floats("output");
        let mut maximum = 0_f32;
        let mut changed = 0;
        for (&a, &b) in actual.iter().zip(&expected) {
            assert!(a.is_finite() && b.is_finite());
            maximum = maximum.max((a - b).abs());
            changed += usize::from(a.to_bits() != b.to_bits());
        }
        eprintln!(
            "{}: {changed}/{} changed, max {maximum}",
            f["name"],
            actual.len()
        );
        assert!(maximum == 0.);
    }
}
