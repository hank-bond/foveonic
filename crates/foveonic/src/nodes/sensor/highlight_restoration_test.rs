//! Checks source highlight controls, directional reconstruction and green masks.
use super::*;
#[test]
fn singular_source_inverse_is_rejected() {
    assert!(reference_inverse([0.; 9]).is_err());
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
