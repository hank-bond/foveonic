//! Checks geometric fitting weights and GPU native patch banks, row+1 bias consumption, and immutable inputs.
use super::*;
use crate::{gpu::Gpu, profile::Profile, source::Matrix};
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../tests/data/banding-reference.json")).unwrap()
}
fn metadata(w: usize, h: usize) -> Metadata {
    let matrix = |values: Vec<f64>| Matrix {
        encoding: MatrixEncoding::Uint32,
        representation: "u32",
        dimensions: vec![values.len()],
        values,
    };
    Metadata {
        processing_version: None,
        width: w,
        height: h,
        rotation: 0,
        properties: Default::default(),
        lists: Default::default(),
        matrices: [
            (
                "ActiveImageArea".into(),
                matrix(vec![8., 8., (w - 9) as f64, (h - 9) as f64]),
            ),
            (
                "KeepImageArea".into(),
                matrix(vec![0., 0., (w - 1) as f64, (h - 1) as f64]),
            ),
        ]
        .into(),
    }
}
#[test]
fn geometric_weights_match_native_svd_for_all_selected_polynomial_bases() {
    let f = fixture();
    for side in [31, 73, 151] {
        let (n, basis, inverse) = matrices(side).unwrap();
        let native = &f["matrices"][side.to_string()];
        assert_eq!(
            n,
            u32::try_from(native["coefficients"].as_u64().unwrap()).unwrap()
        );
        for (index, values) in native["basis"].as_object().unwrap() {
            let i = index.parse::<usize>().unwrap();
            for (k, value) in values.as_array().unwrap().iter().enumerate() {
                assert_eq!(basis[i * n as usize + k], value.as_f64().unwrap() as f32);
            }
        }
        for (index, value) in native["inverse"].as_object().unwrap() {
            let i = index.parse::<usize>().unwrap();
            assert!(
                (inverse[i] - value.as_f64().unwrap() as f32).abs() < 1e-9,
                "side {side} weight {i}: {} != {value}",
                inverse[i]
            );
        }
    }
}
#[test]
fn bank_geometry_and_row_links_preserve_native_side_bank_and_column_order() {
    let p = prepare(&metadata(1600, 640), [4.6, 2.2, 1.6]).unwrap();
    assert_eq!(p.batches.len(), 3);
    assert_eq!(p.patches[0].geometry, [8, 8, 31, 3]);
    assert_eq!(p.patches[1].geometry, [39, 8, 31, 3]);
    assert_eq!(p.rows[0][1], 0);
    assert_eq!(p.rows[8][1], 0);
    assert!(p.rows[9][1] > 0);
    assert!(p.rows.iter().all(
        |range| p.links[range[0] as usize..(range[0] + range[1]) as usize]
            .windows(2)
            .all(|p| p[0] < p[1])
    ));
    assert!(prepare(&metadata(1600, 640), [0., 1., 1.]).is_err());
    assert!(matrices(32).is_err());
}
#[test]
#[ignore = "requires a working GPU adapter"]
fn gpu_row_patch_banks_match_native_biases_counts_and_bounds() {
    let f = fixture();
    let w = f["width"].as_u64().unwrap() as usize;
    let h = f["height"].as_u64().unwrap() as usize;
    let mut state = f["seed"].as_u64().unwrap() as u32;
    let mut source = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                source.push(
                    800. + c as f32 * 100.
                        + x as f32 / 512.
                        + y as f32 / 1024.
                        + (state >> 24) as f32 / 64.
                        + (((y * (c + 1)) % 7) as f32 - 3.) / 16.,
                );
            }
        }
    }
    let gpu = Gpu::new().unwrap();
    let gains = [4.6, 2.2, 1.6];
    {
        let samples = source;
        let input = gpu.upload("Immutable row-banding input", &samples).unwrap();
        let p = prepare(&metadata(w, h), gains).unwrap();
        let result = apply(&gpu, &input, &p, true, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&result.rows).unwrap();
        let actual = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
        let mut expected = vec![[0_f32; 3]; h];
        for y in 0..h {
            let count = f["counts"][y].as_u64().unwrap() as u32;
            assert_eq!(actual[y][3], count as f32, "row {y} contributions");
            if count >= 20 {
                for c in 0..3 {
                    let mean = f["offsets"][y * 3 + c].as_f64().unwrap() as f32 / count as f32;
                    if mean.abs() as f64 <= f64::from(gains[c]) * 0.75 {
                        expected[y][c] = mean;
                    }
                }
            }
            for c in 0..3 {
                assert!(
                    (actual[y][c] - expected[y][c]).abs() < 0.004,
                    "row {y} channel {c}: {} != {}",
                    actual[y][c],
                    expected[y][c]
                );
            }
        }
        let bytes = gpu.readback(result.image.as_ref().unwrap()).unwrap();
        let corrected = bytemuck::cast_slice::<u8, f32>(&bytes);
        for y in 0..h {
            for x in [0, 7, 8, 31, w - 17, w - 16, w - 9, w - 1] {
                let pixel = (y * w + x) * 3;
                for (c, &sample) in samples[pixel..pixel + 3].iter().enumerate() {
                    let i = pixel + c;
                    let bias = if y < h - 16 && x >= 8 && x < w - 16 {
                        expected[y + 1][c]
                    } else {
                        0.
                    };
                    assert!(
                        (corrected[i] - (sample - bias)).abs() < 0.004,
                        "image ({x},{y}) channel {c}"
                    );
                }
            }
        }
        assert!(
            gpu.readback(&input).unwrap() == bytemuck::cast_slice::<f32, u8>(&samples),
            "row estimation mutated its source"
        );
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn parallel_patch_statistics_keep_bright_dark_and_correlated_rejection_decisions() {
    use crate::gpu::Kernel;
    let gpu = Gpu::new().unwrap();
    for side in [31_u32, 73, 151] {
        for level in [500_f32, 700.] {
            for correlation in [0_f32, 0.059, 0.061, 0.099, 0.101, 0.8, 1.] {
                let count = (side * side) as usize;
                let samples: Vec<[f32; 3]> = (0..count)
                    .map(|q| {
                        let red = if q & 1 == 0 { -1. } else { 1. };
                        let independent = if q & 2 == 0 { -1. } else { 1. };
                        let blue = if q & 4 == 0 { -1. } else { 1. };
                        [
                            red,
                            correlation * red
                                + (1. - correlation * correlation).sqrt() * independent,
                            blue,
                        ]
                    })
                    .collect();
                // Independent Float64 statistics are an oracle for the decision,
                // rather than reproducing the shader's compensated lane merges.
                let mean: [f64; 3] = std::array::from_fn(|c| {
                    samples.iter().map(|p| f64::from(p[c])).sum::<f64>() / count as f64
                });
                let mut variance = [0_f64; 3];
                let mut covariance = [0_f64; 3];
                for sample in &samples {
                    let d: [f64; 3] = std::array::from_fn(|c| f64::from(sample[c]) - mean[c]);
                    for c in 0..3 {
                        variance[c] += d[c] * d[c];
                    }
                    covariance[0] += d[0] * d[1];
                    covariance[1] += d[0] * d[2];
                    covariance[2] += d[2] * d[1];
                }
                let maximum = [(0, 1), (0, 2), (2, 1)]
                    .into_iter()
                    .enumerate()
                    .map(|(c, (a, b))| covariance[c].abs() / (variance[a] * variance[b]).sqrt())
                    .fold(0_f64, f64::max);
                let expected = u32::from(maximum < if level > 600. { 0.1 } else { 0.06 });
                let pixels = gpu
                    .upload("Patch brightness", &vec![[level; 3]; count])
                    .unwrap();
                let residuals = gpu.upload("Independent patch residuals", &samples).unwrap();
                let patches = gpu
                    .upload(
                        "One statistics patch",
                        &[Patch {
                            geometry: [0, 0, side, 6],
                            storage: [0; 4],
                        }],
                    )
                    .unwrap();
                let params = gpu.uniform(&Params {
                    source: [side, side, 1, 0],
                    area: [0; 4],
                    values: [0.; 4],
                    gains: [1.; 4],
                });
                let batch = gpu.uniform(&[0_u32, 1, side, 6]);
                let mut accepted = gpu.storage("Patch rejection result", 4).unwrap();
                let mut trace = gpu.trace(false, 1);
                gpu.dispatch(
                    Kernel::BandingQuality,
                    &mut [
                        (0, Read(&pixels)),
                        (1, Read(&patches)),
                        (2, Read(&params)),
                        (5, Read(&residuals)),
                        (6, Write(accepted.write())),
                        (8, Read(&batch)),
                    ],
                    [1, 1],
                    &mut trace,
                );
                let bytes = gpu.readback(&accepted).unwrap();
                assert_eq!(
                    u32::from_le_bytes(bytes[..4].try_into().unwrap()),
                    expected,
                    "side={side} level={level} correlation={correlation}"
                );
            }
        }
    }
}
