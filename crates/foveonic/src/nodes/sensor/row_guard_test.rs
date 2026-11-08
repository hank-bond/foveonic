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

fn kernel(sigma: f64, radius: usize) -> Vec<f64> {
    let radius = radius as isize;
    let mut values: Vec<f64> = (-radius..=radius)
        .map(|v| (-0.5 * (v as f64 / sigma).powi(2)).exp())
        .collect();
    let sum: f64 = values.iter().sum();
    values.iter_mut().for_each(|v| *v /= sum);
    values
}

fn blur(
    values: &[Vector],
    width: usize,
    height: usize,
    axis: usize,
    sigma: f64,
    radius: usize,
) -> Vec<Vector> {
    let weights = kernel(sigma, radius);
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

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) * 0.5
    } else {
        values[middle]
    }
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
    clipping: Option<&[u32]>,
) -> Expected {
    let [left, top, right, bottom] = prepared.params.area.map(|v| v as usize);
    let stride = prepared.extent()[0] as usize;
    let width = right - left;
    let height = bottom - top;
    let span_count = prepared.params.sampling[1] as usize;
    let divisor = f64::from(prepared.params.sampling[0]);
    let sigmas = [FINE_SIGMA, SIGNAL_SIGMA, CONFIDENCE_SIGMA, GUIDE_SIGMA]
        .map(|sigma| (sigma / divisor).max(0.5));
    let blur_at = |values: &[Vector], w, h, axis, stage: usize| {
        blur(
            values,
            w,
            h,
            axis,
            sigmas[stage],
            prepared.params.radii[stage] as usize,
        )
    };
    let variance_floor = f64::from(prepared.params.limits[0]);
    let error_floor = f64::from(prepared.params.limits[1]);
    let scatter_floor = f64::from(prepared.params.limits[2]);
    let noise: [f64; 9] = std::array::from_fn(|i| prepared.params.noise[i / 3][i % 3] as f64);
    let image: Vec<Vector> = (top..bottom)
        .flat_map(|y| {
            (left..right).map(move |x| {
                std::array::from_fn(|c| (original[y * stride + x][c] - black[c]) as f64)
            })
        })
        .collect();
    let smooth = blur_at(&image, width, height, 0, 1);
    let fine = blur_at(&image, width, height, 0, 0);
    let variances: Vec<Vector> = smooth
        .into_iter()
        .map(|v| variance(v, noise).map(|x| x.max(variance_floor)))
        .collect();
    let mut spans = vec![vec![[0.; 3]; height]; span_count];
    let mut span_variance = spans.clone();
    let mut reference = vec![[0.; 3]; height];
    for y in 0..height {
        for x in 0..width {
            let i = (y + top) * stride + x + left;
            for c in 0..3 {
                reference[y][c] += f64::from(original[i][c] - input[i][c]) / width as f64;
            }
        }
        for span in 0..span_count {
            let base = width / span_count;
            let remainder = width % span_count;
            let start = span * base + span.min(remainder);
            let end = start + base + usize::from(span < remainder);
            let mut count: f64 = 0.;
            for x in start..end {
                let i = y * width + x;
                if image[i].iter().any(|v| *v >= 3800. || *v <= -50.)
                    || clipping.is_some_and(|flags| flags[(y + top) * stride + x + left] != 0)
                {
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
            let b: Vec<f64> = (0..span_count).map(|s| spans[s][y][c]).collect();
            let m = median(b.clone());
            let scatter = 1.4826 * median(b.iter().map(|v| (v - m).abs()).collect());
            let mut value = m;
            for _ in 0..2 {
                let weights = b
                    .iter()
                    .map(|v| {
                        (1.5 * scatter.max(scatter_floor) / (v - value).abs().max(error_floor))
                            .min(1.)
                    })
                    .collect::<Vec<_>>();
                value = b.iter().zip(&weights).map(|(v, w)| v * w).sum::<f64>()
                    / weights.iter().sum::<f64>();
            }
            center[y][c] = value;
            error[y][c] = (scatter * scatter / span_count as f64).max(error_floor);
        }
    }
    let square: Vec<Vector> = center.iter().map(|v| v.map(|x| x * x)).collect();
    let power = blur_at(&square, 1, height, 0, 2);
    let uncertainty = blur_at(&error, 1, height, 0, 2);
    let mut rows: Vec<Vector> = (0..height)
        .map(|y| {
            std::array::from_fn(|c| {
                center[y][c] * (1. - uncertainty[y][c] / power[y][c].max(error_floor)).clamp(0., 1.)
            })
        })
        .collect();
    let low = blur_at(&rows, 1, height, 0, 2);
    for y in 0..height {
        for c in 0..3 {
            rows[y][c] -= low[y][c];
        }
    }
    let mean: Vector =
        std::array::from_fn(|c| rows.iter().map(|v| v[c]).sum::<f64>() / height as f64);
    let reference_low = blur_at(&reference, 1, height, 0, 0);
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
        let mut distances: Vec<f64> = (0..span_count)
            .map(|s| {
                (0..3)
                    .map(|c| {
                        (spans[s][y][c] - center[y][c]).powi(2)
                            / span_variance[s][y][c].max(error_floor)
                    })
                    .sum()
            })
            .collect();
        distances.sort_by(f64::total_cmp);
        let heterogeneity =
            (distances[..span_count.min(9)].iter().sum::<f64>() / span_count.min(9) as f64 / 4.
                - 1.)
                .max(0.);
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
    let guide = blur_at(&blur_at(&image, width, height, 1, 3), width, height, 0, 3);
    let gate: Vec<f64> = guide
        .iter()
        .map(|v| {
            let snr = v.iter().sum::<f64>().max(0.)
                / (variance(*v, noise).iter().sum::<f64>() * divisor * divisor)
                    .max(error_floor * divisor * divisor)
                    .sqrt();
            let t = ((12. - snr) / 8.).clamp(0., 1.);
            t * t * (3. - 2. * t)
        })
        .collect();
    let mut output = original.to_vec();
    for y in 0..height {
        for x in 0..width {
            let i = (y + top) * stride + x + left;
            if clipping.map_or_else(
                || original[i].iter().any(|v| *v >= 3900.),
                |flags| flags[i] != 0,
            ) {
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
    let module = wgpu::naga::front::wgsl::parse_str(concat!(
        include_str!("calibration_common.wgsl"),
        "\n",
        include_str!("row_guard.wgsl")
    ))
    .unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn row_guard_metadata_and_reflection_are_bounded() {
    assert!(Prepared::new(
        [190, 170],
        [3, 2, 187, 168],
        [1.; 9],
        100,
        noise_model::embedded().unwrap()
    )
    .is_ok());
    assert!(Prepared::new(
        [190, 170],
        [3, 2, 187, 168],
        [f32::NAN; 9],
        100,
        noise_model::embedded().unwrap()
    )
    .is_err());
    assert!(Prepared::new(
        [190, 170],
        [3, 2, 191, 168],
        [1.; 9],
        100,
        noise_model::embedded().unwrap()
    )
    .is_err());
    assert!(Prepared::new(
        [190, 170],
        [3, 2, 20, 168],
        [1.; 9],
        100,
        noise_model::embedded().unwrap()
    )
    .is_err());
    assert!(Prepared::new(
        [190, 170],
        [3, 2, 187, 168],
        [1.; 9],
        0,
        noise_model::embedded().unwrap()
    )
    .is_err());
    assert_eq!(
        (-3..10).map(|v| reflect(v, 4)).collect::<Vec<_>>(),
        [2, 1, 0, 0, 1, 2, 3, 3, 2, 1, 0, 0, 1]
    );
    let low = Prepared::new(
        [100, 100],
        [0, 0, 100, 100],
        [1.; 9],
        50,
        noise_model::embedded().unwrap(),
    )
    .unwrap();
    let high = Prepared::new(
        [100, 100],
        [0, 0, 100, 100],
        [1.; 9],
        6400,
        noise_model::embedded().unwrap(),
    )
    .unwrap();
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
        noise_model::embedded().unwrap(),
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
    let expected = oracle(&original, &input, black, &prepared, None);
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
    let expected = oracle(&original, &input, black, &prepared, None);
    let source = gpu
        .upload("Native row original fixture", &original)
        .unwrap();
    let corrected = gpu.upload("Native row shield fixture", &input).unwrap();
    let pedestal = gpu.upload("Native row black fixture", &black).unwrap();
    let result = apply(
        &gpu,
        Inputs::Native {
            original: &source,
            shield_corrected: &corrected,
            black: &pedestal,
        },
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
                (component - expected.output[i][c]).abs() < 0.0001,
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
        noise_model::embedded().unwrap(),
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
        Inputs::Native {
            original: &source,
            shield_corrected: &corrected,
            black: &pedestal,
        },
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

fn calibration(extent: [u32; 2]) -> Arc<Calibration> {
    Arc::new(Calibration {
        params: super::super::calibration::CalibrationParams {
            source: [extent[0], extent[1], 0, 0],
            dark: [0; 4],
            gains: [2., 3., 0.5, 0.],
            shading: [0.; 4],
            shading_geometry: [0., 0., 1., 0.],
            grids: [[0, 2, 2, 0], [4, 2, 2, 0], [8, 2, 2, 0]],
        },
        grid_values: vec![1.; 12],
    })
}

#[test]
fn resized_supports_preserve_distinct_pixel_and_row_noise_units() {
    let native = [1027, 769];
    let base = Prepared::new(
        native,
        [4, 3, 1021, 763],
        [4.; 9],
        100,
        noise_model::embedded().unwrap(),
    )
    .unwrap();
    let native_energy = highpass_energy(&base.weights[..11]);
    for d in [2, 4, 8, 64, 1024, 1 << 31] {
        let grid = WorkingGrid::new(native, crate::render::Scale::new(d).unwrap(), 8)
            .unwrap()
            .with_halo(8)
            .unwrap();
        let p = base.clone().on_grid(grid, calibration(native));
        assert_eq!(p.extent(), grid.extent);
        assert_eq!(p.params.sampling[1], p.params.source[2].min(12));
        assert_eq!(
            p.params.noise[0][0],
            4. / (u64::from(d) * u64::from(d)) as f32
        );
        let radius = p.params.radii[0] as usize;
        let energy = highpass_energy(&p.weights[..2 * radius + 1]);
        let factor = f64::from(d) * native_energy / energy;
        let expected = (f64::from(base.params.inverse_covariance[0][0]) * factor) as f32;
        assert_eq!(p.params.inverse_covariance[0][0], expected);
        for (i, (_, native_radius)) in GAUSSIANS.into_iter().enumerate() {
            assert_eq!(p.params.radii[i], (native_radius as u32).div_ceil(d).max(1));
            let offset = p.params.offsets[i] as usize;
            let count = 2 * p.params.radii[i] as usize + 1;
            assert!((p.weights[offset..offset + count].iter().sum::<f32>() - 1.).abs() < 1e-6);
        }
        assert!(p.key() != base.key());
    }
}

#[test]
#[ignore = "requires a GPU"]
fn resized_rows_match_independent_equations_and_preserve_repairs() {
    let gpu = Gpu::new().unwrap();
    let native = [515, 387];
    let black = [18., 16., 19.];
    for d in [2, 4, 8, 64] {
        let grid = WorkingGrid::new(native, crate::render::Scale::new(d).unwrap(), 8)
            .unwrap()
            .with_halo(8)
            .unwrap();
        let p = Prepared::new(
            native,
            [4, 3, 509, 381],
            [4., 9., 0.2, 0.01, 0.02, 0.005, 0., 0., 0.],
            100,
            noise_model::embedded().unwrap(),
        )
        .unwrap()
        .on_grid(grid, calibration(native));
        let [width, height] = p.extent();
        let mut original = Vec::new();
        let mut shield = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let in_area = x >= p.params.area[0]
                    && x < p.params.area[2]
                    && y >= p.params.area[1]
                    && y < p.params.area[3];
                let row = 0.15 * (y as f32 * 1.9).sin() / (d as f32).sqrt();
                let jitter = (((x * 17 + y * 29) % 31) as f32 / 31. - 0.5) / d as f32;
                let signal = if x > width * 3 / 4 { 80. } else { 3. };
                let value = std::array::from_fn(|c| {
                    black[c] + signal + row * [1., -0.8, 0.4][c] + jitter * [1., 1.4, 0.4][c]
                });
                original.push(value);
                shield.push(std::array::from_fn(|c| {
                    value[c]
                        - if in_area {
                            0.02 * (y as f32 * 1.1).sin() * [-0.7, 0.6, 0.1][c]
                        } else {
                            0.
                        }
                }));
            }
        }
        let gains = [2., 3., 0.5];
        let mut input: Vec<[f32; 3]> = shield
            .iter()
            .map(|v| std::array::from_fn(|c| (v[c] - black[c]) * gains[c]))
            .collect();
        let mut flags = vec![0_u32; input.len()];
        let protected = ((p.params.area[1] + 1) * width + p.params.area[0]) as usize;
        flags[protected] = 1;
        input[protected] = [60., 54., 66.];
        // This non-clipped repair must not alter the original/post-shield evidence.
        let repaired = protected + 1;
        input[repaired][1] += 0.25;
        let expected = oracle(&original, &shield, black, &p, Some(&flags));
        let original_buffer = gpu.upload("Resized row originals", &original).unwrap();
        let shield_buffer = gpu.upload("Resized post-shield evidence", &shield).unwrap();
        let black_buffer = gpu.upload("Native black levels", &black).unwrap();
        let input_buffer = gpu.upload("Repaired calibrated layers", &input).unwrap();
        let clipping = gpu.upload("Original cell clipping", &flags).unwrap();
        let output = apply(
            &gpu,
            Inputs::Calibrated {
                input: &input_buffer,
                original: &original_buffer,
                shield_corrected: &shield_buffer,
                black: &black_buffer,
                clipping: &clipping,
            },
            &p,
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, [f32; 3]>(&bytes);
        let mut changed = 0;
        for (i, value) in actual.iter().enumerate() {
            let x = i as u32 % width;
            let y = i as u32 / width;
            let active = x >= p.params.area[0]
                && x < p.params.area[2]
                && y >= p.params.area[1]
                && y < p.params.area[3];
            for c in 0..3 {
                let target = if active && flags[i] == 0 {
                    input[i][c] + (expected.output[i][c] - shield[i][c]) * gains[c]
                } else {
                    input[i][c]
                };
                assert!(
                    (value[c] - target).abs() < 0.0015,
                    "d{d} pixel{i} c{c}: {} vs {target}",
                    value[c]
                );
            }
            changed += usize::from((value[0] - input[i][0]).abs() > 1e-5);
        }
        assert_eq!(actual[protected], input[protected]);
        assert!(changed > 0, "d{d} must exercise a row correction");
        assert_eq!(
            gpu.readback(&input_buffer).unwrap(),
            bytemuck::cast_slice::<[f32; 3], u8>(&input)
        );
        assert_eq!(
            gpu.readback(&original_buffer).unwrap(),
            bytemuck::cast_slice::<[f32; 3], u8>(&original)
        );
    }
}

#[test]
#[ignore = "requires a GPU"]
fn resized_rows_handle_single_cells_and_short_spans_without_inventing_offsets() {
    let gpu = Gpu::new().unwrap();
    let native = [257, 193];
    for d in [64, 512, 1 << 31] {
        let grid = WorkingGrid::new(native, crate::render::Scale::new(d).unwrap(), 8)
            .unwrap()
            .with_halo(8)
            .unwrap();
        let p = Prepared::new(
            native,
            [0, 0, 257, 193],
            [1.; 9],
            100,
            noise_model::embedded().unwrap(),
        )
        .unwrap()
        .on_grid(grid, calibration(native));
        let count = (grid.extent[0] * grid.extent[1]) as usize;
        let original = gpu
            .upload("Flat row original", &vec![[21_f32, 19., 22.]; count])
            .unwrap();
        let shield = gpu
            .upload(
                "Constant shield offset",
                &vec![[20.99_f32, 18.99, 21.99]; count],
            )
            .unwrap();
        let input = gpu
            .upload(
                "Calibrated constant input",
                &vec![[5.98_f32, 8.97, 1.495]; count],
            )
            .unwrap();
        let black = gpu.upload("Black levels", &[18_f32, 16., 19.]).unwrap();
        let clipping = gpu
            .upload("Flat unclipped cells", &vec![0_u32; count])
            .unwrap();
        let output = apply(
            &gpu,
            Inputs::Calibrated {
                input: &input,
                original: &original,
                shield_corrected: &shield,
                black: &black,
                clipping: &clipping,
            },
            &p,
            &mut Profile::default(),
        )
        .unwrap();
        for value in bytemuck::cast_slice::<u8, [f32; 3]>(&gpu.readback(&output).unwrap()) {
            for (actual, expected) in value.iter().zip([5.98, 8.97, 1.495]) {
                assert!((actual - expected).abs() < 1e-4);
            }
        }
    }
}
