//! Checks reference-style shader arithmetic against synthetic fixtures from the read-only reference library.

use super::*;
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
        column: [0.375, 0., 0., 0.],
        reduction: [1., 0., 0., 0.],
        geometry: [8, 8, 21, 0],
    }
}
#[test]
#[ignore = "requires a GPU"]
fn reduced_local_and_global_bases_match_native_filters_restore_and_transposed_seed() {
    let f: Value =
        serde_json::from_str(include_str!("../tests/data/reduced-reference.json")).unwrap();
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
    let check = |buffer: &wgpu::Buffer, expected: Vec<f32>, tolerance: f32, label: &str| {
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
    let bilateral = gpu
        .storage("Global bilateral verification", 80 * 72 * 12)
        .unwrap();
    let radii = gpu.upload("Unused global radii", &[0_u32]).unwrap();
    let lookup = gpu.upload("Global lookup", &tables(p)).unwrap();
    let uniform = gpu.uniform(&p);
    let mut trace = gpu.trace(false, 1);
    gpu.dispatch(
        Kernel::NoiseBilateral,
        &[
            (0, &input),
            (1, &uniform),
            (2, &bilateral),
            (6, &lookup),
            (7, &radii),
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
        p.filter = [0.08, 0.8, 0.6, 0.95];
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
    p.filter = [0.08, 0.8, 0.6, 0.95];
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
fn column_conditioning_matches_native_blocks_borders_gain_and_clamps() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/data/column-reference.json")).unwrap();
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
fn floats(gpu: &Gpu, buffer: &wgpu::Buffer) -> Vec<f32> {
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
    let block = gpu.storage("Block means", 9 * 4).unwrap();
    let radii = gpu.storage("Radii", 9 * 4).unwrap();
    let mut profile = Profile::new(true);
    let mut trace = gpu.trace(true, 3);
    gpu.dispatch(
        Kernel::NoiseBlock,
        &[(0, &input), (1, &uniform), (4, &block)],
        [1, 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseRadius,
        &[(1, &uniform), (4, &block), (7, &radii)],
        [1, 1],
        &mut trace,
    );
    let filtered = gpu.storage("Bilateral fixture", 80 * 72 * 12).unwrap();
    gpu.dispatch(
        Kernel::NoiseBilateral,
        &[
            (0, &input),
            (1, &uniform),
            (2, &filtered),
            (6, &lookup),
            (7, &radii),
        ],
        bilateral_groups(p),
        &mut trace,
    );
    gpu.finish_trace(&trace, &mut profile).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/data/noise-reference.json")).unwrap();
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
    let a = gpu.storage("Luminance A", 80 * 72 * 4).unwrap();
    let b = gpu.storage("Luminance B", 80 * 72 * 4).unwrap();
    let mut trace = gpu.trace(false, 5);
    gpu.dispatch(
        Kernel::NoiseLuminance,
        &[(0, &input), (1, &uniform), (2, &a)],
        groups(p),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseDiffuse,
        &[(0, &a), (1, &uniform), (2, &b)],
        groups(p),
        &mut trace,
    );
    check(&floats(&gpu, &b), &fixture, "noise_diffusion", 1, 0.001);
    let mut fixed = p;
    fixed.diffusion[3] = 1.;
    let fixed = gpu.uniform(&fixed);
    gpu.dispatch(
        Kernel::NoiseDiffuse,
        &[(0, &b), (1, &fixed), (2, &a)],
        groups(p),
        &mut trace,
    );
    check(&floats(&gpu, &a), &fixture, "fixed_diffusion", 1, 0.001);
    gpu.dispatch(
        Kernel::NoiseOsf,
        &[(0, &a), (1, &uniform), (2, &b)],
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
        serde_json::from_str(include_str!("../tests/data/noise-reference.json")).unwrap();
    let expected: Vec<f32> = fixture["projection"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    assert_eq!(floats(&gpu, &output), expected);
}
