//! Checks source highlight controls, directional reconstruction and green masks.
use super::*;
#[test]
fn singular_source_inverse_is_rejected() {
    assert!(reference_inverse([0.; 9]).is_err());
}

#[test]
#[ignore = "requires GPU"]
fn full_quattro_extreme_evidence_keeps_original_peaks_and_halo() {
    let gpu = Gpu::test_executor();
    let extent = [2944, 1836];
    let source: Vec<f32> = (0..extent[1])
        .flat_map(|y| {
            let value = if y < extent[1] / 2 { 1000. } else { 3000. };
            std::iter::repeat_n(value, extent[0] as usize * 3)
        })
        .collect();
    let input = gpu
        .upload("Extreme highlight evidence source", &source)
        .unwrap();
    let prepared = Prepared::new(
        extent,
        [1, 1, extent[0] - 1, extent[1] - 1],
        1.,
        [1.; 3],
        [1., 0., 0., 0., 1., 0., 0., 0., 1.],
    )
    .unwrap();
    let grid = crate::nodes::sensor::pool::Prepared::new(
        extent,
        1 << 31,
        crate::nodes::sensor::pool::Signal::Rgb,
    )
    .unwrap()
    .with_extent([136, 136])
    .unwrap()
    .with_origin([64, 64]);
    let output = prepared
        .evidence(grid)
        .unwrap()
        .apply(&gpu, &input, &mut Profile::default())
        .unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let values: &[[f32; 8]] = bytemuck::cast_slice(&bytes);
    for value in values {
        assert_eq!(&value[..4], &[3000., 3000., 3000., 0.]);
        // The independently normalized means cancel to zero within eight
        // float32 ULPs at the fixture's 2000-code mean.
        assert!(
            value[4..7].iter().all(|v| v.abs() <= 0.000_976_562_5),
            "{value:?}"
        );
        assert_eq!(value[7], 0.);
        assert_eq!(value.map(f32::to_bits), values[0].map(f32::to_bits));
    }
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&source)
    );
}

#[test]
#[ignore = "requires GPU"]
fn large_evidence_cells_match_ordered_single_pass_with_partial_edges() {
    use crate::nodes::sensor::pool::{Prepared as Grid, Signal};
    let gpu = Gpu::test_executor();
    let extent = [517_u32, 519_u32];
    let mut source = [1000_f32, 2000., 3000.].repeat((extent[0] * extent[1]) as usize);
    for [x, y] in [[200, 200], [516, 518]] {
        let index = ((y * extent[0] + x) * 3) as usize;
        source[index..index + 3].fill(14100.);
    }
    let input = gpu.upload("Large evidence partial cells", &source).unwrap();
    let prepared = Prepared::new(
        extent,
        [1, 1, extent[0] - 1, extent[1] - 1],
        1.,
        [1., 1., 2.],
        [2., 0., 0., 0., 2., 0., 0., 0., 2.],
    )
    .unwrap();
    for divisor in [256, 512] {
        let base = Grid::new(extent, divisor, Signal::Rgb).unwrap();
        let logical = base.logical_extent();
        let grid = base
            .with_extent(logical.map(|side| side + 6))
            .unwrap()
            .with_origin([3, 3]);
        let before = gpu.activity();
        let actual = prepared
            .evidence(grid)
            .unwrap()
            .apply(&gpu, &input, &mut Profile::default())
            .unwrap();
        assert_eq!(
            gpu.activity().since(before).dispatches,
            if divisor == 256 { 1 } else { 2 }
        );

        // Run the original ordered cell shader directly as an independent path
        // for the same boundary-spanning, partially covered cells.
        let [w, h] = grid.output_extent();
        let mut expected = gpu
            .storage("Single-pass evidence", u64::from(w) * u64::from(h) * 32)
            .unwrap();
        let params = gpu.uniform(&prepared.params);
        let matrix = gpu
            .upload("Single-pass matrices", &prepared.matrix)
            .unwrap();
        let uniform = gpu.uniform(&grid);
        let mut trace = gpu.trace(false, 1);
        gpu.dispatch(
            Kernel::HighlightEvidence,
            &mut [
                (0, Read(&input)),
                (1, Read(&params)),
                (2, Read(&matrix)),
                (4, Write(expected.write())),
                (9, Read(&uniform)),
            ],
            [logical[0].div_ceil(8), logical[1].div_ceil(8)],
            &mut trace,
        );
        let actual = gpu.readback(&actual).unwrap();
        let expected = gpu.readback(&expected).unwrap();
        assert_eq!(actual, expected, "divisor {divisor}");
        let values: &[[f32; 8]] = bytemuck::cast_slice(&actual);
        assert!(
            values.iter().any(|v| v[3] > 0.),
            "fixture must include detector coverage"
        );
        assert!(
            values.iter().any(|v| v[4..7].iter().any(|v| *v != 0.)),
            "fixture must include highlight correction"
        );
    }
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&source)
    );
}

#[test]
#[ignore = "requires GPU"]
fn repaired_speckles_do_not_regain_original_highlight_damage() {
    let gpu = Gpu::test_executor();
    let grid = crate::render::WorkingGrid::new([64, 64], crate::render::Scale::new(4).unwrap(), 8)
        .unwrap()
        .with_halo(4)
        .unwrap();
    let [w, h] = grid.extent;
    let center = (h / 2 * w + w / 2) as usize;
    let repaired = vec![1000_f32; (w * h * 3) as usize];
    let mut before = repaired.clone();
    before[center * 3..center * 3 + 3].fill(6000.);
    let mut evidence = vec![[0_f32; 8]; (w * h) as usize];
    evidence[center] = [60000., 60000., 60000., 1., -5000., -5000., -5000., 255.];
    let input = gpu.upload("Repaired bright defect", &repaired).unwrap();
    let original = gpu.upload("Before speckle repair", &before).unwrap();
    let controls = gpu.upload("Original defect evidence", &evidence).unwrap();
    for mode in [DIRECTIONAL_ONLY, CHANNEL_EXTENSION, CHANNEL_LIMITS_ONLY] {
        let mut prepared = Prepared::new(
            [64, 64],
            [1, 1, 63, 63],
            1.,
            [1.; 3],
            [1., 0., 0., 0., 1., 0., 0., 0., 1.],
        )
        .unwrap();
        if mode != DIRECTIONAL_ONLY {
            prepared = prepared
                .with_channel_limits([14000.; 3], 13000., mode == CHANNEL_EXTENSION)
                .unwrap();
        }
        let prepared = prepared.on_grid(grid).unwrap();
        let output = apply(
            &gpu,
            &input,
            Some(EvidenceInput {
                controls: &controls,
                unfiltered: &original,
            }),
            &prepared,
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output.rgb).unwrap();
        assert_eq!(
            bytemuck::cast_slice::<u8, f32>(&bytes),
            repaired,
            "mode{mode}"
        );
        let bytes = gpu.readback(&output.mask).unwrap();
        assert!(bytemuck::cast_slice::<u8, u32>(&bytes)
            .iter()
            .all(|v| *v == 0));
    }
}

#[test]
#[ignore = "requires GPU"]
fn pooled_detector_coverage_matches_native_classification() {
    let gpu = Gpu::test_executor();
    let extent = [33, 25];
    let cells = extent[0] * extent[1];
    let values: Vec<f32> = (0..cells * 3)
        .map(|i| ((i * 197 + 31) % 17000) as f32)
        .collect();
    let input = gpu
        .upload("Native clipping boundary fixture", &values)
        .unwrap();
    let prepared = Prepared::new(
        extent,
        [1, 1, 32, 24],
        1.,
        [1., 1., 2.],
        [1., 0., 0., 0., 1., 0., 0., 0., 1.],
    )
    .unwrap();
    let params = gpu.uniform(&prepared.params);
    let matrix = gpu
        .upload("Native detector matrices", &prepared.matrix)
        .unwrap();
    let mut work = gpu
        .storage("Native detector work", u64::from(cells) * 13 * 4)
        .unwrap();
    let mut mask = gpu
        .storage("Native green detection", u64::from(cells) * 4)
        .unwrap();
    let mut trace = gpu.trace(false, 2);
    gpu.dispatch(
        Kernel::HighlightPrepare,
        &mut [
            (0, Read(&input)),
            (1, Read(&params)),
            (2, Read(&matrix)),
            (3, Write(work.write())),
            (5, Write(mask.write())),
            (8, Read(&input)),
            (10, Read(&input)),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::HighlightClassify,
        &mut [
            (0, Read(&input)),
            (1, Read(&params)),
            (3, Write(work.write())),
            (8, Read(&input)),
            (10, Read(&input)),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    let bytes = gpu.readback(&work).unwrap();
    let work: &[f32] = bytemuck::cast_slice(&bytes);
    let bytes = gpu.readback(&mask).unwrap();
    let mask: &[u32] = bytemuck::cast_slice(&bytes);
    for divisor in [2, 4, 8, 64] {
        let logical = extent.map(|v| v.div_ceil(divisor));
        let output_extent = logical.map(|v| v.div_ceil(8) * 8);
        let grid = crate::nodes::sensor::pool::Prepared::new(
            extent,
            divisor,
            crate::nodes::sensor::pool::Signal::Rgb,
        )
        .unwrap()
        .with_extent(output_extent)
        .unwrap();
        let output = prepared
            .evidence(grid)
            .unwrap()
            .apply(&gpu, &input, &mut Profile::default())
            .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[[f32; 8]] = bytemuck::cast_slice(&bytes);
        for y in 0..output_extent[1] {
            for x in 0..output_extent[0] {
                let first = [
                    x.min(logical[0] - 1) * divisor,
                    y.min(logical[1] - 1) * divisor,
                ];
                let end = [
                    (first[0] + divisor).min(extent[0]),
                    (first[1] + divisor).min(extent[1]),
                ];
                let mut total = 0.;
                let mut green = 0;
                for yy in first[1]..end[1] {
                    for xx in first[0]..end[0] {
                        let i = (yy * extent[0] + xx) as usize;
                        total += work[cells as usize * 6 + i];
                        green = green.max(mask[i]);
                    }
                }
                let expected = total / ((end[0] - first[0]) * (end[1] - first[1])) as f32;
                let value = actual[(y * output_extent[0] + x) as usize];
                assert!(
                    (value[3] - expected).abs() < 1e-6,
                    "divisor{divisor} cell{x},{y}: {} vs {expected}",
                    value[3]
                );
                assert_eq!(value[7], green as f32);
            }
        }
    }
}

#[test]
fn evidence_rejects_grid_origins_outside_output() {
    use crate::nodes::sensor::pool::{Prepared as Grid, Signal};
    let extent = [17, 13];
    let prepared = Prepared::new(
        extent,
        [1, 1, 16, 12],
        1.,
        [1., 1., 2.],
        [2., 0., 0., 0., 2., 0., 0., 0., 2.],
    )
    .unwrap();
    let grid = Grid::new(extent, 4, Signal::Rgb).unwrap();
    assert!(prepared.evidence(grid).is_ok());
    assert!(prepared.evidence(grid.with_origin([1, 0])).is_err());
    assert!(prepared.evidence(grid.with_origin([0, u32::MAX])).is_err());
}

#[test]
#[ignore = "requires GPU"]
fn pooled_evidence_preserves_partial_cell_peaks_and_native_control_coverage() {
    let gpu = Gpu::test_executor();
    let extent = [17, 13];
    let mut pixels = [1000_f32, 2000., 3000.].repeat((extent[0] * extent[1]) as usize);
    pixels[..3].fill(14100.);
    let last = pixels.len() - 3;
    pixels[last..].fill(14100.);
    let input = gpu
        .upload("Mixed native highlight coverage", &pixels)
        .unwrap();
    let prepared = Prepared::new(
        extent,
        [1, 1, 16, 12],
        1.,
        [1., 1., 2.],
        [2., 0., 0., 0., 2., 0., 0., 0., 2.],
    )
    .unwrap();
    for divisor in [4, 16, 64] {
        let logical = extent.map(|v| v.div_ceil(divisor));
        let out = logical.map(|v| (v + 6).div_ceil(8) * 8);
        let grid = crate::nodes::sensor::pool::Prepared::new(
            extent,
            divisor,
            crate::nodes::sensor::pool::Signal::Rgb,
        )
        .unwrap()
        .with_extent(out)
        .unwrap()
        .with_origin([3; 2]);
        let output = prepared
            .evidence(grid)
            .unwrap()
            .apply(&gpu, &input, &mut Profile::default())
            .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[[f32; 8]] = bytemuck::cast_slice(&bytes);
        for y in 0..out[1] {
            for x in 0..out[0] {
                let cell = [
                    x.saturating_sub(3).min(logical[0] - 1),
                    y.saturating_sub(3).min(logical[1] - 1),
                ];
                let first = cell.map(|v| v * divisor);
                let end = [
                    (first[0] + divisor).min(extent[0]),
                    (first[1] + divisor).min(extent[1]),
                ];
                let count = (end[0] - first[0]) * (end[1] - first[1]);
                let hot = u32::from(first == [0, 0]) + u32::from(end == extent);
                let peak = if hot > 0 {
                    [14100.; 3]
                } else {
                    [1000., 2000., 3000.]
                };
                let value = actual[(y * out[0] + x) as usize];
                assert_eq!(&value[..3], &peak);
                let marked = [[1, 1], [15, 11]]
                    .into_iter()
                    .filter(|point| {
                        point[0] >= first[0]
                            && point[0] < end[0]
                            && point[1] >= first[1]
                            && point[1] < end[1]
                    })
                    .count();
                assert!(
                    (value[3] - marked as f32 / count as f32).abs() < 1e-6,
                    "cell{x},{y}: {value:?}"
                );
                for c in 0..3 {
                    let base = [1000_f64, 2000., 3000.][c];
                    let mean = (f64::from(count - hot) * base + f64::from(hot) * 14100.)
                        / f64::from(count);
                    let corrected =
                        (f64::from(count - hot) * base + f64::from(hot) * 7000.) / f64::from(count);
                    let expected = (corrected - mean.min(14000.)) * [1., 1., 2.][c];
                    assert!(
                        (f64::from(value[c + 4]) - expected).abs() < 0.005,
                        "cell{x},{y} channel{c}: {value:?} expected {expected}"
                    );
                }
            }
        }
    }
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&pixels)
    );
}

#[test]
#[ignore = "requires GPU"]
fn reduced_recovery_uses_original_clipping_below_the_averaged_threshold() {
    let gpu = Gpu::new().unwrap();
    let grid = crate::render::WorkingGrid::new([64, 64], crate::render::Scale::new(4).unwrap(), 8)
        .unwrap()
        .with_halo(4)
        .unwrap();
    let prepared = Prepared::new(
        [64, 64],
        [1, 1, 63, 63],
        1.,
        [1.; 3],
        [1., 0., 0., 0., 1., 0., 0., 0., 1.],
    )
    .unwrap()
    .with_channel_limits([14000.; 3], 13000., true)
    .unwrap()
    .on_grid(grid)
    .unwrap();
    let [w, h] = grid.extent;
    let numerical = [1000_f32, 2000., 3000.].repeat((w * h) as usize);
    let mut evidence: Vec<[f32; 8]> = numerical
        .as_chunks::<3>()
        .0
        .iter()
        .map(|v| [v[0], v[1], v[2], 0., 0., 0., 0., 0.])
        .collect();
    let center = (h / 2 * w + w / 2) as usize;
    evidence[center][2] = 14000.;
    let input = gpu
        .upload("Pooled healthy and clipped means", &numerical)
        .unwrap();
    let peaks = gpu.upload("Original channel peaks", &evidence).unwrap();
    let output = apply(
        &gpu,
        &input,
        Some(EvidenceInput {
            controls: &peaks,
            unfiltered: &input,
        }),
        &prepared,
        &mut Profile::default(),
    )
    .unwrap();
    let flags = gpu.readback(&output.recovery_mask.unwrap()).unwrap();
    let flags: &[u32] = bytemuck::cast_slice(&flags);
    assert_eq!(
        flags[center], 4,
        "the average must not erase original blue clipping"
    );
    assert!(flags
        .iter()
        .enumerate()
        .all(|(i, flags)| i == center || *flags == 0));
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&numerical)
    );
    assert_eq!(
        gpu.readback(&peaks).unwrap(),
        bytemuck::cast_slice::<[f32; 8], u8>(&evidence)
    );
    let bytes = gpu.readback(&output.rgb).unwrap();
    assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
        .iter()
        .all(|v| v.is_finite()));
    assert!(apply(&gpu, &input, None, &prepared, &mut Profile::default()).is_err());
}

#[test]
#[ignore = "requires GPU"]
fn partial_color_cells_keep_edge_recovery_and_original_samples() {
    let gpu = Gpu::test_executor();
    for [width, height] in [[35, 33], [50, 35]] {
        let mut source = [1000_f32, 2000., 3000.].repeat(width * height);
        for y in 0..height {
            source[(y * width + width - 1) * 3 + 2] = 14000.;
        }
        let input = gpu
            .upload("Partial-cell highlight fixture", &source)
            .unwrap();
        let prepared = Prepared::new(
            [width as u32, height as u32],
            [1, 1, width as u32 - 1, height as u32 - 1],
            1.,
            [1.; 3],
            [1., 0., 0., 0., 1., 0., 0., 0., 1.],
        )
        .unwrap()
        .with_channel_limits([14000.; 3], 13000., true)
        .unwrap();
        let output = apply(&gpu, &input, None, &prepared, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output.recovery_mask.unwrap()).unwrap();
        let masks: &[u32] = bytemuck::cast_slice(&bytes);
        for y in 0..height {
            for x in 0..width {
                assert_eq!(masks[y * width + x], if x == width - 1 { 4 } else { 0 });
            }
        }
        let bytes = gpu.readback(&output.rgb).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .all(|v| v.is_finite()));
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&source)
        );
    }
}
#[test]
#[ignore = "requires GPU and HIGHLIGHT_REFERENCE"]
fn enabled_restoration_matches_independent_equations() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("HIGHLIGHT_REFERENCE").expect("set HIGHLIGHT_REFERENCE"),
    );
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    let read = |name: String| -> Vec<f32> {
        let b = std::fs::read(directory.join(name)).unwrap();
        b.as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect()
    };
    let mut maximum = 0_f32;
    for c in cases.as_array().unwrap() {
        let n = c["n"].as_u64().unwrap();
        let array = |name: &str| {
            c[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<_>>()
        };
        let extent = array("extent");
        let [w, h] = [extent[0] as u32, extent[1] as u32];
        let prepared = Prepared::new(
            [w, h],
            [1, 1, w - 1, h - 1],
            c["normalization"].as_f64().unwrap() as f32,
            array("gains")
                .into_iter()
                .map(|v| v as f32)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            array("matrix").try_into().unwrap(),
        )
        .unwrap();
        let input = gpu
            .upload(
                "Independent highlight input",
                &read(format!("{n}-input.f32")),
            )
            .unwrap();
        let output = apply(&gpu, &input, None, &prepared, &mut Default::default()).unwrap();
        let b = gpu.readback(&output.rgb).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&b);
        let expected = read(format!("{n}-output.f32"));
        for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
            let e = (a - b).abs();
            maximum = maximum.max(e);
            assert!(
                e <= 0.01_f32.max(b.abs() * f32::EPSILON * 16.),
                "case{n} sample{i} {a} vs {b}"
            );
        }
        let bytes = gpu.readback(&output.mask).unwrap();
        let actual = bytemuck::cast_slice::<u8, u32>(&bytes);
        let expected = std::fs::read(directory.join(format!("{n}-mask.u8"))).unwrap();
        assert!(actual
            .iter()
            .copied()
            .eq(expected.into_iter().map(u32::from)));
    }
    eprintln!("Independent highlight max{maximum}");
}

#[test]
#[ignore = "requires GPU and TIGER_HIGHLIGHT_REFERENCE"]
fn channel_recovery_matches_independent_complete_highlight_equations() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("TIGER_HIGHLIGHT_REFERENCE").expect("set TIGER_HIGHLIGHT_REFERENCE"),
    );
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let n = case["n"].as_u64().unwrap();
        let extent = std::array::from_fn(|i| case["extent"][i].as_u64().unwrap() as u32);
        let gains = std::array::from_fn(|i| case["gains"][i].as_f64().unwrap() as f32);
        let scale = case["scale"].as_f64().unwrap() as f32;
        let active = case["active"].as_bool().unwrap();
        let ceilings = if active && scale == 1. {
            [22000., 24000., 14000.]
        } else {
            [14000. / scale; 3]
        };
        let green = if active && scale == 1. {
            24000.
        } else {
            13000. / scale
        };
        let prepared = Prepared::new(
            extent,
            [1, 1, extent[0] - 1, extent[1] - 1],
            scale,
            gains,
            std::array::from_fn(|i| case["matrix"][i].as_f64().unwrap()),
        )
        .unwrap()
        .with_channel_limits(ceilings, green, active)
        .unwrap();
        let bytes = std::fs::read(directory.join(format!("{n}-input.bin"))).unwrap();
        let input = gpu
            .upload(
                "Independent channel highlight input",
                bytemuck::cast_slice::<u8, f32>(&bytes),
            )
            .unwrap();
        let output = apply(&gpu, &input, None, &prepared, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output.rgb).unwrap();
        let expected = std::fs::read(directory.join(format!("{n}-output.bin"))).unwrap();
        let mut maximum = 0_f32;
        for (i, (&a, &b)) in bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .zip(bytemuck::cast_slice::<u8, f32>(&expected))
            .enumerate()
        {
            maximum = maximum.max((a - b).abs());
            assert!(
                a.is_finite() && (a - b).abs() <= 0.03_f32.max(b.abs() * f32::EPSILON * 32.),
                "case{n} sample{i}: {a} vs {b}"
            );
        }
        eprintln!("Channel highlight case{n}: maximum error {maximum}");
        for (tag, buffer) in [
            ("green", output.mask),
            ("recovery", output.recovery_mask.unwrap()),
        ] {
            let actual = gpu.readback(&buffer).unwrap();
            let expected = std::fs::read(directory.join(format!("{n}-{tag}.bin"))).unwrap();
            assert_eq!(actual, expected, "case{n} {tag} mask");
        }
    }
}
