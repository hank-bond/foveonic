//! Checks local-band cache identities, inactive paths, layer direction, and spatial-scale behavior.

use super::*;

fn parameters() -> Parameters {
    Parameters {
        source: [128, 96, 1, 1],
        tile: [256, 224, 64, 11],
        area: [0, 0, 128, 96],
        filter: [0.08, 1., 0.6, 0.92],
        edge: [1., 10., 5., 100.],
        weights: [1. / 3.; 4],
        model: [[100., 100., 100., 0.], [0.; 4], [0.; 4]],
        mono: [1.; 4],
        diffusion: [0.25, 1., 2., 0.],
        column_area: [0, 0, 128, 96],
        column: [0.375, 27., 0., 0.],
        reduction: [1., 0., 0., 0.],
        geometry: [8, 8, 21, 0],
        window: [0, 0, 1, 1],
        region: [0, 0, 128, 96],
    }
}
fn controls() -> MerrillBandingControls {
    MerrillBandingControls {
        enabled: true,
        ..Default::default()
    }
}

#[test]
fn validates_support_and_keys_each_axis_band_and_boundary() {
    let p = parameters();
    let area = [4, 4, 124, 92];
    let controls = controls();
    let keys = |prepared: Prepared| {
        prepared
            .key(Stage::Correct(Axis::Columns))
            .join(prepared.key(Stage::Correct(Axis::Rows)))
    };
    let key = keys(Prepared::new(p, area, controls).unwrap());
    for axis in 0..2 {
        for index in 0..2 {
            for size in [false, true] {
                let mut edited = controls;
                let axis = if axis == 0 {
                    &mut edited.columns
                } else {
                    &mut edited.rows
                };
                if size {
                    axis.bands[index].size -= 2;
                } else {
                    axis.bands[index].strength += 0.25;
                }
                assert!(key != keys(Prepared::new(p, area, edited).unwrap()));
            }
        }
    }
    assert!(key != keys(Prepared::new(p, [5, 4, 124, 92], controls).unwrap()));
    assert!(
        key != keys(
            Prepared::new(
                p,
                area,
                MerrillBandingControls {
                    enabled: false,
                    ..Default::default()
                }
            )
            .unwrap()
        )
    );
    assert!(Prepared::new(p, [4, 4, 129, 92], controls).is_err());
    let mut short = p;
    short.tile[2] = 32;
    short.tile[0] = 192;
    short.tile[1] = 160;
    assert!(Prepared::new(short, area, controls).is_err());
    let mut coarse = controls.columns;
    coarse.bands[0].strength = 0.;
    coarse.bands[1].strength = 1.;
    assert_eq!(required_estimates(coarse, true), [false, true, true]);
    let reduced = Prepared::new(p, area, controls)
        .unwrap()
        .with_native_extent([256, 192])
        .unwrap();
    assert_eq!(
        reduced.working_controls(0).bands.map(|band| band.size),
        [15, 61]
    );
    assert!(!reduced.uses_reference_fine_boundary(0));
    assert!(key != keys(reduced));
}

#[test]
fn band_measurements_track_support_and_required_slots_independently_of_strength() {
    let p = parameters();
    let area = [4, 4, 124, 92];
    let controls = controls();
    let base = Prepared::new(p, area, controls).unwrap();
    for axis in [Axis::Columns, Axis::Rows] {
        let mut changed = controls;
        let selected = match axis {
            Axis::Columns => &mut changed.columns,
            Axis::Rows => &mut changed.rows,
        };
        selected.bands[0].strength = 2.;
        let changed = Prepared::new(p, area, changed).unwrap();
        assert!(base.key(Stage::Tiles) == changed.key(Stage::Tiles));
        assert!(base.key(Stage::Measure(axis)) == changed.key(Stage::Measure(axis)));
        assert!(base.key(Stage::Correct(axis)) != changed.key(Stage::Correct(axis)));
        let other = match axis {
            Axis::Columns => Axis::Rows,
            Axis::Rows => Axis::Columns,
        };
        assert!(base.key(Stage::Measure(other)) == changed.key(Stage::Measure(other)));
        assert!(base.key(Stage::Correct(other)) == changed.key(Stage::Correct(other)));
    }
    let mut changed = controls;
    changed.rows.bands[1].strength = 0.5;
    let coarse = Prepared::new(p, area, changed).unwrap();
    assert!(base.key(Stage::Measure(Axis::Rows)) != coarse.key(Stage::Measure(Axis::Rows)));
    changed.rows.bands[1].strength = 3.;
    let stronger = Prepared::new(p, area, changed).unwrap();
    assert!(coarse.key(Stage::Measure(Axis::Rows)) == stronger.key(Stage::Measure(Axis::Rows)));
    changed.rows.bands[0].size = 25;
    let changed_support = Prepared::new(p, area, changed).unwrap();
    assert!(
        coarse.key(Stage::Measure(Axis::Rows)) != changed_support.key(Stage::Measure(Axis::Rows))
    );
}

fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let mut tiles = apply_stage(gpu, input, None, prepared, Stage::Tiles, profile)?;
    for axis in [Axis::Columns, Axis::Rows] {
        let estimates = apply_stage(gpu, &tiles, None, prepared, Stage::Measure(axis), profile)?;
        tiles = apply_stage(
            gpu,
            &tiles,
            Some(&estimates),
            prepared,
            Stage::Correct(axis),
            profile,
        )?;
    }
    Ok(tiles)
}

fn read(gpu: &Gpu, buffer: &ReadGuard) -> Vec<f32> {
    bytemuck::cast_slice::<u8, f32>(&gpu.readback(buffer).unwrap()).to_vec()
}

fn reference_fine_axis(input: &[f32], p: Parameters, axis: usize) -> Vec<f32> {
    let width = p.tile[0] as usize;
    let height = p.tile[1] as usize;
    let extent = [width, height];
    let perpendicular = 1 - axis;
    let lookup = super::super::reconstruction::tables(p);
    let at = |direction: usize, other: usize| -> usize {
        if axis == 0 {
            (other * width + direction) * 3
        } else {
            (direction * width + other) * 3
        }
    };
    let mut smooth = vec![0.; input.len()];
    for other in 0..extent[perpendicular] {
        for direction in 13..extent[axis] - 14 {
            let index = at(direction, other);
            let center = &input[index..index + 3];
            let scales = [0, 1, 2].map(|c| (2047_f32 / (8. / p.column[0])) / p.model[0][c]);
            let mut sum = [0_f32; 3];
            let mut total = 0_f32;
            for neighbor in direction - 13..=direction + 13 {
                let sample = &input[at(neighbor, other)..at(neighbor, other) + 3];
                let d = [0, 1, 2].map(|c| sample[c] - center[c]);
                let distance = (d[2] * d[2]).mul_add(
                    scales[2],
                    (d[0] * d[0]).mul_add(scales[0], scales[1] * (d[1] * d[1])),
                );
                if distance < 2047.5 {
                    let weight = lookup[(distance.max(0.) + 0.5).floor() as usize];
                    total += weight;
                    for c in 0..3 {
                        sum[c] = sample[c].mul_add(weight, sum[c]);
                    }
                }
            }
            for c in 0..3 {
                smooth[index + c] = if total != 0. {
                    sum[c] / total
                } else {
                    center[c]
                };
            }
        }
    }
    let start = p.column_area[perpendicular] as usize;
    let end = extent[perpendicular]
        - (p.source[perpendicular] - p.column_area[perpendicular + 2]) as usize;
    let count = (end - start) / 40;
    let mut result = input.to_vec();
    for block in 0..count {
        for direction in 13..extent[axis] - 13 {
            let first = start + block * 40;
            let last = if block + 1 == count { end } else { first + 40 };
            let mut original = [0_f32; 3];
            let mut filtered = [0_f32; 3];
            for other in first..last {
                let index = at(direction, other);
                for c in 0..3 {
                    original[c] += input[index + c];
                    filtered[c] += smooth[index + c];
                }
            }
            let delta = [0, 1, 2].map(|c| {
                original[c] / (last - first + 1) as f32 - filtered[c] / (last - first + 1) as f32
            });
            for other in first..last {
                let index = at(direction, other);
                for c in 0..3 {
                    let value = input[index + c];
                    let limit = (value * 0.06).abs().max(2. * (p.model[0][c] / 40.).sqrt());
                    result[index + c] = (value
                        - if limit > delta[c].abs() { delta[c] } else { 0. })
                    .clamp(0., 32766.);
                }
            }
        }
    }
    result
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn inactive_bands_preserve_source_and_directional_passes_reduce_matching_noise() {
    let gpu = Gpu::new().unwrap();
    let p = parameters();
    let area = [4, 4, 124, 92];
    let source: Vec<f32> = (0..96)
        .flat_map(|y| {
            (0..128).flat_map(move |x| {
                let row = 1.5 * (y as f32 * 0.7).sin();
                let col = 1.5 * (x as f32 * 0.7).sin();
                [100. + row + col, 100. - row - col, 100. + row + col]
            })
        })
        .collect();
    let input = gpu.upload("Directional synthetic sensor", &source).unwrap();
    let raw =
        super::super::reconstruction::raw_tiles(&gpu, &input, p, &mut Profile::default()).unwrap();
    let raw = read(&gpu, &raw);
    let mut off = controls();
    for axis in [&mut off.rows, &mut off.columns] {
        for band in &mut axis.bands {
            band.strength = 0.;
        }
    }
    let output = apply(
        &gpu,
        &input,
        &Prepared::new(p, area, off).unwrap(),
        &mut Profile::default(),
    )
    .unwrap();
    assert_eq!(raw, read(&gpu, &output));
    let prepared = Prepared::new(p, area, controls()).unwrap();
    let output = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
    let actual = read(&gpu, &output);
    let expected = reference_fine_axis(&reference_fine_axis(&raw, p, 0), p, 1);
    let maximum = actual
        .iter()
        .zip(&expected)
        .map(|(a, b)| (a - b).abs())
        .fold(0_f32, f32::max);
    assert!(maximum < 0.001, "reference fine comparison {maximum}");
    let mut before = 0.;
    let mut after = 0.;
    for y in 16..80 {
        for x in 16..112 {
            let index = ((y + 64) * 256 + x + 64) * 3;
            before += (raw[index] - 100.).powi(2);
            after += (actual[index] - 100.).powi(2);
            assert!((actual[index] - actual[index + 2]).abs() < 0.0001);
        }
    }
    assert!(after < before * 0.2, "{after} versus {before}");
    assert_eq!(read(&gpu, &input), source);
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn added_bands_exclude_shields_and_strength_scales_the_same_estimate() {
    let gpu = Gpu::new().unwrap();
    let p = parameters();
    let area = [12, 12, 116, 84];
    let source = |shield: f32| -> Vec<f32> {
        (0..96)
            .flat_map(|y| {
                (0..128).flat_map(move |x| {
                    if !(12..116).contains(&x) || !(12..84).contains(&y) {
                        [shield; 3]
                    } else {
                        let value = 100. + 1.5 * (x as f32 * 0.12).sin();
                        [value, 200. - value, value]
                    }
                })
            })
            .collect()
    };
    let render = |source: &[f32], strength: f32| -> Vec<f32> {
        let input = gpu.upload("Active-boundary fixture", source).unwrap();
        let mut controls = controls();
        for axis in [&mut controls.columns, &mut controls.rows] {
            for band in &mut axis.bands {
                band.strength = 0.;
            }
        }
        controls.columns.bands[1].strength = strength;
        read(
            &gpu,
            &apply(
                &gpu,
                &input,
                &Prepared::new(p, area, controls).unwrap(),
                &mut Profile::default(),
            )
            .unwrap(),
        )
    };
    let ordinary = source(0.);
    let extreme = source(30000.);
    let half = render(&ordinary, 0.5);
    let full = render(&ordinary, 1.);
    let double = render(&ordinary, 2.);
    let triple = render(&ordinary, 3.);
    let other = render(&extreme, 1.);
    let mut changed = false;
    for y in 12..84 {
        for x in 12..116 {
            for c in 0..3 {
                let local = ((y + 64) * 256 + x + 64) * 3 + c;
                let native = (y * 128 + x) * 3 + c;
                assert!(
                    (full[local] - other[local]).abs() < 0.0001,
                    "shield affected active pixel ({x},{y})"
                );
                let delta = ordinary[native] - full[local];
                changed |= delta.abs() > 0.01;
                assert!((ordinary[native] - half[local] - delta * 0.5).abs() < 0.0001);
                assert!((ordinary[native] - double[local] - delta * 2.).abs() < 0.0001);
                assert!((ordinary[native] - triple[local] - delta * 3.).abs() < 0.0001);
            }
        }
    }
    assert!(
        changed,
        "the added band did not exercise its strength control"
    );
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn row_and_column_estimators_transpose_without_blending_a_real_edge() {
    let gpu = Gpu::new().unwrap();
    let p = parameters();
    let area = [4, 4, 124, 92];
    let source: Vec<f32> = (0..96)
        .flat_map(|y| {
            (0..128).flat_map(move |x| {
                let edge = if x < 64 { 100. } else { 160. };
                let noise = 1.5 * (x as f32 * 0.15).sin() + 0.1 * (y as f32 * 0.3).cos();
                [edge + noise, edge - noise, edge + noise]
            })
        })
        .collect();
    let mut columns = controls();
    for band in &mut columns.rows.bands {
        band.strength = 0.;
    }
    columns.columns.bands[1].strength = 0.5;
    let input = gpu.upload("Real edge and column bands", &source).unwrap();
    let actual = read(
        &gpu,
        &apply(
            &gpu,
            &input,
            &Prepared::new(p, area, columns).unwrap(),
            &mut Profile::default(),
        )
        .unwrap(),
    );
    let mut transposed = p;
    transposed.source = [96, 128, 1, 1];
    transposed.tile = [224, 256, 64, 11];
    transposed.area = [0, 0, 96, 128];
    transposed.column_area = [0, 0, 96, 128];
    transposed.region = [0, 0, 96, 128];
    let mut transpose = vec![0.; source.len()];
    for y in 0..96 {
        for x in 0..128 {
            for c in 0..3 {
                transpose[(x * 96 + y) * 3 + c] = source[(y * 128 + x) * 3 + c];
            }
        }
    }
    let input = gpu
        .upload("Transposed real edge and row bands", &transpose)
        .unwrap();
    let rows = MerrillBandingControls {
        enabled: true,
        method: columns.method,
        rows: columns.columns,
        columns: columns.rows,
    };
    let expected = read(
        &gpu,
        &apply(
            &gpu,
            &input,
            &Prepared::new(transposed, [4, 4, 92, 124], rows).unwrap(),
            &mut Profile::default(),
        )
        .unwrap(),
    );
    for y in 0..224 {
        for x in 0..256 {
            for c in 0..3 {
                assert!(
                    (actual[(y * 256 + x) * 3 + c] - expected[(x * 224 + y) * 3 + c]).abs()
                        < 0.0001
                );
            }
        }
    }
    for y in 20..76 {
        for x in 58..70 {
            for c in 0..3 {
                let edge = if x < 64 { 100. } else { 160. };
                assert!(
                    (actual[((y + 64) * 256 + x + 64) * 3 + c] - edge).abs() < 2.,
                    "real edge changed ({x},{y})"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn compact_regions_retain_native_tile_phase_and_reflection() {
    let gpu = Gpu::new().unwrap();
    let mut p = parameters();
    p.source = [256, 192, 2, 2];
    p.window = [0, 0, 2, 2];
    p.area = [0, 0, 256, 192];
    p.region = p.area;
    p.column_area = p.area;
    let area = [4, 4, 252, 188];
    let mut controls = controls();
    controls.columns.bands[1].strength = 0.75;
    controls.rows.bands[1].strength = 0.5;
    let source: Vec<f32> = (0..192)
        .flat_map(|y| {
            (0..256).flat_map(move |x| {
                let noise = 1.5 * (y as f32 * 0.2).sin() + 1.5 * (x as f32 * 0.4).cos();
                [100. + noise, 100. - noise, 100. + noise]
            })
        })
        .collect();
    let input = gpu.upload("Regional native tile fixture", &source).unwrap();
    let full = read(
        &gpu,
        &apply(
            &gpu,
            &input,
            &Prepared::new(p, area, controls).unwrap(),
            &mut Profile::default(),
        )
        .unwrap(),
    );
    let compact = p.for_region([144, 112, 240, 176]).unwrap();
    assert_eq!(compact.window, [1, 1, 1, 1]);
    let region = read(
        &gpu,
        &apply(
            &gpu,
            &input,
            &Prepared::new(compact, area, controls).unwrap(),
            &mut Profile::default(),
        )
        .unwrap(),
    );
    let tile_floats = (p.tile[0] * p.tile[1] * 3) as usize;
    assert_eq!(&full[3 * tile_floats..4 * tile_floats], region);
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn negative_layers_vary_continuously_near_zero_with_added_bands() {
    let gpu = Gpu::new().unwrap();
    let p = parameters();
    let area = [4, 4, 124, 92];
    let source: Vec<f32> = (0..96)
        .flat_map(|_| {
            (0..128).flat_map(|x| {
                let noise = 0.6 * (x as f32 * 0.18).sin() + 0.4 * (x as f32 * 0.07).cos();
                [-8. + noise, -8. - noise, -8. + noise]
            })
        })
        .collect();
    let input = gpu.upload("Signed calibrated layers", &source).unwrap();
    let render = |fine: f32, coarse: f32| -> Vec<f32> {
        let mut controls = controls();
        for band in &mut controls.rows.bands {
            band.strength = 0.;
        }
        for (band, strength) in controls.columns.bands.iter_mut().zip([fine, coarse]) {
            band.strength = strength;
        }
        read(
            &gpu,
            &apply(
                &gpu,
                &input,
                &Prepared::new(p, area, controls).unwrap(),
                &mut Profile::default(),
            )
            .unwrap(),
        )
    };
    let raw = render(0., 0.);
    let unit = render(1., 0.);
    let expected = reference_fine_axis(&raw, p, 0);
    assert_eq!(
        unit, expected,
        "unit-strength black floor differs from the reference fine pass"
    );
    let baseline = render(0., 0.5);
    let epsilon = render(0.0001, 0.5);
    let twice = render(0.0002, 0.5);
    let coarse_epsilon = render(0., 0.0001);
    let mut changed = false;
    for y in 16..80 {
        for x in 16..112 {
            for c in 0..3 {
                let index = ((y + 64) * 256 + x + 64) * 3 + c;
                changed |= (baseline[index] - raw[index]).abs() > 0.01;
                assert!(
                    baseline[index] < -5.,
                    "added bands introduced a black-floor jump"
                );
                assert!((epsilon[index] - baseline[index]).abs() < 0.002);
                assert!(
                    (twice[index] - baseline[index] - 2. * (epsilon[index] - baseline[index]))
                        .abs()
                        < 0.000004
                );
                assert!((coarse_epsilon[index] - raw[index]).abs() < 0.001);
            }
        }
    }
    assert!(changed, "added bands did not exercise accepted corrections");
}
