//! Checks reference photographic phase tables, fixed crop mapping, GPU borders, and immutable RGB.

use super::*;

fn params(source: [u32; 2], crop: [u32; 4], rotation: u32, output: [u32; 2]) -> EncodeParams {
    EncodeParams {
        source: [source[0], source[1], rotation, 0],
        crop,
        output: [output[0], output[1], 1, 0],
    }
}

#[test]
fn photographic_kernels_retain_reference_radial_and_axis_rounding() {
    // Frozen reference implementation samples. Indices span both phases and taps.
    let positions = [0, 5, 10, 15, 2048, 4095];
    for (scales, expected) in [
        (
            [1., 1.],
            [
                0xbd711919, 0x3f61b669, 0xbd5bf477, 0, 0xbcc21847, 0xbd711919,
            ],
        ),
        (
            [0.5, 1.],
            [
                0x3b832830, 0x3ee49328, 0x3c6ca0de, 0xb7c64206, 0xbc30ad94, 0x3b832830,
            ],
        ),
    ] {
        let values = coefficients(scales[0], scales[1]).unwrap();
        assert_eq!(positions.map(|i| values[i].to_bits()), expected);
    }
    for scales in [[0.001, 0.002], [0.9, 1.], [1.1, 1.], [3., 3.]] {
        let values = coefficients(scales[0], scales[1]).unwrap();
        assert!(values.as_chunks::<16>().0.iter().all(|row| {
            row.iter().all(|v| v.is_finite()) && (row.iter().sum::<f32>() - 1.).abs() < 0.000001
        }));
    }
    assert!(coefficients(0., 1.).is_err());
    assert!(coefficients(f64::NAN, 1.).is_err());
}

#[test]
fn crop_mapping_preserves_native_bypass_fixed_steps_and_rotations() {
    for (rotation, natural, origin, increments) in [
        (0, [6, 5], [229376, 229376], [32768, 0, 0, 32768]),
        (90, [5, 6], [229376, 425984], [0, -26214, 40960, 0]),
        (180, [6, 5], [491520, 425984], [-32768, 0, 0, -32768]),
        (270, [5, 6], [491520, 229376], [0, 26214, -40960, 0]),
    ] {
        let p = params([12, 10], [3, 3, 9, 8], rotation, natural);
        assert!(Prepared::new(&p).unwrap().is_none());
        let p = params([12, 10], [3, 3, 9, 8], rotation, [10, 8]);
        let prepared = Prepared::new(&p).unwrap().unwrap();
        assert_eq!(prepared.mapping.origin[..2], origin);
        assert_eq!(prepared.mapping.increments, increments);
        assert_eq!(prepared.mapping.origin[2], 0);
        let unit = if rotation % 180 == 0 { [5, 4] } else { [4, 5] };
        let direct = Prepared::new(&params([12, 10], [3, 3, 9, 8], rotation, unit))
            .unwrap()
            .unwrap();
        assert_eq!(direct.mapping.origin[2], 1);
    }
    for p in [
        params([0, 4], [0, 0, 1, 4], 0, [2, 2]),
        params([4, 4], [0, 0, 5, 4], 0, [2, 2]),
        params([4, 4], [2, 0, 2, 4], 0, [2, 2]),
        params([4, 4], [0, 0, 4, 4], 45, [2, 2]),
        params([4, 4], [0, 0, 4, 4], 0, [0, 2]),
        params([32768, 4], [0, 0, 32768, 4], 0, [2, 2]),
    ] {
        assert!(Prepared::new(&p).is_err());
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn photographic_gpu_resize_matches_frozen_reference_float_samples() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/data/resize-reference.json")).unwrap();
    let floats = |value: &serde_json::Value| -> Vec<f32> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect()
    };
    let gpu = Gpu::new().unwrap();
    let pixels = floats(&fixture["input"]);
    let input = gpu
        .upload("Frozen reference photographic RGB", &pixels)
        .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let target = std::array::from_fn(|i| case["target"][i].as_u64().unwrap() as u32);
        let p = params(
            [12, 10],
            [3, 3, 9, 8],
            case["rotation"].as_u64().unwrap() as u32,
            target,
        );
        let (output, _) = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let expected = floats(&case["pixels"]);
        assert_eq!(actual.len(), expected.len());
        let max = actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        assert!(
            max < 0.000002,
            "rotation {} target {target:?}: {max}",
            p.source[2]
        );
    }
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&pixels)
    );
    let constant = [-10_f32, 0.5, 40000.].repeat(25);
    let input = gpu.upload("Signed resize headroom", &constant).unwrap();
    let (output, _) = apply(
        &gpu,
        &input,
        &params([5, 5], [0, 0, 5, 5], 0, [3, 7]),
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&output).unwrap();
    for rgb in bytemuck::cast_slice::<u8, f32>(&bytes).as_chunks::<3>().0 {
        for (sample, expected) in rgb.iter().zip([-10_f32, 0.5, 40000.]) {
            assert!((sample - expected).abs() < 0.02);
        }
    }
}

// A scalar test oracle only. Production samples are always processed on GPU.
fn expected(pixels: &[f32], params: &EncodeParams) -> Vec<f32> {
    let [w, h, rotation, _] = params.source;
    let [left, top, right, bottom] = params.crop;
    let [ow, oh, _, _] = params.output;
    let crop = [right - left, bottom - top];
    let natural = if rotation % 180 == 0 {
        crop
    } else {
        [crop[1], crop[0]]
    };
    let logical = if rotation % 180 == 0 {
        [ow, oh]
    } else {
        [oh, ow]
    };
    let scale = std::array::from_fn::<_, 2, _>(|i| {
        f64::from(logical[i]) / f64::from(crop[i].saturating_sub(1).max(1))
    });
    let table = coefficients(scale[0], scale[1]).unwrap();
    let mut result = Vec::new();
    for y in 0..oh {
        for x in 0..ow {
            if natural == [ow, oh] {
                let point = match rotation {
                    0 => [left + x, top + y],
                    90 => [left + y, bottom - 1 - x],
                    180 => [right - 1 - x, bottom - 1 - y],
                    270 => [right - 1 - y, top + x],
                    _ => unreachable!(),
                };
                let index = ((point[1] * w + point[0]) * 3) as usize;
                result.extend_from_slice(&pixels[index..index + 3]);
                continue;
            }
            let steps = std::array::from_fn::<_, 2, _>(|i| {
                if crop[i] == 1 {
                    0
                } else {
                    (65536. / scale[i]) as i64
                }
            });
            let start = |n: u32| i64::from(n) * 65536 + 32768;
            let end = |n: u32, size: u32| start(n - 1) - if size == 1 { 0 } else { 65536 };
            let point = match rotation {
                0 => [
                    start(left) + i64::from(x) * steps[0],
                    start(top) + i64::from(y) * steps[1],
                ],
                90 => [
                    start(left) + i64::from(y) * steps[0],
                    end(bottom, crop[1]) - i64::from(x) * steps[1],
                ],
                180 => [
                    end(right, crop[0]) - i64::from(x) * steps[0],
                    end(bottom, crop[1]) - i64::from(y) * steps[1],
                ],
                270 => [
                    end(right, crop[0]) - i64::from(y) * steps[0],
                    start(top) + i64::from(x) * steps[1],
                ],
                _ => unreachable!(),
            };
            let mut rgb = [0_f32; 3];
            if scale == [1., 1.] {
                let index = (((point[1] >> 16) as u32 * w + (point[0] >> 16) as u32) * 3) as usize;
                rgb.copy_from_slice(&pixels[index..index + 3]);
            } else {
                let phase = (((point[1] >> 12) & 15) * 16 + ((point[0] >> 12) & 15)) as usize * 16;
                for row in 0..4 {
                    for column in 0..4 {
                        let sx = ((point[0] >> 16) + column - 1).clamp(0, i64::from(w - 1));
                        let sy = ((point[1] >> 16) + row - 1).clamp(0, i64::from(h - 1));
                        let index = (sy as usize * w as usize + sx as usize) * 3;
                        let weight = table[phase + (row * 4 + column) as usize];
                        for c in 0..3 {
                            rgb[c] = weight.mul_add(pixels[index + c], rgb[c]);
                        }
                    }
                }
            }
            result.extend(rgb);
        }
    }
    result
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn photographic_gpu_resize_matches_scalar_crops_rotations_edges_and_singletons() {
    let gpu = Gpu::new().unwrap();
    for (size, crop) in [
        ([12, 10], [3, 3, 9, 8]),
        ([4, 3], [0, 0, 4, 3]),
        ([1, 4], [0, 0, 1, 4]),
        ([5, 1], [0, 0, 5, 1]),
        ([1, 1], [0, 0, 1, 1]),
    ] {
        let pixels: Vec<f32> = (0..size[0] * size[1])
            .flat_map(|i| {
                let v = i as f32;
                [
                    v * 0.03125 - 0.5,
                    if i % 2 == 0 { 1.5 } else { -0.25 },
                    v.sin(),
                ]
            })
            .collect();
        let input = gpu.upload("Photographic resize fixture", &pixels).unwrap();
        for rotation in [0, 90, 180, 270] {
            let natural: [u32; 2] = if rotation % 180 == 0 {
                [crop[2] - crop[0], crop[3] - crop[1]]
            } else {
                [crop[3] - crop[1], crop[2] - crop[0]]
            };
            for target in [
                natural,
                [3, 2],
                [10, 8],
                [2, 11],
                [1, 1],
                [
                    natural[0].saturating_sub(1).max(1),
                    natural[1].saturating_sub(1).max(1),
                ],
            ] {
                let p = params(size, crop, rotation, target);
                let before = gpu.activity();
                let (output, remapped) = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
                let activity = gpu.activity().since(before);
                assert_eq!(activity.readbacks, 0);
                assert_eq!(remapped.output, p.output);
                let bytes = gpu.readback(&output).unwrap();
                if natural == target {
                    assert_eq!(activity.dispatches, 0);
                    assert_eq!(bytes, bytemuck::cast_slice::<f32, u8>(&pixels));
                    assert_eq!(remapped.crop, crop);
                } else {
                    assert_eq!(activity.dispatches, 1);
                    assert_eq!(remapped.source, [target[0], target[1], 0, 0]);
                    assert_eq!(remapped.crop, [0, 0, target[0], target[1]]);
                    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
                    let expected = expected(&pixels, &p);
                    let max = actual
                        .iter()
                        .zip(expected)
                        .map(|(a, b)| (a - b).abs())
                        .fold(0_f32, f32::max);
                    assert!(max < 0.000002, "source {size:?} crop {crop:?} rotation {rotation} target {target:?}: {max}");
                }
                assert_eq!(
                    gpu.readback(&input).unwrap(),
                    bytemuck::cast_slice::<f32, u8>(&pixels)
                );
            }
        }
    }
}
