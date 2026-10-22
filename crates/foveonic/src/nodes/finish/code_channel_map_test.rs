//! Compares channel mixing with guarded native integer fixtures.

use super::*;

#[test]
#[ignore = "requires GPU and CODE_MIX_REFERENCE"]
fn normalized_channel_mixing_stays_within_one_native_code() {
    let root = std::path::PathBuf::from(std::env::var_os("CODE_MIX_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let extent = std::array::from_fn(|i| case["extent"][i].as_u64().unwrap() as u32);
        let weights = std::array::from_fn(|i| case["weights"][i].as_f64().unwrap());
        let source = std::fs::read(root.join(case["source"].as_str().unwrap())).unwrap();
        let expected = std::fs::read(root.join(case["output"].as_str().unwrap())).unwrap();
        let input = gpu
            .upload(
                "Native channel mixing input",
                bytemuck::cast_slice::<u8, f32>(&source),
            )
            .unwrap();
        let output = apply(
            &gpu,
            &input,
            &Prepared::mix(extent, weights).unwrap(),
            &mut Default::default(),
        )
        .unwrap();
        let actual = gpu.readback(&output).unwrap();
        let differences: Vec<_> = bytemuck::cast_slice::<u8, f32>(&actual)
            .iter()
            .zip(bytemuck::cast_slice::<u8, f32>(&expected))
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, *a, *b))
            .collect();
        // Paired Float32 coefficients can land on the other side of an exact
        // integer boundary from native Double arithmetic. Bound that explicitly.
        assert!(
            differences.iter().all(|(_, a, b)| (a - b).abs() <= 1.),
            "{case}: {differences:?}"
        );
        assert_eq!(gpu.readback(&input).unwrap(), source);
    }
}
