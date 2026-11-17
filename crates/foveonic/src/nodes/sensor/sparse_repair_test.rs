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
#[ignore = "requires GPU"]
fn requested_outputs_preserve_complete_repairs_and_avoid_unused_blue_work() {
    let gpu = Gpu::test_executor();
    let extent: [u32; 2] = [256, 128];
    let n = (extent[0] * extent[1]) as usize;
    let noise = [1., 0., 0., 1., 0., 0., 1., 0., 0.];
    for rgb_only in [false, true] {
        for preserve in [false, true] {
            let mut rgb = vec![0_f32; n * 3];
            let blue_extent = if rgb_only {
                extent
            } else {
                extent.map(|v| v * 2)
            };
            let mut blue = vec![0_f32; (blue_extent[0] * blue_extent[1]) as usize];
            for y in 0..extent[1] {
                for x in 0..extent[0] {
                    let hot = x >= 16
                        && x + 1 < extent[0]
                        && y >= 4
                        && x.is_multiple_of(16)
                        && y.is_multiple_of(4);
                    let base = 100. + ((x * 17 + y * 13 + x * y) % 31) as f32;
                    let i = (y * extent[0] + x) as usize;
                    for c in 0..3 {
                        rgb[i * 3 + c] = if hot {
                            15000.
                        } else {
                            base * [1.2, 0.8, 1.][c]
                        };
                    }
                    if rgb_only {
                        blue[i] = base;
                    } else {
                        for phase in 0..4 {
                            let index =
                                ((y * 2 + phase / 2) * blue_extent[0] + x * 2 + phase % 2) as usize;
                            blue[index] = if hot {
                                16000. + phase as f32
                            } else {
                                base * (1. + phase as f32 * 0.1)
                            };
                        }
                    }
                }
            }
            let input_rgb = gpu.upload("Sparse demand fixture RGB", &rgb).unwrap();
            let input_blue = gpu.upload("Sparse demand fixture blue", &blue).unwrap();
            let prepared = Prepared::new(extent, noise, [1., 0., 0.], 1., preserve).unwrap();
            let prepared = if rgb_only {
                prepared.with_rgb_only()
            } else {
                prepared
            };
            let start = gpu.activity();
            let complete = apply(
                &gpu,
                &input_rgb,
                &input_blue,
                &prepared,
                Requested {
                    blue: true,
                    defects: true,
                },
                &mut Profile::default(),
            )
            .unwrap();
            let full_activity = gpu.activity().since(start);
            let expected_rgb = gpu.readback(&complete.rgb).unwrap();
            let expected_blue = gpu.readback(complete.blue.as_ref().unwrap()).unwrap();
            let expected_defects = gpu.readback(complete.defects.as_ref().unwrap()).unwrap();
            assert_eq!(full_activity.dispatches, if rgb_only { 7 } else { 12 });
            let pixels: &[f32] = bytemuck::cast_slice(&expected_rgb);
            assert!(
                (0..n).any(|i| {
                    let x = i as u32 % extent[0];
                    let y = i as u32 / extent[0];
                    !(x >= 16
                        && x + 1 < extent[0]
                        && y >= 4
                        && x.is_multiple_of(16)
                        && y.is_multiple_of(4))
                        && pixels[i * 3..i * 3 + 3] != rgb[i * 3..i * 3 + 3]
                }),
                "fixture must execute interpolation outside the selected sensor sites"
            );
            if !rgb_only {
                assert!(
                    bytemuck::cast_slice::<u8, u32>(&expected_defects)
                        .iter()
                        .any(|v| *v > 7),
                    "fixture must exercise density's exported mask writes"
                );
            }
            for requested in [
                Requested::default(),
                Requested {
                    blue: false,
                    defects: true,
                },
                Requested {
                    blue: true,
                    defects: false,
                },
            ] {
                let start = gpu.activity();
                let output = apply(
                    &gpu,
                    &input_rgb,
                    &input_blue,
                    &prepared,
                    requested,
                    &mut Profile::default(),
                )
                .unwrap();
                let activity = gpu.activity().since(start);
                assert_eq!(
                    activity.dispatches,
                    if rgb_only {
                        7
                    } else if requested.blue {
                        12
                    } else if requested.defects {
                        8
                    } else {
                        7
                    }
                );
                assert_eq!(
                    gpu.readback(&output.rgb).unwrap(),
                    expected_rgb,
                    "RGB mode{rgb_only} preserve{preserve}"
                );
                assert_eq!(output.blue.is_some(), requested.blue);
                assert_eq!(output.defects.is_some(), requested.defects);
                if let Some(blue) = output.blue {
                    assert_eq!(gpu.readback(&blue).unwrap(), expected_blue);
                }
                if let Some(defects) = output.defects {
                    assert_eq!(gpu.readback(&defects).unwrap(), expected_defects);
                }
                if !rgb_only && !requested.blue && !requested.defects {
                    assert!(activity.storage_bytes < full_activity.storage_bytes);
                }
            }
            assert_eq!(
                gpu.readback(&input_rgb).unwrap(),
                bytemuck::cast_slice::<f32, u8>(&rgb)
            );
            assert_eq!(
                gpu.readback(&input_blue).unwrap(),
                bytemuck::cast_slice::<f32, u8>(&blue)
            );
        }
    }
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
        let result = apply(
            &gpu,
            &rgb,
            &blue,
            &params,
            Requested {
                blue: true,
                defects: true,
            },
            &mut Profile::default(),
        )
        .unwrap();
        for (label, buffer) in [("rgb", result.rgb), ("blue", result.blue.unwrap())] {
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
        let actual = gpu.readback(&result.defects.unwrap()).unwrap();
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
    let result = apply(
        &gpu,
        &rgb,
        &blue,
        &params,
        Requested {
            blue: true,
            defects: true,
        },
        &mut profile,
    )
    .unwrap();
    eprintln!("sparse original GPU passes {:?}", profile.gpu);
    for (label, buffer) in [("rgb", result.rgb), ("blue", result.blue.unwrap())] {
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
#[test]
#[ignore = "requires GPU and LOW_REPAIR_REFERENCE with bounded reference implementation RGB fixtures"]
fn rgb_only_sparse_repair_matches_reference_workers() {
    let root = std::path::PathBuf::from(std::env::var_os("LOW_REPAIR_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let gpu = crate::gpu::Gpu::new().unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let extent = [
            case["extent"][0].as_u64().unwrap() as u32,
            case["extent"][1].as_u64().unwrap() as u32,
        ];
        let data = std::fs::read(root.join(format!("{name}-input.f32"))).unwrap();
        let expected = std::fs::read(root.join(format!("{name}-rgb.f32"))).unwrap();
        let input = gpu
            .upload("LOW reference RGB", bytemuck::cast_slice::<u8, f32>(&data))
            .unwrap();
        let blue = gpu
            .upload(
                "LOW reference blue",
                &vec![0_f32; extent[0] as usize * extent[1] as usize],
            )
            .unwrap();
        let prepared = super::Prepared::new(
            extent,
            std::array::from_fn(|i| manifest["noise_rgb"][i].as_f64().unwrap() as f32),
            [1., 0., 0.],
            1.,
            case["preserve"].as_bool().unwrap(),
        )
        .unwrap()
        .with_rgb_only();
        let output = super::apply(
            &gpu,
            &input,
            &blue,
            &prepared,
            Requested {
                blue: true,
                defects: true,
            },
            &mut Default::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output.rgb).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        let reference: &[f32] = bytemuck::cast_slice(&expected);
        let maximum = actual
            .iter()
            .zip(reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        eprintln!("{name} maximum {maximum}");
        assert!(maximum <= 0.001, "{name} differs by {maximum}");
        assert_eq!(gpu.readback(&input).unwrap(), data);
    }
}
