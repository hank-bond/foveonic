//! Checks row confidence, reflected boundaries and immutable samples against independent equations.

use super::*;

type Vector = [f64; 3];
const FINE_SIGMA: f64 = 1.25;
const SIGNAL_SIGMA: f64 = 3.;
const CONFIDENCE_SIGMA: f64 = 8.;
const GUIDE_SIGMA: f64 = 4.;

fn reflect(value: isize, extent: usize) -> usize {
    let period = 2 * extent as isize;
    let folded = value.rem_euclid(period);
    folded.min(period - 1 - folded) as usize
}

fn kernel(sigma: f64) -> Vec<f64> {
    let radius = (sigma * 4. + 0.5) as isize;
    let mut values: Vec<f64> = (-radius..=radius)
        .map(|v| (-0.5 * (v as f64 / sigma).powi(2)).exp())
        .collect();
    let sum: f64 = values.iter().sum();
    values.iter_mut().for_each(|v| *v /= sum);
    values
}

fn blur(values: &[Vector], width: usize, height: usize, axis: usize, sigma: f64) -> Vec<Vector> {
    let weights = kernel(sigma);
    let radius = weights.len() as isize / 2;
    let mut result = vec![[0.; 3]; values.len()];
    for y in 0..height {
        for x in 0..width {
            for (i, weight) in weights.iter().enumerate() {
                let d = i as isize - radius;
                let sx = if axis == 1 {
                    reflect(x as isize + d, width)
                } else {
                    x
                };
                let sy = if axis == 0 {
                    reflect(y as isize + d, height)
                } else {
                    y
                };
                for c in 0..3 {
                    result[y * width + x][c] += values[sy * width + sx][c] * weight;
                }
            }
        }
    }
    result
}

fn median(mut values: [f64; 12]) -> f64 {
    values.sort_by(f64::total_cmp);
    (values[5] + values[6]) * 0.5
}

fn variance(value: Vector, noise: [f64; 9]) -> Vector {
    std::array::from_fn(|c| {
        let signal = value[c].max(0.);
        noise[c] + noise[3 + c] * signal + noise[6 + c] * signal * signal
    })
}

struct Expected {
    output: Vec<[f32; 3]>,
    rows: Vec<Vector>,
    gate: Vec<f64>,
}

fn oracle(
    original: &[[f32; 3]],
    input: &[[f32; 3]],
    black: [f32; 3],
    prepared: &Prepared,
) -> Expected {
    let [left, top, right, bottom] = prepared.params.area.map(|v| v as usize);
    let stride = prepared.extent()[0] as usize;
    let width = right - left;
    let height = bottom - top;
    let noise: [f64; 9] = std::array::from_fn(|i| prepared.params.noise[i / 3][i % 3] as f64);
    let image: Vec<Vector> = (top..bottom)
        .flat_map(|y| {
            (left..right).map(move |x| {
                std::array::from_fn(|c| (original[y * stride + x][c] - black[c]) as f64)
            })
        })
        .collect();
    let smooth = blur(&image, width, height, 0, SIGNAL_SIGMA);
    let fine = blur(&image, width, height, 0, FINE_SIGMA);
    let variances: Vec<Vector> = smooth
        .into_iter()
        .map(|v| variance(v, noise).map(|x| x.max(1e-5)))
        .collect();
    let mut spans = vec![vec![[0.; 3]; height]; SPANS as usize];
    let mut span_variance = spans.clone();
    let mut reference = vec![[0.; 3]; height];
    for y in 0..height {
        for x in 0..width {
            let i = (y + top) * stride + x + left;
            for c in 0..3 {
                reference[y][c] += f64::from(original[i][c] - input[i][c]) / width as f64;
            }
        }
        for span in 0..SPANS as usize {
            let base = width / SPANS as usize;
            let remainder = width % SPANS as usize;
            let start = span * base + span.min(remainder);
            let end = start + base + usize::from(span < remainder);
            let mut count: f64 = 0.;
            for x in start..end {
                let i = y * width + x;
                if image[i].iter().any(|v| *v >= 3800. || *v <= -50.) {
                    continue;
                }
                count += 1.;
                for c in 0..3 {
                    let limit = 2.5 * variances[i][c].sqrt();
                    spans[span][y][c] += (image[i][c] - fine[i][c]).clamp(-limit, limit);
                    span_variance[span][y][c] += variances[i][c];
                }
            }
            for c in 0..3 {
                spans[span][y][c] /= count.max(1.);
                span_variance[span][y][c] /= count.max(1.).powi(2);
            }
        }
    }
    let mut center = vec![[0.; 3]; height];
    let mut error = center.clone();
    for y in 0..height {
        for c in 0..3 {
            let b: [f64; 12] = std::array::from_fn(|s| spans[s][y][c]);
            let m = median(b);
            let scatter = 1.4826 * median(b.map(|v| (v - m).abs()));
            let mut value = m;
            for _ in 0..2 {
                let weights =
                    b.map(|v| (1.5 * scatter.max(0.05) / (v - value).abs().max(1e-6)).min(1.));
                value = b.iter().zip(weights).map(|(v, w)| v * w).sum::<f64>()
                    / weights.iter().sum::<f64>();
            }
            center[y][c] = value;
            error[y][c] = (scatter * scatter / f64::from(SPANS)).max(1e-6);
        }
    }
    let square: Vec<Vector> = center.iter().map(|v| v.map(|x| x * x)).collect();
    let power = blur(&square, 1, height, 0, CONFIDENCE_SIGMA);
    let uncertainty = blur(&error, 1, height, 0, CONFIDENCE_SIGMA);
    let mut rows: Vec<Vector> = (0..height)
        .map(|y| {
            std::array::from_fn(|c| {
                center[y][c] * (1. - uncertainty[y][c] / power[y][c].max(1e-6)).clamp(0., 1.)
            })
        })
        .collect();
    let low = blur(&rows, 1, height, 0, CONFIDENCE_SIGMA);
    for y in 0..height {
        for c in 0..3 {
            rows[y][c] -= low[y][c];
        }
    }
    let mean: Vector =
        std::array::from_fn(|c| rows.iter().map(|v| v[c]).sum::<f64>() / height as f64);
    let reference_low = blur(&reference, 1, height, 0, FINE_SIGMA);
    for y in 0..height {
        for c in 0..3 {
            rows[y][c] -= mean[c] + reference[y][c] - reference_low[y][c];
        }
        let mahalanobis: f64 = (0..3)
            .map(|c| {
                rows[y][c]
                    * (0..3)
                        .map(|d| rows[y][d] * f64::from(prepared.params.inverse_covariance[c][d]))
                        .sum::<f64>()
            })
            .sum();
        let mut distances: [f64; 12] = std::array::from_fn(|s| {
            (0..3)
                .map(|c| (spans[s][y][c] - center[y][c]).powi(2) / span_variance[s][y][c].max(1e-6))
                .sum()
        });
        distances.sort_by(f64::total_cmp);
        let heterogeneity = (distances[..9].iter().sum::<f64>() / 9. / 4. - 1.).max(0.);
        let confidence =
            1. / ((1. + (mahalanobis / 4.).powi(2)) * (1. + heterogeneity * heterogeneity));
        rows[y].iter_mut().for_each(|v| *v *= confidence);
    }
    let mean: Vector =
        std::array::from_fn(|c| rows.iter().map(|v| v[c]).sum::<f64>() / height as f64);
    for row in &mut rows {
        for c in 0..3 {
            row[c] -= mean[c];
        }
    }
    let guide = blur(
        &blur(&image, width, height, 1, GUIDE_SIGMA),
        width,
        height,
        0,
        GUIDE_SIGMA,
    );
    let gate: Vec<f64> = guide
        .iter()
        .map(|v| {
            let snr = v.iter().sum::<f64>().max(0.)
                / variance(*v, noise).iter().sum::<f64>().max(1e-6).sqrt();
            let t = ((12. - snr) / 8.).clamp(0., 1.);
            t * t * (3. - 2. * t)
        })
        .collect();
    let mut output = original.to_vec();
    for y in 0..height {
        for x in 0..width {
            let i = (y + top) * stride + x + left;
            if original[i].iter().any(|v| *v >= 3900.) {
                continue;
            }
            output[i] = std::array::from_fn(|c| {
                (f64::from(input[i][c]) - rows[y][c] * gate[y * width + x]) as f32
            });
        }
    }
    Expected { output, rows, gate }
}

#[test]
fn row_guard_shader_validates_without_a_device() {
    let module = wgpu::naga::front::wgsl::parse_str(include_str!("row_guard.wgsl")).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn row_guard_metadata_and_reflection_are_bounded() {
    assert!(Prepared::new([190, 170], [3, 2, 187, 168], [1.; 9], 100).is_ok());
    assert!(Prepared::new([190, 170], [3, 2, 187, 168], [f32::NAN; 9], 100).is_err());
    assert!(Prepared::new([190, 170], [3, 2, 191, 168], [1.; 9], 100).is_err());
    assert!(Prepared::new([190, 170], [3, 2, 20, 168], [1.; 9], 100).is_err());
    assert!(Prepared::new([190, 170], [3, 2, 187, 168], [1.; 9], 0).is_err());
    assert_eq!(
        (-3..10).map(|v| reflect(v, 4)).collect::<Vec<_>>(),
        [2, 1, 0, 0, 1, 2, 3, 3, 2, 1, 0, 0, 1]
    );
    let low = Prepared::new([100, 100], [0, 0, 100, 100], [1.; 9], 50).unwrap();
    let high = Prepared::new([100, 100], [0, 0, 100, 100], [1.; 9], 6400).unwrap();
    assert_eq!(
        low.params.inverse_covariance[0][0],
        noise_model::embedded().unwrap().row_inverse_covariance(100)[0]
    );
    assert_eq!(
        high.params.inverse_covariance[0][0],
        noise_model::embedded()
            .unwrap()
            .row_inverse_covariance(1600)[0]
    );
}

struct GuardFixture {
    prepared: Prepared,
    original: Vec<[f32; 3]>,
    input: Vec<[f32; 3]>,
    black: [f32; 3],
}

fn fixture() -> GuardFixture {
    let extent = [199, 173];
    let area = [4, 3, 193, 169];
    let black = [18., 16., 19.];
    let prepared = Prepared::new(
        extent,
        area,
        [4., 9., 0.2, 0.01, 0.02, 0.005, 0., 0., 0.],
        100,
    )
    .unwrap();
    let mut original = Vec::new();
    let mut corrected = Vec::new();
    let mut seed = 1234567u32;
    for y in 0..extent[1] {
        for x in 0..extent[0] {
            let row = 0.3 * (y as f32 * 1.9).sin() + 0.08 * (y as f32 * 2.3).cos();
            let trained = 0.03 * (y as f32 * 1.1).sin();
            let mut value = [0.; 3];
            for c in 0..3 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let jitter = (seed >> 8) as f32 / 16777216. - 0.5;
                value[c] = black[c]
                    + if x > 142 { 80. } else { 3. }
                    + jitter * [1., 1.4, 0.4][c]
                    + row * [1., -0.8, 0.4][c];
                if y == 71 {
                    value[c] += [12., -5., 2.][c];
                }
            }
            let active = x >= area[0] && x < area[2] && y >= area[1] && y < area[3];
            let source = value;
            if active {
                for c in 0..3 {
                    value[c] -= trained * [-0.7, 0.6, 0.1][c];
                }
            }
            original.push(source);
            corrected.push(value);
        }
    }
    let clipped = 30 * extent[0] as usize + 30;
    original[clipped] = [3900., 100., 50.];
    corrected[clipped] = [3890., 99., 51.];
    GuardFixture {
        prepared,
        original,
        input: corrected,
        black,
    }
}

#[test]
fn row_guard_oracle_preserves_bright_detail_and_rejects_strong_color_lines() {
    let GuardFixture {
        prepared,
        original,
        input,
        black,
    } = fixture();
    let expected = oracle(&original, &input, black, &prepared);
    assert!(
        expected.rows.iter().flatten().any(|v| v.abs() > 0.02),
        "fixture must exercise row correction"
    );
    let width = prepared.params.source[2] as usize;
    assert!(expected.gate.iter().step_by(width).any(|v| *v > 0.95));
    assert!(expected
        .gate
        .chunks_exact(width)
        .all(|row| row[width - 1] == 0.));
    let y = 71 - prepared.params.area[1] as usize;
    assert!(
        expected.rows[y].iter().all(|v| v.abs() < 0.01),
        "strong genuine color stripe must not be flattened"
    );
}

#[test]
#[ignore = "requires a GPU"]
fn row_guard_matches_independent_confidence_and_preserves_sensor_contracts() {
    let gpu = Gpu::new().unwrap();
    let GuardFixture {
        prepared,
        original,
        input,
        black,
    } = fixture();
    let expected = oracle(&original, &input, black, &prepared);
    let source = gpu
        .upload("Native row original fixture", &original)
        .unwrap();
    let corrected = gpu.upload("Native row shield fixture", &input).unwrap();
    let pedestal = gpu.upload("Native row black fixture", &black).unwrap();
    let result = apply(
        &gpu,
        &source,
        &corrected,
        &pedestal,
        &prepared,
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&result).unwrap();
    let actual = bytemuck::cast_slice::<u8, [f32; 3]>(&bytes);
    let mut changed = 0;
    let stride = prepared.extent()[0] as usize;
    for (i, value) in actual.iter().enumerate() {
        let x = i % stride;
        let y = i / stride;
        let outside = x < prepared.params.area[0] as usize
            || x >= prepared.params.area[2] as usize
            || y < prepared.params.area[1] as usize
            || y >= prepared.params.area[3] as usize;
        if outside || original[i].iter().any(|v| *v >= 3900.) {
            assert_eq!(*value, original[i]);
        }
        for (c, component) in value.iter().enumerate() {
            assert!(
                (component - expected.output[i][c]).abs() < 0.0015,
                "native {x},{y} layer{c}: {} vs {}",
                component,
                expected.output[i][c]
            );
        }
        changed += usize::from((value[0] - input[i][0]).abs() > 0.02 && !outside);
    }
    assert!(changed > 100, "fixture must exercise meaningful correction");
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_ROW_GUARD_FIXTURE native-layer equations"]
fn row_guard_matches_frozen_native_photo_profiles() {
    let folder = std::path::PathBuf::from(
        std::env::var_os("FOVEONIC_ROW_GUARD_FIXTURE").expect("native row fixture directory"),
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join("metadata.json")).unwrap()).unwrap();
    let extent = std::array::from_fn(|i| metadata["extent"][i].as_u64().unwrap() as u32);
    let area = std::array::from_fn(|i| metadata["area"][i].as_u64().unwrap() as u32);
    let noise = std::array::from_fn(|i| metadata["noise"][i].as_f64().unwrap() as f32);
    let black: [f32; 3] = std::array::from_fn(|i| metadata["black"][i].as_f64().unwrap() as f32);
    let prepared = Prepared::new(
        extent,
        area,
        noise,
        metadata["iso"].as_u64().unwrap() as u32,
    )
    .unwrap();
    let raw = std::fs::read(folder.join("source.u16")).unwrap();
    let original: Vec<[f32; 3]> = raw
        .as_chunks::<6>()
        .0
        .iter()
        .map(|v| std::array::from_fn(|c| f32::from(u16::from_le_bytes([v[c * 2], v[c * 2 + 1]]))))
        .collect();
    assert_eq!(original.len(), extent[0] as usize * extent[1] as usize);
    drop(raw);
    let load = |name: &str| -> Vec<f32> {
        std::fs::read(folder.join(name))
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect()
    };
    let reference = load("reference.f32");
    let expected_rows = load("rows.f32");
    let gate = load("gate.f32");
    let width = (area[2] - area[0]) as usize;
    let height = (area[3] - area[1]) as usize;
    let stride = extent[0] as usize;
    assert_eq!(reference.len(), height * 3);
    assert_eq!(expected_rows.len(), height * 3);
    assert_eq!(gate.len(), width * height);
    let mut input = original.clone();
    for y in 0..height {
        for x in 0..width {
            let i = (y + area[1] as usize) * stride + x + area[0] as usize;
            for c in 0..3 {
                input[i][c] -= reference[y * 3 + c];
            }
        }
    }
    let gpu = Gpu::new().unwrap();
    let source = gpu.upload("Native photo row original", &original).unwrap();
    let corrected = gpu
        .upload("Native photo row shield correction", &input)
        .unwrap();
    let pedestal = gpu.upload("Native photo black levels", &black).unwrap();
    let start = std::time::Instant::now();
    let result = apply(
        &gpu,
        &source,
        &corrected,
        &pedestal,
        &prepared,
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&result).unwrap();
    let elapsed = start.elapsed().as_secs_f64();
    let actual = bytemuck::cast_slice::<u8, [f32; 3]>(&bytes);
    let mut maximum: f64 = 0.;
    let mut squared = 0.;
    let mut measured = 0usize;
    let mut changed = 0usize;
    let mut profile_maximum: f64 = 0.;
    for y in 0..extent[1] as usize {
        let mut estimate = [0.; 3];
        let mut support = 0.;
        for x in 0..stride {
            let i = y * stride + x;
            if x < area[0] as usize
                || x >= area[2] as usize
                || y < area[1] as usize
                || y >= area[3] as usize
                || original[i].iter().any(|v| *v >= 3900.)
            {
                assert_eq!(actual[i], original[i], "immutable native sample {x},{y}");
                continue;
            }
            let local_y = y - area[1] as usize;
            let w = f64::from(gate[local_y * width + x - area[0] as usize]);
            for c in 0..3 {
                let expected =
                    f64::from(input[i][c]) - f64::from(expected_rows[local_y * 3 + c]) * w;
                let error = f64::from(actual[i][c]) - expected;
                maximum = maximum.max(error.abs());
                squared += error * error;
                measured += 1;
                if w > 0.5 {
                    estimate[c] += w * f64::from(input[i][c] - actual[i][c]);
                }
            }
            if w > 0.5 {
                support += w * w;
            }
            changed += usize::from((input[i][0] - actual[i][0]).abs() > 0.0001);
        }
        if support > 10. {
            let local_y = y - area[1] as usize;
            for c in 0..3 {
                profile_maximum = profile_maximum
                    .max((estimate[c] / support - f64::from(expected_rows[local_y * 3 + c])).abs());
            }
        }
    }
    let rms = (squared / measured as f64).sqrt();
    let report = serde_json::json!({"maximum_native_dn":maximum,"rms_native_dn":rms,"maximum_profile_dn":profile_maximum,"changed_pixels":changed,"node_and_readback_seconds":elapsed,"gate":"float32, compared to unquantized equations"});
    println!("{report}");
    std::fs::write(
        folder.join("parity.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(maximum < 0.003, "native layer error {maximum}");
    assert!(
        profile_maximum < 0.0015,
        "row profile error {profile_maximum}"
    );
    assert!(
        changed > 1000,
        "native photo must exercise meaningful correction"
    );
}
