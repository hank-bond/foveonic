//! Compares complete sparse repair with independent native wrapper fixtures and masks.

use super::*;

#[test]
fn validates_sparse_context_and_finite_controls() {
    assert!(Prepared::new([64, 20], [1.; 9], [1.; 3], 1., true).is_ok());
    for extent in [[63, 20], [64, 19], [u32::MAX, u32::MAX]] {
        assert!(Prepared::new(extent, [1.; 9], [1.; 3], 1., true).is_err());
    }
    assert!(Prepared::new([64, 20], [f32::NAN; 9], [1.; 3], 1., true).is_err());
    assert!(Prepared::new([64, 20], [1.; 9], [1.; 3], 0., true).is_err());
}

#[test]
#[ignore = "requires a GPU and SPARSE_REPAIR_REFERENCE with native wrapper fixtures"]
fn complete_ordinary_hi_wrapper_matches_native_planes_and_defects() {
    let directory = std::path::PathBuf::from(std::env::var("SPARSE_REPAIR_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let extent = [
            case["extent"][0].as_u64().unwrap() as u32,
            case["extent"][1].as_u64().unwrap() as u32,
        ];
        let read =
            |suffix: &str| std::fs::read(directory.join(format!("{name}-{suffix}.bin"))).unwrap();
        let rgb = read("input-rgb");
        let blue = read("input-blue");
        let rgb = gpu
            .upload(
                "Sparse repair input RGB",
                bytemuck::cast_slice::<u8, f32>(&rgb),
            )
            .unwrap();
        let blue = gpu
            .upload(
                "Sparse repair input blue",
                bytemuck::cast_slice::<u8, f32>(&blue),
            )
            .unwrap();
        let noise_rgb = std::array::from_fn(|i| case["noise_rgb"][i].as_f64().unwrap() as f32);
        let noise_blue = std::array::from_fn(|i| case["noise_blue"][i].as_f64().unwrap() as f32);
        let params = Prepared::new(
            extent,
            noise_rgb,
            noise_blue,
            1.,
            case["preserve"].as_bool().unwrap(),
        )
        .unwrap();
        let result = apply(&gpu, &rgb, &blue, &params, &mut Profile::default()).unwrap();
        for (label, buffer) in [("rgb", result.rgb), ("blue", result.blue)] {
            let expected = read(label);
            let actual = gpu.readback(&buffer).unwrap();
            assert_eq!(actual.len(), expected.len());
            let mut count = 0;
            let mut max = 0_f32;
            let mut examples = Vec::new();
            for (i, (a, b)) in actual
                .as_chunks::<4>()
                .0
                .iter()
                .zip(expected.as_chunks::<4>().0)
                .enumerate()
            {
                if a != b {
                    count += 1;
                    let a = f32::from_le_bytes(*a);
                    let b = f32::from_le_bytes(*b);
                    max = max.max((a - b).abs());
                    if examples.len() < 8 {
                        examples.push((i, a, b));
                    }
                }
            }
            eprintln!("{name} {label} differences {count}, max {max}, examples {examples:?}");
            assert_eq!(count, 0, "{name} {label}");
        }
        let expected = read("defects");
        let actual = gpu.readback(&result.defects).unwrap();
        let actual = bytemuck::cast_slice::<u8, u32>(&actual);
        let actual: Vec<u8> = actual.iter().map(|&v| v as u8).collect();
        assert_eq!(actual, expected, "{name} defects");
    }
}

#[test]
#[ignore = "requires a GPU and SPARSE_REPAIR_ORIGINAL_REFERENCE containing an actual native wrapper boundary"]
fn complete_original_hi_wrapper_matches_native_boundary() {
    let directory =
        std::path::PathBuf::from(std::env::var("SPARSE_REPAIR_ORIGINAL_REFERENCE").unwrap());
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("sparse-boundary.json")).unwrap())
            .unwrap();
    let extent = [
        receipt["input"]["rgb"]["extent"][0].as_u64().unwrap() as u32,
        receipt["input"]["rgb"]["extent"][1].as_u64().unwrap() as u32,
    ];
    let noise_rgb = std::array::from_fn(|i| receipt["rgb_noise"][i].as_f64().unwrap() as f32);
    let noise_blue = std::array::from_fn(|i| receipt["blue_noise"][i].as_f64().unwrap() as f32);
    assert_eq!(receipt["registers"]["x4"], 0);
    assert_eq!(receipt["registers"]["x5"], 1);
    assert_eq!(receipt["registers"]["x6"], 0);
    let gpu = Gpu::new().unwrap();
    let load = |name: &str| {
        let bytes = std::fs::read(directory.join(format!("sparse-input-{name}.f32"))).unwrap();
        gpu.upload(
            "Actual sparse repair boundary",
            bytemuck::cast_slice::<u8, f32>(&bytes),
        )
        .unwrap()
    };
    let rgb = load("rgb");
    let blue = load("blue");
    let params = Prepared::new(extent, noise_rgb, noise_blue, 1., true).unwrap();
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
    let result = apply(&gpu, &rgb, &blue, &params, &mut profile).unwrap();
    eprintln!("sparse original GPU passes {:?}", profile.gpu);
    for (label, buffer) in [("rgb", result.rgb), ("blue", result.blue)] {
        let expected = std::fs::read(directory.join(format!("sparse-output-{label}.f32"))).unwrap();
        let actual = gpu.readback(&buffer).unwrap();
        let mut count = 0;
        let mut maximum = 0_f32;
        let mut examples = Vec::new();
        for (i, (a, b)) in actual
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0)
            .enumerate()
        {
            if a != b {
                count += 1;
                let a = f32::from_le_bytes(*a);
                let b = f32::from_le_bytes(*b);
                maximum = maximum.max((a - b).abs());
                if examples.len() < 12 {
                    examples.push((i, a, b));
                }
            }
        }
        eprintln!("original {label}: {count} different, max {maximum}, examples {examples:?}");
        assert_eq!(count, 0, "original {label}");
    }
}
