//! Checks sparse gain and paired interpolation against independent scalar fixtures.
use super::*;
#[test]
fn rejects_unowned_patch_context() {
    assert!(Prepared::new([40, 40], [10, 11, 16, 32], [1, 1], vec![1.; 2], true).is_err());
    assert!(Prepared::new([40, 40], [11, 11, 16, 32], [1, 1], vec![1.; 2], true).is_ok());
}
#[test]
#[ignore = "requires a GPU"]
fn sparse_chroma_and_all_paired_luma_branches() {
    let gpu = Gpu::new().unwrap();
    let p = Prepared::new(
        [19, 17],
        [2, 3, 8, 8],
        [2, 2],
        vec![1.5, 0.5, 2., 3., 4., 5.],
        false,
    )
    .unwrap();
    let values = (0..19 * 17 * 3)
        .map(|v| v as f32 + 0.125)
        .collect::<Vec<_>>();
    let mut expected = values.clone();
    for y in [3, 11] {
        for (column, x) in [2, 10].into_iter().enumerate() {
            for channel in 0..3 {
                let i = (y * 19 + x) * 3 + channel;
                expected[i] *= p.gains[channel * 2 + column];
            }
        }
    }
    let input = gpu.upload("Sparse chroma fixture", &values).unwrap();
    let out = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
    assert_eq!(
        gpu.readback(&out).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&expected)
    );
    // Source-derived fixtures independently checked against the complete native
    // luma wrapper. Each targets a different branch of the paired interpolation.
    let patterns: [([f32; 12], bool); 6] = [
        (
            [
                -50., 20., 70., 100., 20., 200., 30., 30., 220., 10., 60., 90.,
            ],
            false,
        ),
        (
            [20., 20., 20., 21., 300., 22., 23., 300., 24., 25., 25., 25.],
            false,
        ),
        (
            [
                50., 100., 60., 80., 300., 180., 30., 300., 170., 20., 90., 60.,
            ],
            false,
        ),
        (
            [
                100., 200., 300., 400., 70., 500., 600., 80., 700., 800., 900., 1000.,
            ],
            true,
        ),
        (
            [
                0.01, 0.02, 0.03, 0.04, 0.05, 0.06, 0.07, 0.08, 0.09, 0.10, 0.11, 0.12,
            ],
            true,
        ),
        (
            [
                201., 70., 399., 266., 247., 130., 292., 263., 145., 291., 330., 160.,
            ],
            false,
        ),
    ];
    for (index, (pattern, masked)) in patterns.iter().enumerate() {
        let mut values = vec![100.; 40 * 40];
        for row in 0..4 {
            for col in 0..3 {
                values[(16 + row - 1) * 40 + 16 + col - 1] = pattern[row * 3 + col];
            }
        }
        let defects = if *masked {
            if index == 4 {
                vec![15 * 40 + 16, 16 * 40 + 15, 16 * 40 + 17, 17 * 40 + 15]
            } else {
                vec![15 * 40 + 16]
            }
        } else {
            vec![]
        };
        let p = Prepared::new([40, 40], [16, 16, 16, 32], [1, 1], vec![1.1, 0.9], true)
            .unwrap()
            .with_defects(defects.clone())
            .unwrap();
        let input = gpu.upload("Paired luma fixture", &values).unwrap();
        let out = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&out).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let expected = scalar_pair(&values, 40, 16, 16, &defects, [1.1, 0.9]);
        assert_eq!(
            [actual[16 * 40 + 16], actual[17 * 40 + 16]],
            expected,
            "pattern {index}"
        );
        for i in 0..values.len() {
            if i != 16 * 40 + 16 && i != 17 * 40 + 16 {
                assert_eq!(actual[i], values[i]);
            }
        }
    }
}
#[allow(clippy::approx_constant)] // Preserve the recovered Float32 diagonal distances.
fn scalar_pair(
    v: &[f32],
    w: usize,
    x: usize,
    y: usize,
    defects: &[u32],
    gain: [f32; 2],
) -> [f32; 2] {
    let at =
        |dx: isize, dy: isize| v[((y as isize + dy) as usize) * w + (x as isize + dx) as usize];
    let bad = |dx: isize, dy: isize| {
        u32::from(
            defects
                .contains(&(((y as isize + dy) as usize * w + (x as isize + dx) as usize) as u32)),
        )
    };
    let [a, b, c, d, e, f, g, h, i, j, k, l] =
        std::array::from_fn(|n| at(n as isize % 3 - 1, n as isize / 3 - 1));
    let n = bad(0, -1) + bad(-1, 0) + bad(1, 0) + bad(-1, 1) + bad(1, 1) + bad(0, 2);
    let horizontal = ((((((a - b).abs() + (b - c).abs()) + (d - f).abs() * 0.5)
        + (g - i).abs() * 0.5)
        + (j - k).abs())
        + (k - l).abs())
        / 6.;
    let vertical = ((((((a - d).abs() + (d - g).abs()) + (g - j).abs()) + (i - l).abs())
        + (f - i).abs())
        + (c - f).abs())
        / 6.;
    let median = |a: f32, b: f32, c: f32| a.min(b).max(a.max(b).min(c));
    if n == 0 && (vertical > horizontal * 2.2 || vertical > (e + h) * 0.45) {
        return [median(d, f, e * gain[0]), median(g, i, h * gain[1])];
    }
    if n == 0 && (vertical < horizontal * 0.5 || vertical < (e + h) * 0.1) {
        let diagonal = ((((g - c).abs() / 2.828427 + (b - d).abs() / 1.4142135)
            + (i - k).abs() / 1.4142135)
            + (j - f).abs() / 2.828427)
            * 0.25;
        let other = ((((b - f).abs() / 1.4142135 + (a - i).abs() / 2.828427)
            + (g - k).abs() / 1.4142135)
            + (d - l).abs() / 2.828427)
            * 0.25;
        if vertical.min(diagonal) <= other {
            if vertical <= diagonal {
                return [(k + b * 2.) / 3., (b + k * 2.) / 3.];
            }
            return [(c + g) * 0.5, (f + j) * 0.5];
        }
        return [(a + i) * 0.5, (d + l) * 0.5];
    }
    if n >= 4 {
        return [e * gain[0], h * gain[1]];
    }
    let offsets = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (1, 1),
        (-1, 2),
        (0, 2),
        (1, 2),
    ];
    let mut best = f32::MAX;
    let mut chosen = [e, h];
    for dy in -10isize..=10 {
        for dx in -10isize..=10 {
            if (dy.abs() <= 1 && dx == 0) || bad(dx, dy) != 0 || bad(dx, dy + 1) != 0 {
                continue;
            }
            let mut score = 0.;
            for (nx, ny) in offsets {
                let d = at(dx + nx, dy + ny) - at(nx, ny);
                let square = d * d;
                score = square.mul_add((1 - bad(nx, ny)) as f32, score);
            }
            if score < best {
                best = score;
                chosen = [at(dx, dy), at(dx, dy + 1)];
            }
        }
    }
    [
        chosen[0].clamp(b.min(d).min(f), 0f32.max(b).max(d).max(f)),
        chosen[1].clamp(g.min(i).min(k), 0f32.max(g).max(i).max(k)),
    ]
}
#[test]
#[ignore = "requires a GPU and FPPD_REAL_SITES"]
fn original_camera_diagonal_ties_match_native_wrapper() {
    let root = std::path::PathBuf::from(std::env::var_os("FPPD_REAL_SITES").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let bytes = std::fs::read(root.join(format!("{name}-input.bin"))).unwrap();
        let values = bytemuck::cast_slice::<u8, f32>(&bytes);
        let gains = case["gains"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        let defects = case["defects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        let p = Prepared::new([40, 40], [16, 16, 16, 32], [1, 1], gains, true)
            .unwrap()
            .with_defects(defects)
            .unwrap();
        let input = gpu.upload("Original FPPD neighborhood", values).unwrap();
        let output = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
        let result = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&result);
        assert_eq!(
            [actual[16 * 40 + 16], actual[17 * 40 + 16]],
            std::array::from_fn::<_, 2, _>(|i| case["expected"][i].as_f64().unwrap() as f32),
            "{name}"
        );
    }
}
