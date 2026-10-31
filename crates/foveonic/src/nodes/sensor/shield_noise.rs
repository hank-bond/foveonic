//! Predicts native green-magenta row and column errors from the original shield samples.

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
const AXES: u64 = 2;
const HISTOGRAM_BINS: u64 = 4096;
const LINE_WORKGROUP: u32 = 64;
const IMAGE_WORKGROUP: u32 = 8;
const DISPATCHES: u32 = 7;
const COLUMN_REFERENCE_LENGTH: u32 = 4704;
const MODEL_COUNT: usize = 5;
const SCALES: usize = 5;
const WEIGHTS_PER_MODEL: usize = SCALES * LAYERS as usize;
const GAUSSIAN_SIGMAS: [usize; 4] = [2, 8, 32, 128];
const GAUSSIAN_WEIGHT_COUNT: usize = 8
    * (GAUSSIAN_SIGMAS[0] + GAUSSIAN_SIGMAS[1] + GAUSSIAN_SIGMAS[2] + GAUSSIAN_SIGMAS[3])
    + GAUSSIAN_SIGMAS.len();
const REFERENCE_OFFSET: usize = MODEL_COUNT * WEIGHTS_PER_MODEL + GAUSSIAN_WEIGHT_COUNT;
const MODEL_FLOATS: usize = REFERENCE_OFFSET + AXES as usize * COLUMN_REFERENCE_LENGTH as usize;
const BASIS_MINIMUM: f32 = 1e-6;
const DIRECTION_SUM_TOLERANCE: f32 = 1e-5;
const NATIVE_GM_AXIS: [f32; 3] = [-0.7025801, 0.6792753, -0.21205261];
// Little-endian Float32 tables contain five coefficient sets, four normalized
// Gaussian kernels and two scalar column reference curves. The sets are Rows,
// Full low/high rows and Full low/high columns. Adjacent band coefficients are
// differenced so identity and Gaussian profiles compose the same prediction.
// These fixed additive-error models were fitted from 340 caps on three bodies.
const MODEL_BYTES: &[u8] = include_bytes!("shield_noise_model.bin");

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    source: [u32; 4],
    active: [u32; 4],
    bottom: [u32; 4],
    right: [u32; 4],
    direction: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    model: Vec<f32>,
}

impl Prepared {
    /// Rectangles are half-open sensor coordinates. The direction must preserve
    /// the native layer sum. Its magnitude is normalized to the trained axis.
    pub(crate) fn new(
        extent: [u32; 2],
        active: [u32; 4],
        bottom: [u32; 4],
        right: [u32; 4],
        direction: [f32; 3],
        columns: bool,
    ) -> Result<Self> {
        let valid =
            |r: [u32; 4]| r[0] < r[2] && r[1] < r[3] && r[2] <= extent[0] && r[3] <= extent[1];
        if !valid(active)
            || !valid(bottom)
            || !valid(right)
            || active[2] - active[0] < 2
            || active[3] - active[1] < 2
            || bottom[0] != active[0]
            || bottom[2] != active[2]
            || bottom[1] < active[3]
            || right[1] != active[1]
            || right[3] != active[3]
            || right[0] < active[2]
            || (columns && active[2] - active[0] != COLUMN_REFERENCE_LENGTH)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / LAYERS
            || direction.iter().any(|v| !v.is_finite())
        {
            return Err("invalid shield-noise geometry or direction".into());
        }
        let projection = direction
            .iter()
            .zip(NATIVE_GM_AXIS)
            .map(|(a, b)| a * b)
            .sum::<f32>();
        if !projection.is_finite() || projection.abs() < BASIS_MINIMUM {
            return Err("shield-noise direction has no trained color projection".into());
        }
        let direction = direction.map(|v| v / projection);
        if direction.iter().sum::<f32>().abs() > DIRECTION_SUM_TOLERANCE {
            return Err("shield-noise direction must preserve the native layer sum".into());
        }
        let maximum = (active[2] - active[0]).max(active[3] - active[1]);
        let model = MODEL_BYTES
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .map(f32::from_le_bytes)
            .collect::<Vec<_>>();
        if model.len() != MODEL_FLOATS || model.iter().any(|v| !v.is_finite()) {
            return Err("invalid shield-noise model tables".into());
        }
        Ok(Self {
            params: Params {
                source: [extent[0], extent[1], maximum, u32::from(columns)],
                active,
                bottom,
                right,
                direction: [direction[0], direction[1], direction[2], 0.],
            },
            model,
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.source[0], self.params.source[1]]
    }

    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).slice(&self.model)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    original: &ReadGuard,
    black: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = prepared.extent();
    let image_bytes = u64::from(width) * u64::from(height) * LAYERS * 4;
    if input.size() != image_bytes || original.size() != image_bytes || black.size() != LAYERS * 4 {
        return Err("shield-noise input geometry mismatch".into());
    }
    let maximum = prepared.params.source[2];
    let axes = if prepared.params.source[3] != 0 {
        AXES
    } else {
        1
    };
    let profile_groups = [maximum.div_ceil(LINE_WORKGROUP), axes as u32];
    let uniform = gpu.uniform(&prepared.params);
    let model = gpu.upload("Shield-noise model", &prepared.model)?;
    let mut histogram = gpu.storage(
        "Shield sample histograms",
        AXES * LAYERS * HISTOGRAM_BINS * 4,
    )?;
    let mut guards = gpu.storage("Shield median and guard widths", AXES * LAYERS * 2 * 4)?;
    let mut profiles = gpu.storage(
        "Original shield profiles",
        AXES * u64::from(maximum) * LAYERS * 4,
    )?;
    let mut means = gpu.storage("Shield profile means", AXES * LAYERS * 4)?;
    let mut predictions = gpu.storage(
        "Predicted shield color errors",
        AXES * u64::from(maximum) * 4,
    )?;
    let mut prediction_means = gpu.storage("Shield prediction means", AXES * 4)?;
    let mut output = gpu.storage("Shield-corrected native layers", image_bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), DISPATCHES);
    let count = |r: [u32; 4]| (r[2] - r[0]) * (r[3] - r[1]);
    let samples = count(prepared.params.right).max(if axes == AXES {
        count(prepared.params.bottom)
    } else {
        0
    });
    gpu.dispatch(
        Kernel::ShieldHistogram,
        &mut [
            (0, Read(&uniform)),
            (2, Read(original)),
            (5, Write(histogram.write())),
        ],
        [samples.div_ceil(LINE_WORKGROUP), axes as u32],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ShieldGuard,
        &mut [
            (0, Read(&uniform)),
            (5, Read(&histogram)),
            (6, Write(guards.write())),
        ],
        [LAYERS as u32, axes as u32],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ShieldProfile,
        &mut [
            (0, Read(&uniform)),
            (2, Read(original)),
            (6, Read(&guards)),
            (7, Write(profiles.write())),
        ],
        profile_groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ShieldCenter,
        &mut [
            (0, Read(&uniform)),
            (7, Read(&profiles)),
            (8, Write(means.write())),
        ],
        [LAYERS as u32, axes as u32],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ShieldPredict,
        &mut [
            (0, Read(&uniform)),
            (3, Read(black)),
            (4, Read(&model)),
            (7, Read(&profiles)),
            (8, Read(&means)),
            (9, Write(predictions.write())),
        ],
        profile_groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ShieldReduce,
        &mut [
            (0, Read(&uniform)),
            (9, Read(&predictions)),
            (10, Write(prediction_means.write())),
        ],
        [1, axes as u32],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ShieldApply,
        &mut [
            (0, Read(&uniform)),
            (1, Read(input)),
            (2, Read(original)),
            (9, Read(&predictions)),
            (10, Read(&prediction_means)),
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
#[path = "shield_noise_test.rs"]
mod tests;
