//! Crops, rotates and resizes integral RGB codes with fixed-point mapping and source-derived interpolation phases.
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

// The reference kernel uses this literal, independently of the standard PI.
#[allow(clippy::approx_constant)]
const KERNEL_PI: f64 = 3.14159;
const PHASES: usize = 16;
const TAPS: usize = 16;
const FIXED_ONE: f64 = 65536.;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    /// Base x/y and per-column x/y increments, represented modulo2^32.
    column: [u32; 4],
    /// Per-row x/y increments, direct-copy flag and interpolated upper cap.
    row: [u32; 4],
}
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    kernel: Vec<f32>,
}
impl Prepared {
    /// Crop right/bottom are exclusive. Output dimensions include rotation.
    pub(crate) fn new(
        input: [u32; 2],
        crop: [u32; 4],
        output: [u32; 2],
        rotation: u32,
        clamp15: bool,
    ) -> Result<Self> {
        let [w, h] = input;
        let [left, top, right, bottom] = crop;
        let angle = rotation % 360;
        if input.contains(&0)
            || output.contains(&0)
            || w > 65535
            || h > 65535
            || left >= right
            || top >= bottom
            || right > w
            || bottom > h
            || ![0, 90, 180, 270].contains(&angle)
            || u64::from(w) * u64::from(h) > u64::from(u32::MAX) / 3
            || u64::from(output[0]) * u64::from(output[1]) > u64::from(u32::MAX) / 3
        {
            return Err("invalid code crop geometry".into());
        }
        let [rw, rh] = if angle == 90 || angle == 270 {
            [output[1], output[0]]
        } else {
            output
        };
        let direct = right - left == rw && bottom - top == rh;
        let sx = f64::from(rw) / f64::from(right - left);
        let sy = f64::from(rh) / f64::from(bottom - top);
        let step_x = (FIXED_ONE / sx) as i64;
        let step_y = (FIXED_ONE / sy) as i64;
        if step_x > u32::MAX as i64 || step_y > u32::MAX as i64 {
            return Err("code crop step is out of range".into());
        }
        let (x, y, dx, dy, rx, ry) = match angle {
            0 => (left, top, step_x, 0, 0, step_y),
            90 => (left, bottom - 1, 0, -step_y, step_x, 0),
            180 => (right - 1, bottom - 1, -step_x, 0, 0, -step_y),
            _ => (right - 1, top, 0, step_y, -step_x, 0),
        };
        Ok(Self {
            params: Params {
                shape: [w, h, output[0], output[1]],
                column: [x << 16, y << 16, dx as u32, dy as u32],
                row: [
                    rx as u32,
                    ry as u32,
                    u32::from(direct),
                    if clamp15 { 32767 } else { 65535 },
                ],
            },
            kernel: if direct {
                vec![0.]
            } else {
                interpolation_kernel(sx, sy)
            },
        })
    }
    pub(crate) fn input_extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[2], self.params.shape[3]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).slice(&self.kernel)
    }
}

fn interpolation_kernel(sx: f64, sy: f64) -> Vec<f32> {
    let radial = 0.9 < sx / sy && sx / sy < 1.1;
    let scale = (sx + sy) * 0.5;
    let frequency = 0.88_f64.min(scale.mul_add(0.43, 0.43)).min(scale * 0.86);
    let window = 0.88_f64.min(scale.sqrt().sqrt()) * 0.25;
    let exponent = (scale * 1.6).clamp(0.7, 1.6);
    let axis = |scale: f64| {
        (
            (scale * 0.94).min(1.) * KERNEL_PI,
            scale.sqrt().sqrt().min(0.9) * KERNEL_PI * 0.25,
            (2. * scale).min(2.),
        )
    };
    let (ax, ay) = if sx >= sy {
        (axis(sx.min(1.1)), axis(sy.min(0.8)))
    } else {
        (axis(sx.min(0.8)), axis(sy.min(1.1)))
    };
    let sample_axis = |position: f64, (frequency, window, power): (f64, f64, f64)| {
        let angle = frequency * position;
        (angle.sin() / angle) * (window * position).cos().max(0.).powf(power)
    };
    let mut kernel = Vec::with_capacity(PHASES * PHASES * TAPS);
    for py in 0..PHASES {
        let y = (15.5 - py as f64) * 0.0625;
        for px in 0..PHASES {
            let x = (15.5 - px as f64) * 0.0625;
            let mut values = [0_f64; TAPS];
            let mut total = 0.;
            for dy in 0..4 {
                for dx in 0..4 {
                    let xx = x + dx as f64 - 2.;
                    let yy = y + dy as f64 - 2.;
                    let (a, b) = if radial {
                        let radius = xx.mul_add(xx, yy * yy).sqrt() * KERNEL_PI;
                        let angle = frequency * radius;
                        (
                            angle.sin() / angle,
                            (window * radius).cos().max(0.).powf(exponent),
                        )
                    } else {
                        (sample_axis(yy, ay), sample_axis(xx, ax))
                    };
                    values[dy * 4 + dx] = a * b;
                    total = a.mul_add(b, total);
                }
            }
            let normalization = 1. / total;
            kernel.extend(values.map(|v| (v * normalization) as f32));
        }
    }
    kernel
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = prepared.input_extent();
    if input.size() != u64::from(w) * u64::from(h) * 12 {
        return Err("code crop input geometry mismatch".into());
    }
    let [ow, oh] = prepared.extent();
    let uniform = gpu.uniform(&prepared.params);
    let weights = gpu.upload("Code interpolation phases", &prepared.kernel)?;
    let mut output = gpu.storage(
        "Cropped integral RGB codes",
        u64::from(ow) * u64::from(oh) * 12,
    )?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::CodeCrop,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&weights)),
            (3, Write(output.write())),
        ],
        [ow.div_ceil(8), oh.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
#[cfg(test)]
#[path = "code_crop_test.rs"]
mod tests;
