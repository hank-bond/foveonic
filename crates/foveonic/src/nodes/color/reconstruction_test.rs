//! Checks reference-style shader arithmetic against synthetic fixtures from the read-only reference library.

use super::*;
use crate::gpu::Binding::{Read, Write};
use serde_json::Value;

fn parameters() -> Parameters {
    Parameters {
        source: [80, 72, 1, 1],
        tile: [80, 72, 0, 11],
        area: [0, 0, 80, 72],
        filter: [0.08, 1., 0.6, 0.92],
        edge: [1., 10., 5., 100.],
        weights: [1. / 3., 1. / 3., 1. / 3., 0.],
        model: [
            [25., 30., 15., 0.],
            [0.03, 0.04, 0.05, 0.],
            [0.0001, 0.0002, 0.00005, 0.],
        ],
        mono: [70. / 9., 0.12 / 9., 0.00035 / 9., 0.],
        residual: [0.6, 0.3, 1.2, 1.],
        diffusion: [0.25, 1., 2., 0.],
        column_area: [0, 8, 80, 64],
        column: [0.375, 27., 0., 0.],
        reduction: [1., 0., 0., 0.],
        geometry: [8, 8, 21, 0],
        window: [0, 0, 1, 1],
        region: [0, 0, 80, 72],
    }
}
#[test]
#[ignore = "requires a GPU"]
fn reduced_local_and_global_bases_match_native_filters_restore_and_transposed_seed() {
    let f: Value =
        serde_json::from_str(include_str!("../../../tests/data/reduced-reference.json")).unwrap();
    let array = |v: &Value| {
        v.as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect::<Vec<_>>()
    };
    let gpu = Gpu::new().unwrap();
    let source = array(&f["input"]);
    let input = gpu.upload("Immutable reduced fixture", &source).unwrap();
    let check =
        |buffer: &crate::gpu::ReadGuard, expected: Vec<f32>, tolerance: f32, label: &str| {
            let actual = floats(&gpu, buffer);
            let mut max = 0_f32;
            let mut pos = 0;
            for (i, (a, b)) in actual.iter().zip(&expected).enumerate() {
                let d = (a - b).abs();
                if d > max {
                    max = d;
                    pos = i;
                }
            }
            assert_eq!(actual.len(), expected.len());
            eprintln!("{label}: maximum {max} at {pos}");
            assert!(
                max < tolerance,
                "{label} maximum {max} at {pos}, {} vs {}",
                actual[pos],
                expected[pos]
            );
        };
    let mut p = parameters();
    p.filter = [0.08, 0.8, 0.6, 0.95];
    p.geometry = [4, 0, 21, 1];
    p.tile[3] = 21;
    let mut bilateral = gpu
        .storage("Global bilateral verification", 80 * 72 * 12)
        .unwrap();
    let radii = gpu.upload("Unused global radii", &[0_u32]).unwrap();
    let lookup = gpu.upload("Global lookup", &tables(p)).unwrap();
    let uniform = gpu.uniform(&p);
    let mut trace = gpu.trace(false, 1);
    gpu.dispatch(
        Kernel::NoiseBilateral,
        &mut [
            (0, Read(&input)),
            (1, Read(&uniform)),
            (2, Write(bilateral.write())),
            (6, Read(&lookup)),
            (7, Read(&radii)),
        ],
        bilateral_groups(p),
        &mut trace,
    );
    gpu.finish_trace(&trace, &mut Profile::default()).unwrap();
    check(
        &bilateral,
        array(&f["global"]["bilateral"]),
        0.08,
        "global bilateral",
    );
    let global = filter(&gpu, &input, p, true, &mut Profile::default()).unwrap();
    check(
        global.tiles.as_ref().unwrap(),
        array(&f["global"]["filtered"]),
        0.1,
        "global fixed/mean",
    );
    check(
        &global.edges,
        array(&f["global"]["edges"]),
        0.005,
        "global LUT",
    );
    for (key, scale) in [("local", 0.75), ("global_half", 0.5)] {
        let mut p = parameters();
        // Base filtering receives resolved coefficients, including linked limits.
        p.filter = [0.08, 0.8, 0.6, if key == "local" { 0.92 } else { 0.95 }];
        p.reduction[0] = scale;
        let base = base(&gpu, &input, p, true, true, true, &mut Profile::default()).unwrap();
        check(
            base.tiles.as_ref().unwrap(),
            array(&f[key]["restored"]),
            0.12,
            key,
        );
        check(
            base.seed.as_ref().unwrap(),
            array(&f[key]["seed"]),
            0.002,
            "native F backing seed",
        );
    }
    // Four context tiles must extend the first source edge (not the local second edge).
    let mut p = parameters();
    p.source = [80, 72, 2, 2];
    p = p.full_frame();
    p.tile = [56, 52, 8, 11];
    let copied = global_tiles(&gpu, &input, p, 3, &mut Profile::default()).unwrap();
    let actual = floats(&gpu, &copied);
    for tile in 0..4 {
        for y in 0..52 {
            for x in 0..56 {
                let sx = ((tile % 2) * 40 + x - 8_i32).clamp(0, 79) as usize;
                let sy = ((tile / 2) * 36 + y - 8_i32).clamp(0, 71) as usize;
                let dst = ((tile * 52 + y) * 56 + x) as usize * 3;
                let src = (sy * 80 + sx) * 3;
                assert!(
                    actual[dst..dst + 3] == source[src..src + 3],
                    "global tile edge at {tile},{x},{y}"
                );
            }
        }
    }
    // Compact reduced B ends in zeroed native scratch, never the following tile.
    let mut p = parameters();
    p.filter = [0.08, 0.8, 0.6, 0.92];
    p.reduction[0] = 0.75;
    let first = base(&gpu, &input, p, false, true, false, &mut Profile::default()).unwrap();
    let first = gpu.readback(first.tiles.as_ref().unwrap()).unwrap();
    let mut wide = Vec::new();
    for row in source.as_chunks::<240>().0 {
        wide.extend_from_slice(row);
        wide.extend(row.iter().map(|v| v + 10000.));
    }
    let wide = gpu
        .upload("Different following reduced tile", &wide)
        .unwrap();
    p.source = [160, 72, 2, 1];
    p = p.full_frame();
    p.area[2] = 160;
    let two = base(&gpu, &wide, p, false, true, false, &mut Profile::default()).unwrap();
    let two = gpu.readback(two.tiles.as_ref().unwrap()).unwrap();
    assert!(
        two[..first.len()] == first,
        "following reduced tile leaked into first base"
    );
    assert!(
        gpu.readback(&input).unwrap() == bytemuck::cast_slice::<f32, u8>(&source),
        "base mutated original source"
    );
}
#[test]
#[ignore = "requires a GPU"]
fn all_column_banding_presets_match_native_windows_and_coefficients() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../tests/data/column-presets.json")).unwrap();
    let source: Vec<f32> = fixture["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload("Five-preset identical column source", &source)
        .unwrap();
    let mut outputs = std::collections::HashSet::new();
    for row in fixture["positions"].as_array().unwrap() {
        let mut p = parameters();
        p.source = [80, 112, 1, 1];
        p = p.full_frame();
        p.tile = [80, 112, 0, 11];
        p.area = [0, 0, 80, 112];
        p.column_area = [0, 8, 80, 104];
        p.column = [
            row["coefficient"].as_f64().unwrap() as f32,
            row["window"].as_f64().unwrap() as f32,
            0.,
            0.,
        ];
        let result = column(&gpu, &input, p, &mut Profile::default()).unwrap();
        let actual = floats(&gpu, &result);
        let expected = row["corrected"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        let maximum = actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (*a - b.as_f64().unwrap() as f32).abs())
            .fold(0_f32, f32::max);
        assert!(actual.iter().all(|v| v.is_finite()));
        assert!(
            maximum < 0.12,
            "preset {} maximum error {maximum}",
            row["preset"]
        );
        assert_ne!(actual, source, "preset 0 is not Off");
        let half = p.column[1] as usize / 2;
        for y in 0..112 {
            for x in 0..80 {
                if !(8..104).contains(&y) || x < half || x >= 80 - half {
                    let at = (y * 80 + x) * 3;
                    assert_eq!(
                        &actual[at..at + 3],
                        &source[at..at + 3],
                        "untouched margin ({x},{y})"
                    );
                }
            }
        }
        outputs.insert(actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>());
    }
    assert_eq!(outputs.len(), 5);
    assert_eq!(floats(&gpu, &input), source);
}

#[test]
#[ignore = "requires a GPU"]
fn column_conditioning_matches_native_blocks_borders_gain_and_clamps() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../tests/data/column-reference.json")).unwrap();
    let source: Vec<f32> = fixture["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    let expected: Vec<f32> = fixture["corrected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    let gpu = Gpu::new().unwrap();
    for gain in [0., 1., 1.3] {
        let mut p = parameters();
        p.source = [80, 112, 1, 1];
        p = p.full_frame();
        p.tile = [80, 112, 0, 11];
        p.area = [0, 0, 80, 112];
        p.column_area = [0, 8, 80, 104];
        for c in 0..3 {
            p.model[0][c] *= gain * gain;
            p.model[1][c] *= gain;
        }
        let scaled: Vec<f32> = source.iter().map(|v| v * gain).collect();
        let input = gpu.upload("Column component source", &scaled).unwrap();
        let result = column(&gpu, &input, p, &mut Profile::default()).unwrap();
        let actual = floats(&gpu, &result);
        let mut maximum = 0_f32;
        let mut position = 0;
        for (i, (&value, &reference)) in actual.iter().zip(&expected).enumerate() {
            assert!(value.is_finite());
            let reference = (reference * gain).clamp(0., 32766.);
            let error = (value - reference).abs();
            if error > maximum {
                maximum = error;
                position = i;
            }
        }
        assert!(
            maximum < 0.12,
            "column gain {gain}: maximum error {maximum}, component {position}"
        );
        assert!(floats(&gpu, &input)
            .iter()
            .zip(&scaled)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        if gain > 0. {
            assert_eq!(actual[(55 * 80 + 40) * 3], 32766.);
            assert_eq!(actual[(56 * 80 + 41) * 3 + 1], 0.);
        }
    }
}

fn source() -> Vec<f32> {
    (0..72)
        .flat_map(|y| {
            (0..80).flat_map(move |x| {
                let v = 100.
                    + x as f32 * 9.
                    + y as f32 * 4.
                    + (((x * 17 + y * 31) % 23) as f32 - 11.) * 1.5;
                [v + if x >= 40 { 160. } else { 0. }, v * 0.8, v * 1.1]
            })
        })
        .collect()
}
fn floats(gpu: &Gpu, buffer: &crate::gpu::ReadGuard) -> Vec<f32> {
    bytemuck::cast_slice::<u8, f32>(&gpu.readback(buffer).unwrap()).to_vec()
}
fn check(data: &[f32], fixture: &Value, name: &str, channels: usize, tolerance: f32) {
    let mut largest = 0_f32;
    let mut location = (0, 0, 0);
    for sample in fixture[name].as_array().unwrap() {
        let x = sample[0].as_u64().unwrap() as usize;
        let y = sample[1].as_u64().unwrap() as usize;
        for c in 0..channels {
            let expected = sample[2][c].as_f64().unwrap() as f32;
            let actual = data[(y * 80 + x) * channels + c];
            assert!(actual.is_finite(), "{name} contains a nonfinite value");
            let difference = (actual - expected).abs();
            if difference > largest {
                largest = difference;
                location = (x, y, c);
            }
        }
    }
    assert!(
        largest <= tolerance,
        "{name}: largest difference {largest} at {location:?}, tolerance {tolerance}"
    );
}

#[test]
#[ignore = "requires a GPU"]
fn reference_noise_and_detail_components_match_synthetic_fixtures() {
    let gpu = Gpu::new().unwrap();
    let p = parameters();
    let input = gpu.upload("Synthetic sensor layers", &source()).unwrap();
    let uniform = gpu.uniform(&p);
    let lookup = gpu.upload("Noise tables", &tables(p)).unwrap();
    let mut block = gpu.storage("Block means", 9 * 4).unwrap();
    let mut radii = gpu.storage("Radii", 9 * 4).unwrap();
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
    let mut trace = gpu.trace(true, 3);
    gpu.dispatch(
        Kernel::NoiseBlock,
        &mut [
            (0, Read(&input)),
            (1, Read(&uniform)),
            (4, Write(block.write())),
        ],
        [1, 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseRadius,
        &mut [
            (1, Read(&uniform)),
            (4, Read(&block)),
            (7, Write(radii.write())),
        ],
        [1, 1],
        &mut trace,
    );
    let mut filtered = gpu.storage("Bilateral fixture", 80 * 72 * 12).unwrap();
    gpu.dispatch(
        Kernel::NoiseBilateral,
        &mut [
            (0, Read(&input)),
            (1, Read(&uniform)),
            (2, Write(filtered.write())),
            (6, Read(&lookup)),
            (7, Read(&radii)),
        ],
        bilateral_groups(p),
        &mut trace,
    );
    gpu.finish_trace(&trace, &mut profile).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../../../tests/data/noise-reference.json")).unwrap();
    let radius_bytes = gpu.readback(&radii).unwrap();
    let actual = bytemuck::cast_slice::<u8, u32>(&radius_bytes);
    let expected: Vec<u32> = fixture["radii"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u32)
        .collect();
    assert_eq!(actual, expected);
    check(&floats(&gpu, &filtered), &fixture, "bilateral", 3, 0.08);
    let base = base(&gpu, &input, p, true, true, false, &mut profile).unwrap();
    check(
        &floats(&gpu, base.edges.as_ref().unwrap()),
        &fixture,
        "edges",
        1,
        0.001,
    );
    let base = base.tiles.unwrap();
    check(&floats(&gpu, &base), &fixture, "base", 3, 0.1);
    let mut a = gpu.storage("Luminance A", 80 * 72 * 4).unwrap();
    let mut b = gpu.storage("Luminance B", 80 * 72 * 4).unwrap();
    let mut trace = gpu.trace(false, 5);
    gpu.dispatch(
        Kernel::NoiseLuminance,
        &mut [
            (0, Read(&input)),
            (1, Read(&uniform)),
            (2, Write(a.write())),
        ],
        groups(p),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseDiffuse,
        &mut [(0, Read(&a)), (1, Read(&uniform)), (2, Write(b.write()))],
        groups(p),
        &mut trace,
    );
    check(&floats(&gpu, &b), &fixture, "noise_diffusion", 1, 0.001);
    let mut fixed = p;
    fixed.diffusion[3] = 1.;
    let fixed = gpu.uniform(&fixed);
    gpu.dispatch(
        Kernel::NoiseDiffuse,
        &mut [(0, Read(&b)), (1, Read(&fixed)), (2, Write(a.write()))],
        groups(p),
        &mut trace,
    );
    check(&floats(&gpu, &a), &fixture, "fixed_diffusion", 1, 0.001);
    gpu.dispatch(
        Kernel::NoiseOsf,
        &mut [(0, Read(&a)), (1, Read(&uniform)), (2, Write(b.write()))],
        groups(p),
        &mut trace,
    );
    check(&floats(&gpu, &b), &fixture, "osf", 1, 0.001);
    let ratio = ratio(&gpu, &input, &base, p, &mut profile).unwrap();
    let gains = mix(&gpu, &input, &base, &ratio, p, &mut profile).unwrap();
    check(&floats(&gpu, &ratio), &fixture, "ratio", 1, 0.001);
    check(&floats(&gpu, &gains), &fixture, "gains", 3, 0.001);
    let sharp = signal(&gpu, &base, &ratio, p, &mut profile).unwrap();
    let signal_samples = floats(&gpu, &sharp);
    check(&signal_samples, &fixture, "signal", 1, 0.3);
    for y in 0..72 {
        for x in 0..80 {
            if x < 2 || x + 2 >= 80 || y < 2 || y + 2 >= 72 {
                assert_eq!(signal_samples[y * 80 + x], 0.);
            }
        }
    }
    let output = restore(&gpu, &base, &gains, p, &mut profile).unwrap();
    check(&floats(&gpu, &output), &fixture, "restored", 3, 0.2);
    assert_eq!(floats(&gpu, &input), source());
    // Detail zero retains the filtered base through the reference reconstruction clamps.
    let mut neutral = p;
    neutral.residual[3] = 0.;
    let neutral = mix(&gpu, &input, &base, &ratio, neutral, &mut profile).unwrap();
    assert!(floats(&gpu, &neutral).iter().all(|v| *v == 1.));
}

#[test]
#[ignore = "requires a GPU"]
fn processing_tile_projection_matches_reference_overlap_ramps() {
    let gpu = Gpu::new().unwrap();
    let mut p = parameters();
    p.source = [32, 24, 2, 2];
    p = p.full_frame();
    p.tile = [32, 28, 8, 11];
    let mut values = Vec::new();
    for tile in 0..4 {
        for y in 0..28 {
            for x in 0..32 {
                for c in 0..3 {
                    values.push((tile * 100 + x * 3 + y * 2 + c) as f32);
                }
            }
        }
    }
    let tiles = gpu.upload("Projection fixture", &values).unwrap();
    let mut profile = Profile::default();
    let output = project(&gpu, &tiles, p, 3, false, &mut profile).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../../../tests/data/noise-reference.json")).unwrap();
    let expected: Vec<f32> = fixture["projection"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    assert_eq!(floats(&gpu, &output), expected);
}

#[test]
#[ignore = "requires a GPU"]
fn compact_native_tiles_match_full_filters_and_projection_at_edges_and_seams() {
    let gpu = Gpu::new().unwrap();
    let mut p = parameters();
    p.source = [256, 256, 4, 4];
    p.tile = [192, 192, 64, 11];
    p.area = [0, 0, 256, 256];
    p.column_area = [0, 8, 256, 248];
    p = p.full_frame();
    let samples: Vec<f32> = (0..256 * 256 * 3)
        .map(|i| 300. + ((i * 17 + i / 768 * 23) % 109) as f32)
        .collect();
    let input = gpu
        .upload("Immutable native tile source", &samples)
        .unwrap();
    let render = |p: Parameters| {
        let mut profile = Profile::default();
        let base = base(&gpu, &input, p, false, true, true, &mut profile).unwrap();
        let base_tiles = base.tiles.as_ref().unwrap();
        let conditioned = column(&gpu, &input, p, &mut profile).unwrap();
        let ratio = ratio(&gpu, &conditioned, base_tiles, p, &mut profile).unwrap();
        let prepared = crate::nodes::color::pattern::Prepared::new(
            [p.tile[0], p.tile[1], p.tile_count(), 4],
            [0.1, 0.1, 0.5, 0.],
        )
        .unwrap();
        let ratio = crate::nodes::color::pattern::apply(
            &gpu,
            &ratio,
            base.seed.as_ref().unwrap(),
            &prepared,
            true,
            false,
            &mut profile,
        )
        .unwrap()
        .ratio
        .unwrap();
        let gains = mix(&gpu, &conditioned, base_tiles, &ratio, p, &mut profile).unwrap();
        let color = color(
            &gpu,
            base_tiles,
            ColorParams {
                source: [256, 256, 0, 0],
                matrix: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            },
            p,
            &mut profile,
        )
        .unwrap();
        let output = restore(&gpu, &color, &gains, p, &mut profile).unwrap();
        let signal = signal(&gpu, &color, &ratio, p, &mut profile).unwrap();
        (floats(&gpu, &output), floats(&gpu, &signal))
    };
    for scale in [1., 0.75, 0.5, 0.375] {
        p.reduction[0] = scale;
        let (full, full_signal) = render(p);
        for rect in [
            [0, 0, 24, 28],
            [30, 30, 50, 50],
            [60, 60, 72, 72],
            [125, 63, 132, 130],
            [232, 230, 256, 256],
        ] {
            let regional = p.for_region(rect).unwrap();
            assert!(regional.tile_count() < p.tile_count());
            let (detail, detail_signal) = render(regional);
            let width = (rect[2] - rect[0]) as usize;
            for y in rect[1]..rect[3] {
                for x in rect[0]..rect[2] {
                    let dst = (y - rect[1]) as usize * width + (x - rect[0]) as usize;
                    let src = (y * 256 + x) as usize;
                    assert_eq!(
                        &detail[dst * 3..dst * 3 + 3],
                        &full[src * 3..src * 3 + 3],
                        "scale {scale}, region {rect:?}, pixel {x},{y}"
                    );
                    assert_eq!(
                        detail_signal[dst], full_signal[src],
                        "signal scale {scale}, region {rect:?}, pixel {x},{y}"
                    );
                }
            }
        }
    }
    assert_eq!(floats(&gpu, &input), samples);
}
