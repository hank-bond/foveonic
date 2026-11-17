//! Checks signed chroma kernels against composed native row callbacks.
use super::*;
#[test]
fn rejects_invalid_correction_contracts() {
    assert!(Prepared::new([17, 11], [0., 1., 1.], [0.8; 3], 4, 3, Support::Binomial3).is_err());
    assert!(Prepared::new([17, 11], [1.; 3], [0.8; 3], 4, 16, Support::Box5).is_err());
}

#[test]
fn every_power_of_two_preserves_finite_physical_chroma_support() {
    for support in [Support::Binomial3, Support::Box5, Support::Recursive] {
        let native = Prepared::new([128, 96], [16.; 3], [0.8; 3], 128, 3, support).unwrap();
        for level in 0..32 {
            let divisor = 1_u32 << level;
            let prepared = native.clone().on_grid([16, 16], divisor).unwrap();
            assert_eq!(prepared.params.correction[2], divisor);
            assert_eq!(prepared.tables, native.tables);
            if support == Support::Recursive && divisor > 1 {
                let group = prepared.params.correction[3];
                assert!([1, 2, 4].contains(&group));
                assert!(prepared
                    .params
                    .decay
                    .into_iter()
                    .all(|v| (0..=256).contains(&v)));
                let expected = (-f64::from(divisor) / 5.).exp();
                assert!((f64::from(prepared.params.decay[1]) / 256. - expected).abs() < 1. / 256.);
            }
        }
        for invalid in [0, 3, 12, u32::MAX] {
            assert!(native.clone().on_grid([16, 16], invalid).is_err());
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn large_divisors_do_not_wrap_signed_chroma_stencil_weights() {
    let gpu = Gpu::test_executor();
    let pixels: Vec<_> = (0..64)
        .flat_map(|i| [1000. + (i % 2) as f32 * 32., 1000., 1000.])
        .collect();
    let input = gpu.upload("Extreme-scale chroma codes", &pixels).unwrap();
    for support in [Support::Binomial3, Support::Box5] {
        let native = Prepared::new([8, 8], [16.; 3], [0.8; 3], 128, 3, support).unwrap();
        let baseline = apply(
            &gpu,
            &input,
            &native.clone().on_grid([8, 8], 128).unwrap(),
            &mut Profile::default(),
        )
        .unwrap();
        let baseline = gpu.readback(&baseline).unwrap();
        for divisor in [1 << 16, 1 << 30, 1 << 31] {
            let prepared = native.clone().on_grid([8, 8], divisor).unwrap();
            let image = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
            assert_eq!(
                gpu.readback(&image).unwrap(),
                baseline,
                "{support:?} divisor{divisor}"
            );
        }
    }
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
    let gpu = Gpu::test_executor();
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
    let gpu = Gpu::test_executor();
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
    assert!(recursive.on_grid([16, 12], 8).is_ok());
}

#[test]
#[ignore = "requires a GPU"]
fn reduced_chroma_preserves_native_response_on_signed_constants() {
    let gpu = Gpu::test_executor();
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
    let gpu = Gpu::test_executor();
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

#[test]
fn recursive_decay_retains_physical_distances_and_sixteen_sample_units() {
    let native = Prepared::new([128, 96], [16.; 3], [0.8; 3], 128, 3, Support::Recursive).unwrap();
    for divisor in [4, 8] {
        let reduced = native.clone().on_grid([32, 24], divisor).unwrap();
        let native_beta = f64::from(native.params.decay[1]) / 256.;
        let beta = f64::from(reduced.params.decay[1]) / 256.;
        assert!((beta - native_beta.powi(divisor as i32)).abs() < 0.015);
        let group = reduced.params.correction[3];
        assert_eq!(group, 1);
        assert_eq!(16 / (group * group), 16);
        assert_eq!(native.tables, reduced.tables);
        if divisor == 4 {
            assert_eq!(native.params.coarse_decay, reduced.params.coarse_decay);
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn reduced_recursive_chroma_keeps_neutral_constants_and_channel_symmetry() {
    let gpu = Gpu::test_executor();
    let native = Prepared::new([136, 88], [16.; 3], [0.8; 3], 128, 3, Support::Recursive).unwrap();
    for divisor in [4, 8] {
        let prepared = native.clone().on_grid([17, 11], divisor).unwrap();
        for code in [-100_f32, 0., 512., 1024.] {
            let pixels = vec![code; 17 * 11 * 3];
            let input = gpu.upload("Reduced recursive neutral", &pixels).unwrap();
            let output = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
            let bytes = gpu.readback(&output).unwrap();
            for (index, rgb) in bytemuck::cast_slice::<u8, f32>(&bytes)
                .as_chunks::<3>()
                .0
                .iter()
                .enumerate()
            {
                assert_eq!(rgb[0], rgb[1]);
                assert_eq!(rgb[1], rgb[2]);
                if code < 0. {
                    // Negative brightness selects a zero normalization ratio.
                    // The unchanged lookup and shared-channel units determine
                    // this response independently of the spatial recurrence.
                    let correction = prepared.tables[(-code) as usize];
                    let shared = (3 * correction) >> prepared.params.correction[1];
                    assert_eq!(rgb[0], code + (correction - shared) as f32);
                } else if code == 0. {
                    assert_eq!(rgb, &[0.; 3]);
                } else {
                    // The original IIR starts from zero at its edges. Check a
                    // settled flat interior separately from those transients.
                    let interior =
                        (3..14).contains(&(index % 17)) && (3..8).contains(&(index / 17));
                    let lookup_limit = prepared.tables.iter().map(|v| v.abs()).max().unwrap();
                    let limit = if interior {
                        2
                    } else {
                        lookup_limit + ((3 * lookup_limit) >> prepared.params.correction[1])
                    };
                    assert!(
                        (rgb[0] - code).abs() <= limit as f32,
                        "divisor {divisor} code {code} pixel {index}: {rgb:?}"
                    );
                }
            }
        }
        let pixels: Vec<f32> = (0..17 * 11)
            .flat_map(|i| {
                if i % 17 < 8 {
                    [960., 1024., 1088.]
                } else {
                    [1088., 1024., 960.]
                }
            })
            .collect();
        let swapped: Vec<f32> = pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|rgb| [rgb[2], rgb[1], rgb[0]])
            .collect();
        let filter = |pixels: &[f32]| {
            let input = gpu
                .upload("Reduced recursive chromatic edge", pixels)
                .unwrap();
            let output = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
            let bytes = gpu.readback(&output).unwrap();
            bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
        };
        let original = filter(&pixels);
        let permuted = filter(&swapped);
        for (a, b) in original
            .as_chunks::<3>()
            .0
            .iter()
            .zip(permuted.as_chunks::<3>().0.iter())
        {
            assert_eq!(a, &[b[2], b[1], b[0]]);
        }
    }
}
