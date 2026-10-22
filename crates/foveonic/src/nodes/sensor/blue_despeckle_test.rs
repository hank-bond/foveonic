//! Checks immutable blue despeckle stages against independent native fixtures.
use super::*;
// The camera noise fit is independent of mathematical LOG10_E.
#[allow(clippy::approx_constant)]
const BLUE_NOISE: [f32; 3] = [20.843, 0.4342, 8e-7];
#[test]
fn rejects_invalid_geometry_and_noise() {
    assert!(Prepared::new(
        [19, 17],
        [1.; 3],
        1.,
        10.,
        [3, 3, 16, 14],
        Stage::General,
        MissingContext::Reject
    )
    .is_ok());
    assert!(Prepared::new(
        [19, 17],
        [1.; 3],
        1.,
        10.,
        [3, 3, 16, 14],
        Stage::DarkPairs,
        MissingContext::Reject
    )
    .is_err());
    assert!(Prepared::new(
        [19, 18],
        [f32::NAN; 3],
        1.,
        10.,
        [3, 3, 16, 14],
        Stage::DarkPairs,
        MissingContext::Reject
    )
    .is_err());
}
#[test]
#[ignore = "requires a GPU and BLUE_DESPECKLE_REFERENCE"]
fn complete_native_wrappers_match_immutable_gpu_filters() {
    let dir = std::path::PathBuf::from(std::env::var("BLUE_DESPECKLE_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let extent = std::array::from_fn(|k| case["extent"][k].as_u64().unwrap() as u32);
        let noise = std::array::from_fn(|k| case["noise"][k].as_f64().unwrap() as f32);
        let bounds = std::array::from_fn(|k| case["rectangle"][k].as_u64().unwrap() as u32 * 2);
        let stage = if case["stage"] == "dark" {
            Stage::DarkPairs
        } else {
            Stage::General
        };
        let p = Prepared::new(
            extent,
            noise,
            case["threshold"].as_f64().unwrap() as f32,
            case["minimum"].as_f64().unwrap() as f32,
            bounds,
            stage,
            MissingContext::Reject,
        )
        .unwrap();
        let input = std::fs::read(dir.join(format!("{name}-input.bin"))).unwrap();
        let expected = std::fs::read(dir.join(format!("{name}-output.bin"))).unwrap();
        let source = gpu
            .upload(
                "Blue speckle fixture",
                bytemuck::cast_slice::<u8, f32>(&input),
            )
            .unwrap();
        let output = apply(&gpu, &source, &p, &mut Profile::default()).unwrap();
        let actual = gpu.readback(&output).unwrap();
        assert_eq!(actual.len(), expected.len());
        let mut examples = Vec::new();
        for (i, (a, b)) in actual
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0)
            .enumerate()
        {
            if a != b && examples.len() < 8 {
                examples.push((i, f32::from_le_bytes(*a), f32::from_le_bytes(*b)));
            }
        }
        assert!(examples.is_empty(), "{name} {examples:?}");
        assert_eq!(gpu.readback(&source).unwrap(), input);
    }
}
#[test]
#[ignore = "requires a GPU and LUMA_NATIVE_BOUNDARIES"]
fn original_wrappers_match_captured_boundaries() {
    let dir = std::path::PathBuf::from(std::env::var("LUMA_NATIVE_BOUNDARIES").unwrap());
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("luma-boundaries.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for (label, stage) in [
        ("dark-despeckle", Stage::DarkPairs),
        ("despeckle", Stage::General),
    ] {
        let case = &data[label][0];
        if case.is_null() {
            eprintln!(
                "missing {label}, keys {:?}",
                data.as_object().unwrap().keys().collect::<Vec<_>>()
            );
            continue;
        }
        let extent = std::array::from_fn(|k| case["input"]["extent"][k].as_u64().unwrap() as u32);
        let bounds = if matches!(stage, Stage::General) {
            std::array::from_fn(|k| case["rectangle"][k].as_u64().unwrap() as u32 * 2)
        } else {
            [0, 0, extent[0] - 1, extent[1] - 1]
        };
        let p = Prepared::new(
            extent,
            BLUE_NOISE,
            1.,
            10.,
            bounds,
            stage,
            MissingContext::Reject,
        )
        .unwrap();
        let bytes = std::fs::read(dir.join(case["input"]["file"].as_str().unwrap())).unwrap();
        let input = gpu
            .upload(
                "Original blue speckle boundary",
                bytemuck::cast_slice::<u8, f32>(&bytes),
            )
            .unwrap();
        let mut profile = Profile::new(crate::profile::Mode::Gpu);
        let result = apply(&gpu, &input, &p, &mut profile).unwrap();
        let actual = gpu.readback(&result).unwrap();
        let expected = std::fs::read(dir.join(case["output"]["file"].as_str().unwrap())).unwrap();
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
                if examples.len() < 8 {
                    examples.push((i, a, b));
                }
            }
        }
        eprintln!(
            "{label}: {count} differences max{maximum}, {examples:?}, {:?}",
            profile.gpu
        );
        assert_eq!(count, 0, "{label}");
    }
}

#[test]
#[ignore = "requires GPU"]
fn physical_bottom_pairs_preserve_pixels_without_inventing_tail_context() {
    let extent = [24_u32, 20_u32];
    let mut pixels = vec![100_f32; (extent[0] * extent[1]) as usize];
    let x = 10;
    let y = extent[1] - 6;
    pixels[(y * extent[0] + x) as usize] = 0.;
    pixels[((y + 1) * extent[0] + x) as usize] = 0.;
    let gpu = Gpu::new().unwrap();
    let source = gpu
        .upload("Dark pair at physical bottom context", &pixels)
        .unwrap();
    let prepared = |policy| {
        Prepared::new(
            extent,
            [1., 0., 0.],
            1.,
            10.,
            [0, 0, extent[0] - 1, extent[1] - 1],
            Stage::DarkPairs,
            policy,
        )
        .unwrap()
    };
    let keep = prepared(MissingContext::Preserve);
    let reject = prepared(MissingContext::Reject);
    assert!(keep.key() != reject.key());
    let output = apply(&gpu, &source, &keep, &mut Default::default()).unwrap();
    let actual = gpu.readback(&output).unwrap();
    assert_eq!(bytemuck::cast_slice::<u8, f32>(&actual), pixels);
    assert!(apply(&gpu, &source, &reject, &mut Default::default()).is_err());
    assert_eq!(
        gpu.readback(&source).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&pixels)
    );
}

#[test]
#[ignore = "requires a GPU and MONO_BLUE_BOUNDARY parent directory"]
fn original_neighbor_range_matches_monochrome_boundary() {
    let dir = std::path::PathBuf::from(std::env::var("MONO_BLUE_BOUNDARY").unwrap());
    let original = std::fs::read(dir.join("monochrome-stage1/input-rgb.f32")).unwrap();
    let expected = std::fs::read(dir.join("monochrome-images-full/csr-input.f32")).unwrap();
    let blue = |bytes: &[u8]| -> Vec<f32> {
        bytes
            .as_chunks::<12>()
            .0
            .iter()
            .map(|p| f32::from_le_bytes(p[8..12].try_into().unwrap()))
            .collect()
    };
    let source = blue(&original);
    let expected = blue(&expected);
    assert_eq!(source.len(), 2944 * 1888);
    let prepared = Prepared::new(
        [2944, 1888],
        BLUE_NOISE,
        1.,
        0.,
        [0, 0, 2943, 1887],
        Stage::NeighborRange,
        MissingContext::Preserve,
    )
    .unwrap();
    let gpu = Gpu::new().unwrap();
    let input = gpu.upload("Original monochrome blue", &source).unwrap();
    let result = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(&result).unwrap();
    let mut changed = 0;
    for (i, (bytes, expected)) in bytes.as_chunks::<4>().0.iter().zip(expected).enumerate() {
        let actual = f32::from_le_bytes(*bytes).max(0.);
        assert_eq!(actual, expected, "blue pixel {i}");
        changed += usize::from(actual != source[i].max(0.));
    }
    eprintln!("Neighbor-range blue repairs: {changed}");
    assert!(changed > 0);
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&source)
    );
}

#[test]
#[ignore = "requires a GPU and NEIGHBOR_RANGE_REFERENCE native fixture directory"]
fn neighbor_range_matches_bounded_native_workers() {
    let root = std::path::PathBuf::from(std::env::var("NEIGHBOR_RANGE_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let extent = std::array::from_fn(|i| case["extent"][i].as_u64().unwrap() as u32);
        let original = std::fs::read(root.join(case["input"].as_str().unwrap())).unwrap();
        let expected = std::fs::read(root.join(case["output"].as_str().unwrap())).unwrap();
        let p = Prepared::new(
            extent,
            BLUE_NOISE,
            1.,
            0.,
            [0, 0, extent[0] - 1, extent[1] - 1],
            Stage::NeighborRange,
            MissingContext::Preserve,
        )
        .unwrap();
        let input = gpu
            .upload(
                "Bounded blue excursions",
                bytemuck::cast_slice::<u8, f32>(&original),
            )
            .unwrap();
        let output = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
        assert_eq!(gpu.readback(&output).unwrap(), expected);
        assert_eq!(gpu.readback(&input).unwrap(), original);
    }
}
