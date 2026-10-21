//! Estimates a camera-layer neutral from reduced pixels and chroma-weighted histograms.
//! Equations follow the recovered reference implementation. Image planes stay on the GPU.
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, GpuTrace, Kernel, ReadGuard,
    },
    nodes::finish::signed_response,
    profile::Profile,
    source::Result,
};
#[path = "weighted_white_balance_stats.rs"]
mod stats;

#[derive(Clone)]
pub(crate) struct Prepared {
    /// Original layer dimensions in pixels.
    pub extent: [u32; 2],
    pub camera: [f64; 9],
    pub tone: [f64; 9],
    pub saturation: [f64; 3],
    /// Exposure-derived brightness used by the illuminant constraints.
    pub brightness: f64,
    pub scale: f64,
    pub flash: bool,
    pub constrained: bool,
    pub shields: [[u32; 4]; 2],
    pub estimate_drift: bool,
    pub drift: [f32; 3],
    pub flare: [f32; 3],
    /// Signed Q14 polynomial coefficients, packed as three padded rows per channel.
    pub polynomial: [[i32; 4]; 9],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    sampling: [u32; 4],
    camera: [[f32; 4]; 3],
    tone: [[f32; 4]; 3],
    saturation: [f32; 4],
    control: [f32; 4],
    white: [f32; 4],
    minimum: [i32; 4],
    maximum: [i32; 4],
    shields: [[u32; 4]; 2],
    drift: [f32; 4],
    flare: [f32; 4],
    polynomial: [[i32; 4]; 9],
    options: [u32; 4],
}
pub(super) fn transform(m: [f64; 9], v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|r| v[1] * m[r * 3 + 1] + v[0] * m[r * 3] + v[2] * m[r * 3 + 2])
}
pub(crate) fn multiply(a: [f64; 9], b: [f64; 9]) -> [f64; 9] {
    std::array::from_fn(|i| (0..3).map(|k| a[i / 3 * 3 + k] * b[k * 3 + i % 3]).sum())
}
pub(super) fn inverse(m: [f64; 9]) -> [f64; 9] {
    let [a, b, c, d, e, f, g, h, i] = m;
    let co = [
        e * i - f * h,
        c * h - b * i,
        b * f - c * e,
        f * g - d * i,
        a * i - c * g,
        c * d - a * f,
        d * h - e * g,
        b * g - a * h,
        a * e - b * d,
    ];
    let det = a * co[0] + b * co[3] + c * co[6];
    co.map(|x| x / det)
}
const CANDIDATES: [[f64; 3]; 6] = [
    [94.9466, 100., 138.4398],
    [94.38, 100., 117.898],
    [95.0456, 100., 108.9058],
    [95.5475, 100., 77.986],
    [102.8524, 100., 49.5528],
    [108.854, 100., 33.4394],
];
fn read_i32(gpu: &Gpu, value: &ReadGuard) -> Result<Vec<i32>> {
    let bytes = gpu.readback(value)?;
    Ok(bytemuck::cast_slice::<u8, i32>(&bytes).to_vec())
}
fn read_f32(gpu: &Gpu, value: &ReadGuard) -> Result<Vec<f32>> {
    let bytes = gpu.readback(value)?;
    Ok(bytemuck::cast_slice::<u8, f32>(&bytes).to_vec())
}

// Bounded chunks avoid image-wide Float32 accumulation error. Original 16-bit
// black samples sum exactly within each chunk. Weighted sums use compensation.
const REDUCTION_CHUNK: u32 = 256;

fn black_statistics(
    gpu: &Gpu,
    input: &ReadGuard,
    params: &Params,
    trace: &mut GpuTrace,
) -> Result<ReadGuard> {
    let sampled = (params.shape[0] + 1) / 4 * ((params.shape[1] + 1) / 4);
    let lengths: [u32; 3] = std::array::from_fn(|region| {
        if region == 0 {
            if params.flare[..3].iter().any(|v| *v != 0.) {
                sampled
            } else {
                0
            }
        } else if params.options[0] != 0 {
            let r = params.shields[region - 1];
            (r[2] - r[0]) * (r[3] - r[1])
        } else {
            0
        }
    });
    let chunks = lengths.into_iter().max().unwrap().div_ceil(REDUCTION_CHUNK);
    if chunks == 0 {
        let fixed = if params.options[0] != 0 {
            [0.; 4]
        } else {
            params.drift
        };
        return gpu.upload("White-balance fixed black offsets", &fixed);
    }
    let uniform = gpu.uniform(params);
    let mut partials = gpu.storage("White-balance partial black sums", u64::from(chunks) * 48)?;
    gpu.dispatch(
        Kernel::WbBlack,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (3, Write(partials.write())),
        ],
        [chunks.div_ceil(64), 1],
        trace,
    );
    let values = read_f32(gpu, &partials)?;
    let mut sums = [[0_f64; 3]; 3];
    for chunk in values.as_chunks::<12>().0 {
        for region in 0..3 {
            for c in 0..3 {
                sums[region][c] += f64::from(chunk[region * 4 + c]);
            }
        }
    }
    let means = std::array::from_fn::<_, 3, _>(|region| {
        sums[region].map(|v| (v / f64::from(lengths[region].max(1))) as f32)
    });
    let drift = if params.options[0] != 0 {
        if means[1][0].abs() > 4000. {
            [0.; 3]
        } else {
            std::array::from_fn(|c| (means[1][c] + means[2][c]) * 0.5)
        }
    } else {
        [params.drift[0], params.drift[1], params.drift[2]]
    };
    let black: [f32; 4] = std::array::from_fn(|c| {
        if c < 3 {
            means[0][c] * params.flare[c] + drift[c]
        } else {
            0.
        }
    });
    gpu.upload("White-balance black offsets", &black)
}

// Partial compensated sums are compact statistics, not image readback. Combining
// them in f64 limits rounding error across the fixed chunk order.
fn combine_partials(values: &[f32], channels: usize) -> Vec<f64> {
    let mut sum = vec![0.; channels];
    for chunk in values.chunks_exact(channels * 2) {
        for c in 0..channels {
            sum[c] += f64::from(chunk[c]) - f64::from(chunk[channels + c]);
        }
    }
    sum
}

/// Measures the original signed layers after the constant mean drift offset.
/// Only integer row statistics leave the GPU; the reference averages rows in f64.
pub(crate) fn mean(
    gpu: &Gpu,
    input: &ReadGuard,
    extent: [u32; 2],
    shields: [[u32; 4]; 2],
    estimate_drift: bool,
    drift: [f32; 3],
    profile: &mut Profile,
) -> Result<[f64; 3]> {
    let [w, h] = extent;
    if w == 0 || h == 0 {
        return Err("camera-layer statistics require nonempty pixels".into());
    }
    let mut p: Params = bytemuck::Zeroable::zeroed();
    p.shape = [w, h, w, h];
    p.shields = shields;
    p.drift = [drift[0], drift[1], drift[2], 0.];
    p.options[0] = u32::from(estimate_drift);
    let uniform = gpu.uniform(&p);
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    let black = black_statistics(gpu, input, &p, &mut trace)?;
    let mut rows = gpu.storage("Camera-layer integer row sums", u64::from(h) * 12)?;
    gpu.dispatch(
        Kernel::WbMean,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&black)),
            (3, Write(rows.write())),
        ],
        [h.div_ceil(64), 1],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let sums = read_i32(gpu, &rows)?;
    let mut result = [0.; 3];
    for row in sums.as_chunks::<3>().0 {
        for c in 0..3 {
            result[c] += f64::from(row[c]) / f64::from(w);
        }
    }
    Ok(result.map(|v| v / f64::from(h)))
}

pub(crate) fn estimate(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<[f64; 3]> {
    let [w, h] = p.extent;
    let large = w > 500;
    let margins = if large {
        [(w / 100 + 3) & !3, (h / 100 + 3) & !3]
    } else {
        [0; 2]
    };
    let [rw, rh] = if large {
        [(w - 2 * margins[0]) / 4, (h - 2 * margins[1]) / 8]
    } else {
        [w, h]
    };
    if rw < 32 || rh < 40 {
        return Err("white-balance estimator needs a working image at least 32 by 40".into());
    }
    let sample_w = (rw - 31) / 4 + 1;
    let sample_h = (rh - 39) / 4 + 1;
    let mut params = Params {
        shape: [w, h, rw, rh],
        sampling: [margins[0], margins[1], sample_w, sample_h],
        camera: std::array::from_fn(|r| {
            [
                p.camera[r * 3] as f32,
                p.camera[r * 3 + 1] as f32,
                p.camera[r * 3 + 2] as f32,
                0.,
            ]
        }),
        tone: std::array::from_fn(|r| {
            [
                p.tone[r * 3] as f32,
                p.tone[r * 3 + 1] as f32,
                p.tone[r * 3 + 2] as f32,
                0.,
            ]
        }),
        saturation: [
            p.saturation[0] as f32,
            p.saturation[1] as f32,
            p.saturation[2] as f32,
            0.,
        ],
        control: [p.scale as f32, 45., 0., 0.],
        white: [0.; 4],
        minimum: [0; 4],
        maximum: [0; 4],
        shields: p.shields,
        drift: [p.drift[0], p.drift[1], p.drift[2], 0.],
        flare: [p.flare[0], p.flare[1], p.flare[2], 0.],
        polynomial: p.polynomial,
        options: [
            u32::from(p.estimate_drift),
            u32::from(p.polynomial.iter().flatten().any(|x| *x != 0)),
            u32::from(large),
            0,
        ],
    };
    let uniform = gpu.uniform(&params);
    let root = signed_response::root();
    let root_gpu = gpu.upload("White-balance root table", &root)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 6);
    let black = black_statistics(gpu, input, &params, &mut trace)?;
    let mut work = gpu.storage("White-balance reduced layers", u64::from(rw * rh) * 32)?;
    let mut maxima = gpu.upload("White-balance maxima", &[0_i32; 8])?;
    gpu.dispatch(
        Kernel::WbReduce,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&black)),
            (3, Write(work.write())),
            (5, Write(maxima.write())),
        ],
        [rw.div_ceil(8), rh.div_ceil(8)],
        &mut trace,
    );
    let mut samples = gpu.storage(
        "White-balance tone samples",
        u64::from(sample_w * sample_h) * 16,
    )?;
    gpu.dispatch(
        Kernel::WbSample,
        &mut [
            (0, Read(&work)),
            (1, Read(&uniform)),
            (3, Write(samples.write())),
            (5, Write(maxima.write())),
        ],
        [sample_w.div_ceil(8), sample_h.div_ceil(8)],
        &mut trace,
    );
    // Eight words describe the tone maxima and measured channel saturation.
    let max_words = read_i32(gpu, &maxima)?;
    let tone_max = std::array::from_fn(|i| f64::from(max_words[i].max(1)));
    for c in 0..3 {
        params.saturation[c] = params.saturation[c].min(f32::from_bits(max_words[3 + c] as u32));
    }
    params.maximum = std::array::from_fn(|i| if i < 3 { max_words[i].max(1) } else { 0 });
    let uniform = gpu.uniform(&params);
    let mut hist = gpu.upload(
        "White-balance auto-tone histograms",
        &vec![0_i32; 32768 + 2048],
    )?;
    gpu.dispatch(
        Kernel::WbAutoHistogram,
        &mut [
            (0, Read(&samples)),
            (1, Read(&uniform)),
            (4, Read(&root_gpu)),
            (5, Write(hist.write())),
        ],
        [sample_w.div_ceil(8), sample_h.div_ceil(8)],
        &mut trace,
    );
    // Read compact 32³ color and 2048-bin luma histograms, rather than image pixels.
    let hist_words = read_i32(gpu, &hist)?;
    let curve = stats::tone(&hist_words[..32768], &hist_words[32768..], tone_max);
    let mut tables = root;
    tables.extend(curve);
    let tables = gpu.upload("White-balance sampled tone curves", &tables)?;
    let mut toned = gpu.storage(
        "White-balance tone-adjusted layers",
        u64::from(rw * rh) * 32,
    )?;
    gpu.dispatch(
        Kernel::WbTone,
        &mut [
            (0, Read(&work)),
            (1, Read(&uniform)),
            (3, Write(toned.write())),
            (4, Read(&tables)),
        ],
        [rw.div_ceil(8), rh.div_ceil(8)],
        &mut trace,
    );
    let mut cube = Vec::with_capacity(10000);
    let mut coordinate = -1_f64;
    for _ in 0..10000 {
        cube.push(coordinate.cbrt() as f32);
        coordinate += 0.0016;
    }
    let cube = gpu.upload("White-balance Lab lookup", &cube)?;
    let score_chunks = (rw * rh / 4).div_ceil(REDUCTION_CHUNK);
    let mut score = gpu.storage(
        "White-balance candidate score partials",
        u64::from(score_chunks) * 48,
    )?;
    gpu.dispatch(
        Kernel::WbScores,
        &mut [
            (0, Read(&toned)),
            (1, Read(&uniform)),
            (2, Read(&cube)),
            (3, Write(score.write())),
        ],
        [score_chunks.div_ceil(64), 1],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    // Preserve the original Float32 scores and candidate-policy arithmetic.
    let scores: Vec<_> = combine_partials(&read_f32(gpu, &score)?, 6)
        .into_iter()
        .map(|sum| sum as f32 / (rw * rh / 4) as f32)
        .collect();
    let mut selected = (0..6)
        .min_by(|a, b| scores[*a].total_cmp(&scores[*b]))
        .unwrap();
    if selected <= 3 && scores[selected] > 0. {
        let near: Vec<_> = (0..6)
            .filter(|i| (scores[*i] - scores[selected]) / scores[selected] < 0.5)
            .collect();
        selected = match near.len() {
            5.. => 2,
            3 | 4 => near[1],
            _ => near[0],
        };
        if selected < 5 {
            let ratio = scores[4] / scores[5];
            if ratio > 1.25 {
                selected = 5;
            } else if ratio > 1.075 {
                selected = 4;
            }
        }
    }
    if p.flash {
        selected = 1;
    }
    let mut previous = stats::constrain(
        p,
        refine(
            gpu,
            &toned,
            &cube,
            &mut params,
            CANDIDATES[selected],
            45.,
            profile,
        )?,
    );
    for _ in 0..5 {
        let xyz = transform(p.camera, previous);
        let white = xyz.map(|x| x / xyz[1] * 100.);
        let current = refine(gpu, &toned, &cube, &mut params, white, 30., profile)?;
        let errors =
            std::array::from_fn::<_, 3, _>(|i| ((previous[i] - current[i]) / current[i]) as f32);
        let error = (errors[0].abs() + errors[1].abs() + errors[2].abs()) / 3.;
        previous = current;
        if error <= 0.005 {
            break;
        }
    }
    let result = stats::finish(p, previous);
    if result.iter().any(|x| !x.is_finite() || *x <= 0.) {
        return Err("white-balance estimator selected no usable neutral pixels".into());
    }
    Ok(result)
}
fn refine(
    gpu: &Gpu,
    input: &ReadGuard,
    cube: &ReadGuard,
    params: &mut Params,
    white: [f64; 3],
    radius: f32,
    profile: &mut Profile,
) -> Result<[f64; 3]> {
    params.white = [white[0] as f32, white[1] as f32, white[2] as f32, 0.];
    params.control[1] = radius;
    let [_, _, w, h] = params.shape;
    let uniform = gpu.uniform(params);
    let mut distance = gpu.storage("White-balance neutral candidates", u64::from(w * h) * 4)?;
    // The reference starts minima at 10,000 and maxima at zero. These are part
    // of its binning behavior when all selected tone codes lie on one side.
    let mut bounds = gpu.upload(
        "White-balance tone bounds",
        &[10000_i32, 10000, 10000, 0, 0, 0, 0, 0],
    )?;
    let mut trace = gpu.trace(profile.mode.gpu(), 3);
    gpu.dispatch(
        Kernel::WbSelect,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(cube)),
            (3, Write(distance.write())),
            (5, Write(bounds.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    let values = read_i32(gpu, &bounds)?;
    if values[6] == 0 {
        return Err("white-balance estimator found no neutral candidates".into());
    }
    params.minimum = [values[0], values[1], values[2], 0];
    params.maximum = [values[3], values[4], values[5], 0];
    let uniform = gpu.uniform(params);
    let mut histogram = gpu.upload("White-balance neutral histogram", &vec![0_i32; 32768])?;
    gpu.dispatch(
        Kernel::WbHistogram,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&distance)),
            (5, Write(histogram.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    let chunks = (w * h).div_ceil(REDUCTION_CHUNK);
    let mut result = gpu.storage(
        "White-balance weighted neutral partials",
        u64::from(chunks) * 24,
    )?;
    gpu.dispatch(
        Kernel::WbAccumulate,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&distance)),
            (3, Write(result.write())),
            (5, Read(&histogram)),
        ],
        [chunks.div_ceil(64), 1],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let values = combine_partials(&read_f32(gpu, &result)?, 3);
    // Keep the original Float32 boundary before downstream fixed-point rounding.
    Ok(stats::normalize(std::array::from_fn(|c| {
        f64::from(values[c] as f32)
    })))
}

#[cfg(test)]
mod tests {
    #[test]
    fn signed_row_statistics_preserve_integer_offsets() {
        let gpu = crate::gpu::Gpu::new().unwrap();
        let values = [
            -32768f32, 100., 32767., 7., -4., 2., 1000., 2000., 3000., -5., 6., -7.,
        ];
        let input = gpu.upload("signed mean fixture", &values).unwrap();
        let result = super::mean(
            &gpu,
            &input,
            [2, 2],
            [[0; 4]; 2],
            false,
            [1.9, -2.9, 0.],
            &mut crate::profile::Profile::default(),
        )
        .unwrap();
        assert_eq!(
            result,
            [
                (-32768. + 7. + 1000. - 5.) / 4. - 1.,
                (100. - 4. + 2000. + 6.) / 4. + 2.,
                (32767. + 2. + 3000. - 7.) / 4.
            ]
        );
    }
    #[test]
    fn shader_contract_validates() {
        let module =
            wgpu::naga::front::wgsl::parse_str(include_str!("weighted_white_balance.wgsl"))
                .expect("white-balance shader parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("white-balance shader validates");
    }
}

#[cfg(test)]
#[path = "weighted_white_balance_test.rs"]
mod reduction_tests;
