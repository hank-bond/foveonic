//! Checks guided native-layer filtering, reflected boundaries and immutable clipping against independent equations.

use super::*;

#[derive(Clone)]
struct Image {
    width: usize,
    height: usize,
    pixels: Vec<[f32; 3]>,
}

fn reflect(value: isize, extent: usize) -> usize {
    let period = 2 * (extent as isize - 1);
    let folded = value.rem_euclid(period);
    folded.min(period - folded) as usize
}

fn average(image: &Image, radius: usize) -> Image {
    let mut output = image.clone();
    let count = ((radius * 2 + 1).pow(2)) as f32;
    for y in 0..image.height {
        for x in 0..image.width {
            let mut sum = [0.; 3];
            for dy in -(radius as isize)..=radius as isize {
                for dx in -(radius as isize)..=radius as isize {
                    let value = image.pixels[reflect(y as isize + dy, image.height) * image.width
                        + reflect(x as isize + dx, image.width)];
                    for c in 0..3 {
                        sum[c] += value[c];
                    }
                }
            }
            output.pixels[y * image.width + x] = sum.map(|v| v / count);
        }
    }
    output
}

fn estimate(image: &Image, noise: [f32; 9], radius: usize, strength: f32, scale: usize) -> Image {
    let mut guide = image.clone();
    let mut square = image.clone();
    let mut cross = image.clone();
    for i in 0..image.pixels.len() {
        let value = image.pixels[i];
        let g = value.iter().sum::<f32>() / 3.;
        guide.pixels[i] = [g * g; 3];
        square.pixels[i] = value.map(|v| v * v);
        cross.pixels[i] = value.map(|v| v * g);
    }
    let mean = average(image, radius);
    let square = average(&square, radius);
    let cross = average(&cross, radius);
    let guide_square = average(&guide, radius);
    let mut a = image.clone();
    let mut b = image.clone();
    let mut weight = image.clone();
    for i in 0..image.pixels.len() {
        let mc = mean.pixels[i];
        let ml = mc.iter().sum::<f32>() / 3.;
        let vl = (guide_square.pixels[i][0] - ml * ml).max(VARIANCE_FLOOR);
        let variance: [f32; 3] = std::array::from_fn(|c| {
            let signal = mc[c].max(0.);
            ((noise[c] + noise[3 + c] * signal + noise[6 + c] * signal * signal)
                / (scale * scale) as f32)
                .max(VARIANCE_FLOOR)
        });
        let el = variance.iter().sum::<f32>() / 9.;
        for c in 0..3 {
            let vc = (square.pixels[i][c] - mc[c] * mc[c]).max(VARIANCE_FLOOR);
            let cov = cross.pixels[i][c] - mc[c] * ml;
            let residual = (vc - cov * cov / (vl + el)).max(0.);
            let w = residual / (residual + variance[c] * strength);
            let prior = (cov / (vl + 1.)).max(0.);
            let slope = ((1. - w) * cov + prior * el) / (vl + el);
            if slope.abs() > 8. {
                a.pixels[i][c] = 0.;
                b.pixels[i][c] = 0.;
                weight.pixels[i][c] = 1.;
            } else {
                a.pixels[i][c] = slope;
                b.pixels[i][c] = (1. - w) * mc[c] - slope * ml;
                weight.pixels[i][c] = w;
            }
        }
    }
    let a = average(&a, radius);
    let b = average(&b, radius);
    let weight = average(&weight, radius);
    let mut output = image.clone();
    for i in 0..image.pixels.len() {
        let g = image.pixels[i].iter().sum::<f32>() / 3.;
        output.pixels[i] = std::array::from_fn(|c| {
            a.pixels[i][c] * g + b.pixels[i][c] + weight.pixels[i][c] * image.pixels[i][c]
        });
    }
    output
}

fn reference(
    input: &[[f32; 3]],
    original: &[[f32; 3]],
    black: [f32; 3],
    p: &Prepared,
) -> Vec<[f32; 3]> {
    let [width, _] = p.extent().map(|v| v as usize);
    let [left, top, right, bottom] = p.params.area.map(|v| v as usize);
    let mut current = Image {
        width: p.params.source[2] as usize,
        height: p.params.source[3] as usize,
        pixels: Vec::new(),
    };
    for y in 0..current.height {
        for x in 0..current.width {
            let value = input[(top + reflect(y as isize, bottom - top)) * width
                + left
                + reflect(x as isize, right - left)];
            current
                .pixels
                .push(std::array::from_fn(|c| value[c] - black[c]));
        }
    }
    let noise: [f32; 9] = std::array::from_fn(|i| p.params.noise[i / 3][i % 3]);
    for Stage {
        scale,
        radius,
        strength,
    } in p.stages()
    {
        let scale = scale as usize;
        let mut coarse = Image {
            width: current.width / scale,
            height: current.height / scale,
            pixels: Vec::new(),
        };
        for y in 0..coarse.height {
            for x in 0..coarse.width {
                let mut sum = [0.; 3];
                for dy in 0..scale {
                    for dx in 0..scale {
                        let value =
                            current.pixels[(y * scale + dy) * current.width + x * scale + dx];
                        for c in 0..3 {
                            sum[c] += value[c];
                        }
                    }
                }
                coarse.pixels.push(sum.map(|v| v / (scale * scale) as f32));
            }
        }
        let filtered = estimate(&coarse, noise, radius as usize, strength, scale);
        let delta: Vec<[f32; 3]> = filtered
            .pixels
            .iter()
            .zip(&coarse.pixels)
            .map(|(a, b)| std::array::from_fn(|c| a[c] - b[c]))
            .collect();
        for y in 0..current.height {
            for x in 0..current.width {
                let coordinate = [
                    (x as f32 + 0.5) / scale as f32 - 0.5,
                    (y as f32 + 0.5) / scale as f32 - 0.5,
                ];
                let lower = coordinate.map(|v| v.floor() as isize);
                let blend = coordinate.map(|v| v - v.floor());
                let sample = |dx: isize, dy: isize| {
                    delta[(lower[1] + dy).clamp(0, coarse.height as isize - 1) as usize
                        * coarse.width
                        + (lower[0] + dx).clamp(0, coarse.width as isize - 1) as usize]
                };
                let [a, b, c, d] = [sample(0, 0), sample(1, 0), sample(0, 1), sample(1, 1)];
                let mut difference: [f32; 3] = std::array::from_fn(|ch| {
                    let top = a[ch] * (1. - blend[0]) + b[ch] * blend[0];
                    let bottom = c[ch] * (1. - blend[0]) + d[ch] * blend[0];
                    top * (1. - blend[1]) + bottom * blend[1]
                });
                let sum = difference.iter().sum::<f32>() / p.params.neutral[3];
                for (ch, value) in difference.iter_mut().enumerate() {
                    *value -= p.params.neutral[ch] * sum;
                    current.pixels[y * current.width + x][ch] += *value;
                }
            }
        }
    }
    let mut result = original.to_vec();
    for y in top..bottom {
        for x in left..right {
            let i = y * width + x;
            if original[i].iter().any(|v| *v >= 3900.) {
                continue;
            }
            result[i] = std::array::from_fn(|c| {
                current.pixels[(y - top) * current.width + x - left][c] + black[c]
            });
        }
    }
    result
}

#[test]
fn multiscale_shader_validates_without_a_device() {
    let module = wgpu::naga::front::wgsl::parse_str(concat!(
        include_str!("calibration_common.wgsl"),
        "\n",
        include_str!("multiscale.wgsl")
    ))
    .unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    for entry in &module.entry_points {
        let expected = if matches!(
            entry.name.as_str(),
            "multiscale_moments" | "multiscale_average"
        ) {
            [HORIZONTAL_WORKGROUP_WIDTH, HORIZONTAL_WORKGROUP_HEIGHT, 1]
        } else {
            [WORKGROUP_SIDE, WORKGROUP_SIDE, 1]
        };
        assert_eq!(entry.workgroup_size, expected, "{} dispatch", entry.name);
    }
}

#[test]
fn rejects_invalid_multiscale_metadata() {
    assert!(Prepared::new([96, 80], [0, 0, 96, 80], [1.; 9], [1.; 3]).is_ok());
    assert!(Prepared::new([96, 80], [0, 0, 64, 80], [1.; 9], [1.; 3]).is_err());
    assert!(Prepared::new([96, 80], [0, 0, 97, 80], [1.; 9], [1.; 3]).is_err());
    assert!(Prepared::new([96, 80], [0, 0, 96, 80], [f32::NAN; 9], [1.; 3]).is_err());
    assert!(Prepared::new([96, 80], [0, 0, 96, 80], [1.; 9], [0.; 3]).is_err());
}

#[test]
#[ignore = "requires a GPU"]
fn guided_three_scales_match_independent_equations_and_preserve_sensor_contracts() {
    let gpu = Gpu::new().unwrap();
    let extent = [100, 86];
    // The padded width of 80 leaves a partial horizontal workgroup at native scale.
    let area = [3, 3, 82, 80];
    let black = [16., 17., 15.];
    let noise = [4., 9., 0.2, 0.04, 0.1, 0.02, 0.0001, 0.0002, 0.0001];
    let prepared = Prepared::new(extent, area, noise, [0.7, 1.3, 0.4]).unwrap();
    let mut input = Vec::new();
    for y in 0..extent[1] {
        for x in 0..extent[0] {
            let guide = 10. + 0.05 * x as f32 + 0.02 * y as f32;
            let jitter = ((x * 17 + y * 31) as f32).sin();
            input.push([
                black[0] + guide * 0.9 + jitter * 2.,
                black[1] + guide * 0.6 - jitter * 3. + if x > 49 { 2. } else { 0. },
                black[2] + guide * 0.4 + jitter * 0.3,
            ]);
        }
    }
    let mut original = input.clone();
    let clipped = 30 * extent[0] as usize + 30;
    // Clipping provenance must come from the immutable original, even when an
    // earlier sensor correction moved the working sample below the threshold.
    original[clipped] = [3900., 100., 50.];
    let expected = reference(&input, &original, black, &prepared);
    let source = gpu
        .upload("Multiscale synthetic native input", &input)
        .unwrap();
    let original_buffer = gpu
        .upload("Multiscale immutable original", &original)
        .unwrap();
    let black_buffer = gpu.upload("Multiscale synthetic black", &black).unwrap();
    let output = apply(
        &gpu,
        &source,
        Evidence::Native {
            original: &original_buffer,
            black: &black_buffer,
        },
        &prepared,
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, [f32; 3]>(&bytes);
    let mut changed = 0;
    for y in 0..extent[1] as usize {
        for x in 0..extent[0] as usize {
            let i = y * extent[0] as usize + x;
            let outside = x < area[0] as usize
                || x >= area[2] as usize
                || y < area[1] as usize
                || y >= area[3] as usize;
            if outside || i == clipped {
                assert_eq!(actual[i], original[i], "immutable pixel {x},{y}");
                continue;
            }
            for c in 0..3 {
                assert!(actual[i][c].is_finite());
                assert!(
                    (actual[i][c] - expected[i][c]).abs() < 0.003,
                    "pixel {x},{y} layer {c}: {} versus {}",
                    actual[i][c],
                    expected[i][c]
                );
            }
            assert!((actual[i].iter().sum::<f32>() - input[i].iter().sum::<f32>()).abs() < 0.00005);
            changed += usize::from((actual[i][0] - input[i][0]).abs() > 0.01);
        }
    }
    assert!(changed > 100, "fixture must exercise actual filtering");
    let constant = vec![[35., 23., 25.]; input.len()];
    let source = gpu
        .upload("Multiscale constant unequal layers", &constant)
        .unwrap();
    let output = apply(
        &gpu,
        &source,
        Evidence::Native {
            original: &source,
            black: &black_buffer,
        },
        &prepared,
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&output).unwrap();
    for (actual, expected) in bytemuck::cast_slice::<u8, [f32; 3]>(&bytes)
        .iter()
        .zip(&constant)
    {
        for c in 0..3 {
            assert!((actual[c] - expected[c]).abs() < 0.0001);
        }
    }
}

fn unit_calibration(extent: [u32; 2], gains: [f32; 3]) -> Arc<Calibration> {
    Arc::new(Calibration {
        params: super::super::calibration::CalibrationParams {
            source: [extent[0], extent[1], 0, 0],
            dark: [0; 4],
            gains: [gains[0], gains[1], gains[2], 0.],
            shading: [0.; 4],
            shading_geometry: [0., 0., 1., 0.],
            grids: [[0, 2, 2, 0], [4, 2, 2, 0], [8, 2, 2, 0]],
        },
        grid_values: vec![1.; 12],
    })
}

#[test]
fn working_grid_projects_noise_support_and_calibration_identity() {
    let extent = [257, 193];
    let base = Prepared::new(extent, [0, 0, 257, 193], [4.; 9], [1.; 3]).unwrap();
    for d in [2, 4, 8, 16, 64, 256] {
        let grid = WorkingGrid::new(extent, crate::render::Scale::new(d).unwrap(), 8).unwrap();
        let p = base
            .clone()
            .on_grid(grid, unit_calibration(extent, [2., 3., 0.5]));
        assert_eq!(p.extent(), grid.extent);
        assert_eq!(p.params.noise[0][0], 4. / (d * d) as f32);
        for (actual, native) in p.stages().into_iter().zip(STAGES) {
            assert!(actual.scale >= 1 && actual.radius >= 1 && actual.radius <= 4);
            assert_eq!(actual.scale, (native.scale / d).max(1));
        }
        let other = base
            .clone()
            .on_grid(grid, unit_calibration(extent, [2., 4., 0.5]));
        assert!(p.key() != other.key());
    }
}

#[test]
#[ignore = "requires a GPU"]
fn calibrated_filter_matches_sensor_basis_and_preserves_original_clipping() {
    let gpu = Gpu::new().unwrap();
    let extent = [257, 193];
    let grid = WorkingGrid::new(extent, crate::render::Scale::new(2).unwrap(), 8).unwrap();
    let gains = [2., 3., 0.5];
    let p = Prepared::new(
        extent,
        [0, 0, 257, 193],
        [16., 12., 9., 0.01, 0.02, 0.01, 0., 0., 0.],
        [1.; 3],
    )
    .unwrap()
    .on_grid(grid, unit_calibration(extent, gains));
    let raw: Vec<[f32; 3]> = (0..grid.extent[0] * grid.extent[1])
        .map(|i| {
            let x = i % grid.extent[0];
            let y = i / grid.extent[0];
            let signal = 60. + x as f32 * 0.2 + y as f32 * 0.1;
            let jitter = ((x * 17 + y * 29) % 19) as f32 - 9.;
            [
                signal + jitter,
                signal - jitter * 0.6,
                signal + jitter * 0.3,
            ]
        })
        .collect();
    let input: Vec<[f32; 3]> = raw
        .iter()
        .map(|v| std::array::from_fn(|c| v[c] * gains[c]))
        .collect();
    let mut flags = vec![0_u32; raw.len()];
    flags[30 * grid.extent[0] as usize + 30] = 1;
    let expected = reference(&raw, &raw, [0.; 3], &p);
    let source = gpu.upload("Calibrated pooled input", &input).unwrap();
    let clipping = gpu.upload("Pooled clipping evidence", &flags).unwrap();
    let output = apply(
        &gpu,
        &source,
        Evidence::Calibrated {
            clipping: &clipping,
        },
        &p,
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, [f32; 3]>(&bytes);
    let mut changed = 0;
    for i in 0..actual.len() {
        for c in 0..3 {
            let want = if flags[i] != 0 {
                input[i][c]
            } else {
                expected[i][c] * gains[c]
            };
            assert!(
                (actual[i][c] - want).abs() < 0.04,
                "pixel {i} channel {c}: {} != {want}",
                actual[i][c]
            );
        }
        changed += usize::from((actual[i][0] - input[i][0]).abs() > 0.01);
    }
    assert!(changed > 100, "must exercise filtering rather than bypass");
    assert_eq!(
        gpu.readback(&source).unwrap(),
        bytemuck::cast_slice::<[f32; 3], u8>(&input)
    );
}

#[test]
#[ignore = "requires a GPU"]
fn calibrated_filter_handles_one_cell_and_replicated_padding() {
    let gpu = Gpu::new().unwrap();
    for d in [4, 8, 64, 512] {
        let extent = [257, 193];
        let grid = WorkingGrid::new(extent, crate::render::Scale::new(d).unwrap(), 8)
            .unwrap()
            .with_halo(8)
            .unwrap();
        let p = Prepared::new(extent, [0, 0, 257, 193], [1.; 9], [1.; 3])
            .unwrap()
            .on_grid(grid, unit_calibration(extent, [2., 3., 0.5]));
        let input = vec![[120., 180., 30.]; (grid.extent[0] * grid.extent[1]) as usize];
        let source = gpu.upload("Flat calibrated cells", &input).unwrap();
        let flags = gpu
            .upload("Unclipped cells", &vec![0_u32; input.len()])
            .unwrap();
        let output = apply(
            &gpu,
            &source,
            Evidence::Calibrated { clipping: &flags },
            &p,
            &mut Profile::default(),
        )
        .unwrap();
        for value in bytemuck::cast_slice::<u8, [f32; 3]>(&gpu.readback(&output).unwrap()) {
            for c in 0..3 {
                assert!((value[c] - input[0][c]).abs() < 0.001);
            }
        }
    }
}
