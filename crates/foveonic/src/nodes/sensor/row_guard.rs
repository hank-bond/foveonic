//! Bounds fine native row offsets using independent image spans and fixed layer covariance.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

const LAYERS: u64 = 3;
const FLOAT_BYTES: u64 = 4;
const SPANS: u32 = 12;
const SPAN_COMPONENTS: u64 = 9;
const ROW_COMPONENTS: u64 = 9;
const LINE_WORKGROUP: u32 = 64;
const IMAGE_WORKGROUP: u32 = 8;
const PASSES: u32 = 8;
const MINIMUM_SIDE: u32 = 65;
const ISOS: [u32; 5] = [100, 200, 400, 800, 1600];
const GAUSSIANS: [(f64, usize); 4] = [(1.25, 5), (3., 12), (8., 32), (4., 16)];
// Inverse native-layer covariance of fine row offsets. The fitting captures
// cover three bodies. These are amplitude priors, not a spatial mean image.
const INVERSE_COVARIANCE: [[f32; 9]; 5] = [
    [
        19.107,
        0.12331478,
        -0.6167973,
        0.12331478,
        14.442785,
        -0.28899893,
        -0.6167973,
        -0.28899893,
        35.301594,
    ],
    [
        25.818092,
        -0.00394548,
        -0.5967394,
        -0.00394548,
        19.616152,
        -0.82909,
        -0.5967394,
        -0.82909,
        29.82748,
    ],
    [
        25.160147,
        0.09412605,
        -0.5922576,
        0.09412605,
        19.473984,
        -0.56491584,
        -0.5922576,
        -0.56491584,
        27.728594,
    ],
    [
        24.412165,
        -0.13668239,
        -0.10386218,
        -0.13668239,
        18.427172,
        -0.24404354,
        -0.10386218,
        -0.24404354,
        115.5543,
    ],
    [
        24.411604,
        0.10168512,
        0.17636898,
        0.10168512,
        18.164845,
        -0.34401447,
        0.17636898,
        -0.34401447,
        65.91954,
    ],
];

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    source: [u32; 4],
    area: [u32; 4],
    noise: [[f32; 4]; 3],
    inverse_covariance: [[f32; 4]; 3],
    mean_pass: u32,
    reserved: [u32; 3],
}

#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    weights: Vec<f32>,
}

impl Prepared {
    /// Uses uncalibrated sensor codes and a half-open active rectangle.
    /// Noise terms are constant, linear and quadratic, each in layer order.
    pub(crate) fn new(extent: [u32; 2], area: [u32; 4], noise: [f32; 9], iso: u32) -> Result<Self> {
        if area[0] >= area[2]
            || area[1] >= area[3]
            || area[2] > extent[0]
            || area[3] > extent[1]
            || area[2] - area[0] < MINIMUM_SIDE
            || area[3] - area[1] < MINIMUM_SIDE
            || noise.iter().any(|v| !v.is_finite())
            || iso == 0
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / LAYERS
        {
            return Err("invalid native row guard parameters".into());
        }
        let model = ISOS
            .iter()
            .enumerate()
            .min_by_key(|(_, value)| value.abs_diff(iso))
            .unwrap()
            .0;
        let mut weights = Vec::new();
        for (sigma, radius) in GAUSSIANS {
            let kernel: Vec<f64> = (-(radius as isize)..=radius as isize)
                .map(|x| (-0.5 * (x as f64 / sigma).powi(2)).exp())
                .collect();
            let sum: f64 = kernel.iter().sum();
            weights.extend(kernel.into_iter().map(|v| (v / sum) as f32));
        }
        Ok(Self {
            params: Params {
                source: [extent[0], extent[1], area[2] - area[0], area[3] - area[1]],
                area,
                noise: std::array::from_fn(|i| {
                    [noise[i * 3], noise[i * 3 + 1], noise[i * 3 + 2], 0.]
                }),
                inverse_covariance: std::array::from_fn(|i| {
                    [
                        INVERSE_COVARIANCE[model][i * 3],
                        INVERSE_COVARIANCE[model][i * 3 + 1],
                        INVERSE_COVARIANCE[model][i * 3 + 2],
                        0.,
                    ]
                }),
                mean_pass: 0,
                reserved: [0; 3],
            },
            weights,
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.source[0], self.params.source[1]]
    }

    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).slice(&self.weights)
    }
}

fn workspace(gpu: &Gpu, label: &str, floats: u64) -> Result<ReadGuard> {
    gpu.storage(label, floats * FLOAT_BYTES)
}

/// The first two inputs must be the immediate pre/post-shield sensor images.
/// The original supplies immutable measurement and clipping evidence.
pub(crate) fn apply(
    gpu: &Gpu,
    original: &ReadGuard,
    shield_corrected: &ReadGuard,
    black: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = prepared.extent();
    let bytes = u64::from(width) * u64::from(height) * LAYERS * FLOAT_BYTES;
    if original.size() != bytes
        || shield_corrected.size() != bytes
        || black.size() != LAYERS * FLOAT_BYTES
    {
        return Err("native row guard input geometry mismatch".into());
    }
    let rows = u64::from(prepared.params.source[3]);
    let mut spans = workspace(
        gpu,
        "Independent native row spans",
        rows * u64::from(SPANS) * SPAN_COMPONENTS,
    )?;
    let mut stats = workspace(gpu, "Robust native row statistics", rows * ROW_COMPONENTS)?;
    let mut field = workspace(gpu, "Native row reliability field", rows * LAYERS)?;
    let mut guarded = workspace(gpu, "Guarded native row field", rows * LAYERS)?;
    let mut means = workspace(gpu, "Native row field means", 2 * LAYERS)?;
    let mut guide = workspace(
        gpu,
        "Native row signal guide",
        rows * u64::from(prepared.params.source[2]) * LAYERS,
    )?;
    let mut output = gpu.storage("Guarded native sensor layers", bytes)?;
    let weights = gpu.upload("Native row Gaussian weights", &prepared.weights)?;
    let uniform = gpu.uniform(&prepared.params);
    let mut trace = gpu.trace(profile.mode.gpu(), PASSES);
    let line_groups = [prepared.params.source[3].div_ceil(LINE_WORKGROUP), 1];
    gpu.dispatch(
        Kernel::RowGuardProfiles,
        &mut [
            (0, Read(&uniform)),
            (1, Read(original)),
            (2, Read(shield_corrected)),
            (3, Read(black)),
            (4, Read(&weights)),
            (5, Write(spans.write())),
        ],
        [line_groups[0], SPANS],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RowGuardRobust,
        &mut [
            (0, Read(&uniform)),
            (5, Read(&spans)),
            (6, Write(stats.write())),
        ],
        line_groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RowGuardReliability,
        &mut [
            (0, Read(&uniform)),
            (4, Read(&weights)),
            (6, Read(&stats)),
            (7, Write(field.write())),
        ],
        line_groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RowGuardMean,
        &mut [
            (0, Read(&uniform)),
            (4, Read(&weights)),
            (7, Read(&field)),
            (9, Write(means.write())),
        ],
        [LAYERS as u32, 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RowGuardConfidence,
        &mut [
            (0, Read(&uniform)),
            (4, Read(&weights)),
            (5, Read(&spans)),
            (6, Read(&stats)),
            (7, Read(&field)),
            (8, Write(guarded.write())),
            (9, Read(&means)),
        ],
        line_groups,
        &mut trace,
    );
    let mut final_params = prepared.params;
    final_params.mean_pass = 1;
    let final_uniform = gpu.uniform(&final_params);
    gpu.dispatch(
        Kernel::RowGuardMean,
        &mut [
            (0, Read(&final_uniform)),
            (4, Read(&weights)),
            (7, Read(&guarded)),
            (9, Write(means.write())),
        ],
        [LAYERS as u32, 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RowGuardHorizontal,
        &mut [
            (0, Read(&uniform)),
            (1, Read(original)),
            (3, Read(black)),
            (4, Read(&weights)),
            (10, Write(guide.write())),
        ],
        [
            prepared.params.source[2].div_ceil(IMAGE_WORKGROUP),
            prepared.params.source[3].div_ceil(IMAGE_WORKGROUP),
        ],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RowGuardApply,
        &mut [
            (0, Read(&uniform)),
            (1, Read(original)),
            (2, Read(shield_corrected)),
            (4, Read(&weights)),
            (8, Read(&guarded)),
            (9, Read(&means)),
            (10, Read(&guide)),
            (11, Write(output.write())),
        ],
        [
            width.div_ceil(IMAGE_WORKGROUP),
            height.div_ceil(IMAGE_WORKGROUP),
        ],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "row_guard_test.rs"]
mod tests;
