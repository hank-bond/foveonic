//! Checks parallel white-balance statistics against independent numerical fixtures.
use super::*;

#[test]
fn signed_black_reduction_preserves_samples_and_unequal_shields() {
    let gpu = Gpu::new().unwrap();
    let [w, h] = [1031_u32, 1031];
    let mut pixels = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            pixels.extend([((x * 17 + y * 31) % 6001) as f32 - 3000., 32767., -32768.]);
        }
    }
    let mut params: Params = bytemuck::Zeroable::zeroed();
    params.shape = [w, h, w, h];
    params.shields = [[3, 7, 9, 60], [20, 17, 31, 46]];
    params.flare = [0.25, -0.125, 0.5, 0.];
    params.options[0] = 1;
    let input = gpu.upload("signed black fixture", &pixels).unwrap();
    let sum = |positions: Vec<(u32, u32)>| {
        let mut sums = [0_f64; 3];
        for &(x, y) in &positions {
            for c in 0..3 {
                sums[c] += f64::from(pixels[((y * w + x) * 3) as usize + c]);
            }
        }
        sums.map(|v| (v / positions.len() as f64) as f32)
    };
    let sampled = sum((2..h)
        .step_by(4)
        .flat_map(|y| (2..w).step_by(4).map(move |x| (x, y)))
        .collect());
    let means = params.shields.map(|[x0, y0, x1, y1]| {
        sum((y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .collect())
    });
    let expected: [f32; 3] =
        std::array::from_fn(|c| sampled[c] * params.flare[c] + (means[0][c] + means[1][c]) * 0.5);
    let mut trace = gpu.trace(false, 1);
    let black = black_statistics(&gpu, &input, &params, &mut trace).unwrap();
    assert_eq!(&read_f32(&gpu, &black).unwrap()[..3], &expected);

    // A large red shield mean disables drift, independently of measured flare.
    params.shields = [[0, 0, 1, 1], [0, 0, 2, 2]];
    let bright = gpu
        .upload(
            "invalid shield fixture",
            &vec![5001_f32; (w * h * 3) as usize],
        )
        .unwrap();
    let black = black_statistics(&gpu, &bright, &params, &mut trace).unwrap();
    assert_eq!(
        &read_f32(&gpu, &black).unwrap()[..3],
        &[1250.25, -625.125, 2500.5]
    );
}

#[test]
fn candidate_scores_cover_all_candidates_and_only_first_quarter() {
    let gpu = Gpu::new().unwrap();
    let n = REDUCTION_CHUNK + 1;
    let mut params: Params = bytemuck::Zeroable::zeroed();
    params.shape = [n * 4, 1, n * 4, 1];
    params.camera = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
    params.control[0] = 1.;
    let mut pixels = vec![10000_f32; (n * 4 * 8) as usize];
    for i in 0..n as usize {
        pixels[i * 8..i * 8 + 3].copy_from_slice(&[0.1 + i as f32 * 0.001, 0.2, 0.3]);
    }
    let input = gpu.upload("candidate quarter fixture", &pixels).unwrap();
    let uniform = gpu.uniform(&params);
    let cube = gpu.upload("unused linear Lab lookup", &[0_f32]).unwrap();
    let chunks = n.div_ceil(REDUCTION_CHUNK);
    let mut output = gpu
        .storage("candidate partials", u64::from(chunks) * 48)
        .unwrap();
    let mut trace = gpu.trace(false, 1);
    gpu.dispatch(
        Kernel::WbScores,
        &mut [
            (0, Read(&input)),
            (1, Read(&uniform)),
            (2, Read(&cube)),
            (3, Write(output.write())),
        ],
        [chunks.div_ceil(64), 1],
        &mut trace,
    );
    let scores = combine_partials(&read_f32(&gpu, &output).unwrap(), 6);
    for c in 0..6 {
        let mut expected = 0_f64;
        for i in 0..n as usize {
            let xyz: [f64; 3] =
                std::array::from_fn(|k| f64::from(pixels[i * 8 + k]) * 0.0156 / CANDIDATES[c][k]);
            let lab = xyz.map(|v| v * 7.787 + 16. / 116.);
            expected += ((lab[0] - lab[1]) * 500.).hypot((lab[1] - lab[2]) * 200.);
        }
        assert!(
            (scores[c] - expected).abs() < 0.003,
            "candidate {c}: {} vs {expected}",
            scores[c]
        );
    }
}

#[test]
fn weighted_reduction_preserves_sparse_tail_and_boundary_weights() {
    let gpu = Gpu::new().unwrap();
    let n = REDUCTION_CHUNK * 2 + 1;
    let mut params: Params = bytemuck::Zeroable::zeroed();
    params.shape = [n, 1, n, 1];
    let mut pixels = vec![0_f32; (n * 8) as usize];
    let mut distances = vec![-1_f32; n as usize];
    let chosen = [
        (256_usize, [12000., -8000., 4000.]),
        (512, [-16000., 2000., 1000.]),
    ];
    for &(i, v) in &chosen {
        pixels[i * 8..i * 8 + 3].copy_from_slice(&v);
    }
    let input = gpu.upload("sparse weighted layers", &pixels).unwrap();
    let mut counts = vec![0_i32; 32768];
    counts[0] = 2;
    let histogram = gpu.upload("sparse weighted counts", &counts).unwrap();
    for (radius, at, coefficient) in [
        (30_f32, 30_f32, (1_f64 - (30_f64 / 45.).powi(3)).powi(3)),
        (45., 45., -1.),
    ] {
        params.control[1] = radius;
        for &(i, _) in &chosen {
            distances[i] = at * at;
        }
        let selection = gpu.upload("sparse tail distances", &distances).unwrap();
        let uniform = gpu.uniform(&params);
        let chunks = n.div_ceil(REDUCTION_CHUNK);
        let mut output = gpu
            .storage("weighted partials", u64::from(chunks) * 24)
            .unwrap();
        let mut trace = gpu.trace(false, 1);
        gpu.dispatch(
            Kernel::WbAccumulate,
            &mut [
                (0, Read(&input)),
                (1, Read(&uniform)),
                (2, Read(&selection)),
                (3, Write(output.write())),
                (5, Read(&histogram)),
            ],
            [chunks.div_ceil(64), 1],
            &mut trace,
        );
        let actual = combine_partials(&read_f32(&gpu, &output).unwrap(), 3);
        for (c, value) in actual.iter().enumerate() {
            let expected = (f64::from(chosen[0].1[c]) + f64::from(chosen[1].1[c])) * coefficient
                / 2_f64.sqrt();
            assert!(
                (*value - expected).abs() < 0.01,
                "radius {radius}, channel {c}: {} vs {expected}",
                value
            );
        }
    }
}
