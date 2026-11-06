//! Checks signed chroma kernels against composed native row callbacks.
use super::*;
#[test]
fn rejects_invalid_correction_contracts() {
    assert!(Prepared::new([17, 11], [0., 1., 1.], [0.8; 3], 4, 3, Support::Binomial3).is_err());
    assert!(Prepared::new([17, 11], [1.; 3], [0.8; 3], 4, 16, Support::Box5).is_err());
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_signed_chroma_matches_native_composed_rows() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/data/chroma-reference.json")).unwrap();
    let input: Vec<f32> = fixture["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap() as f32)
        .collect();
    let gpu = Gpu::new().unwrap();
    let source = gpu.upload("Signed chroma native fixture", &input).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let triplet = |name: &str| std::array::from_fn(|i| case[name][i].as_f64().unwrap());
        let p = Prepared::new(
            [17, 11],
            triplet("amplitude"),
            triplet("slope"),
            case["setting"].as_u64().unwrap() as u32,
            case["shift"].as_u64().unwrap() as u32,
            if case["support"] == "binomial3" {
                Support::Binomial3
            } else {
                Support::Box5
            },
        )
        .unwrap();
        let output = apply(&gpu, &source, &p, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        for (i, v) in actual.iter().enumerate() {
            assert_eq!(
                *v,
                case["expected"][i].as_i64().unwrap() as f32,
                "{} pixel {i}",
                case["support"]
            );
        }
    }
    assert_eq!(
        gpu.readback(&source).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&input)
    );
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_recursive_chroma_matches_native_callbacks() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/chroma-recursive-reference.json"
    ))
    .unwrap();
    let gpu = Gpu::new().unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let width = case["width"].as_u64().unwrap() as u32;
        let height = case["height"].as_u64().unwrap() as u32;
        let input: Vec<f32> = case["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as f32)
            .collect();
        let source = gpu.upload("Recursive chroma reference", &input).unwrap();
        let prepared = Prepared::new(
            [width, height],
            [16., 32., 64.],
            [0.8; 3],
            128,
            3,
            Support::Recursive,
        )
        .unwrap();
        let output = apply(&gpu, &source, &prepared, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        for (index, value) in actual.iter().enumerate() {
            assert_eq!(
                *value,
                case["expected"][index].as_i64().unwrap() as f32,
                "{width}x{height} sample{index}"
            );
        }
        assert_eq!(
            gpu.readback(&source).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&input)
        );
    }
}

#[test]
fn reduced_chroma_changes_geometry_and_spatial_identity_only() {
    for support in [Support::Binomial3, Support::Box5] {
        let original = Prepared::new([128, 96], [16.; 3], [0.8; 3], 128, 3, support).unwrap();
        let reduced = original.clone().on_grid([16, 12], 8).unwrap();
        assert_eq!(reduced.params.shape[..2], [16, 12]);
        assert_eq!(reduced.tables, original.tables);
        assert_eq!(
            reduced.params.correction[..2],
            original.params.correction[..2]
        );
        assert_ne!(reduced.key(), original.key());
    }
    let recursive =
        Prepared::new([128, 96], [16.; 3], [0.8; 3], 128, 3, Support::Recursive).unwrap();
    assert!(recursive.on_grid([16, 12], 8).is_err());
}

#[test]
#[ignore = "requires a GPU"]
fn reduced_chroma_preserves_native_response_on_signed_constants() {
    let gpu = Gpu::new().unwrap();
    let pixels: Vec<f32> = vec![-100.; 16 * 12 * 3];
    let input = gpu.upload("Reduced neutral signed codes", &pixels).unwrap();
    for divisor in [4, 8] {
        for support in [Support::Binomial3, Support::Box5] {
            let native = Prepared::new([16, 12], [16.; 3], [0.8; 3], 128, 3, support).unwrap();
            let expected = apply(&gpu, &input, &native, &mut Profile::default()).unwrap();
            let prepared = native.on_grid([16, 12], divisor).unwrap();
            let output = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
            let bytes = gpu.readback(&output).unwrap();
            assert!(
                bytes == gpu.readback(&expected).unwrap(),
                "{support:?} divisor {divisor}"
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn reduced_chroma_contracts_spatial_support_without_changing_noise_tables() {
    let gpu = Gpu::new().unwrap();
    let pixels: Vec<f32> = (0..16 * 12)
        .flat_map(|i| [1000. + if i % 2 == 0 { 64. } else { -64. }, 1000., 1000.])
        .collect();
    let input = gpu
        .upload("Signed chroma spatial pattern", &pixels)
        .unwrap();
    for support in [Support::Binomial3, Support::Box5] {
        let native = Prepared::new([16, 12], [32.; 3], [0.8; 3], 128, 3, support).unwrap();
        let reduced = native.clone().on_grid([16, 12], 8).unwrap();
        assert_eq!(native.tables, reduced.tables);
        let difference = |prepared: &Prepared| {
            let output = apply(&gpu, &input, prepared, &mut Profile::default()).unwrap();
            let bytes = gpu.readback(&output).unwrap();
            bytemuck::cast_slice::<u8, f32>(&bytes)
                .iter()
                .zip(&pixels)
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>()
        };
        assert!(difference(&reduced) < difference(&native));
    }
}
