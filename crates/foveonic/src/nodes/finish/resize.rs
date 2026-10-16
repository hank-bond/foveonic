//! Prepares reference photographic resize kernels and samples owned floating RGB on the GPU.

// Kernel preparation and 16.16 crop mapping are adapted with owner permission
// from the reference implementation's resizing.rs at e47af35.
use crate::{
    gpu::{Binding::Read, Binding::Write, Gpu, Kernel, ReadGuard},
    nodes::sensor::calibration::EncodeParams,
    profile::Profile,
    source::Result,
};
use bytemuck::{Pod, Zeroable};

#[cfg(test)]
#[path = "resize_test.rs"]
mod tests;

const PHASES: usize = 16;
const TAPS: usize = 4;
const ANGLE: f64 = f64::from_bits(0x4009_21f9_f01b_866e);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Mapping {
    origin: [i32; 4],
    increments: [i32; 4],
}

struct Prepared {
    mapping: Mapping,
    coefficients: Vec<f32>,
}

// These are geometric tables only. Photographic samples never leave the GPU.
fn coefficients(sx: f64, sy: f64) -> Result<Vec<f32>> {
    if [sx, sy].iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("invalid photographic resize scales".into());
    }
    let radial = (sx / sy) > 0.9 && (sx / sy) < 1.1;
    let mut values = vec![0.; PHASES * PHASES * TAPS * TAPS];
    if radial {
        let scale = (sx + sy) * 0.5;
        let frequency = scale.mul_add(0.43, 0.43).min(0.88).min(scale * 0.86);
        let root = scale.sqrt().sqrt();
        let window = if root > 0.88 { 0.22 } else { root * 0.25 };
        let exponent = (scale * 1.6).clamp(0.7, 1.6);
        for py in 0..PHASES {
            let fy = (15.5 - py as f64) * 0.0625;
            for px in 0..PHASES {
                let fx = (15.5 - px as f64) * 0.0625;
                let mut row = [0.; TAPS * TAPS];
                let mut sum = 0.;
                for y in 0..TAPS {
                    let dy = fy + (y as f64 - 2.);
                    for x in 0..TAPS {
                        let dx = fx + (x as f64 - 2.);
                        let angle = dx.mul_add(dx, dy * dy).sqrt() * ANGLE;
                        let argument = frequency * angle;
                        let wave = argument.sin() / argument;
                        let envelope = (window * angle).cos().max(0.).powf(exponent);
                        row[y * TAPS + x] = wave * envelope;
                        sum = wave.mul_add(envelope, sum);
                    }
                }
                let reciprocal = 1. / sum;
                let start = (py * PHASES + px) * TAPS * TAPS;
                for (out, value) in values[start..start + TAPS * TAPS].iter_mut().zip(row) {
                    *out = (value * reciprocal) as f32;
                }
            }
        }
    } else {
        let (sx, sy) = if sx >= sy {
            (sx.min(1.1), sy.min(0.8))
        } else {
            (sx.min(0.8), sy.min(1.1))
        };
        let axis = |scale: f64, phase: usize| {
            let frequency = (scale * 0.94).min(1.) * ANGLE;
            let root = scale.sqrt().sqrt();
            let window = if root > 0.9 {
                f64::from_bits(0x3fe6_9e94_24e5_9296)
            } else {
                (root * ANGLE) * 0.25
            };
            let exponent = (scale + scale).min(2.);
            let fraction = (15.5 - phase as f64) * 0.0625;
            std::array::from_fn::<_, TAPS, _>(|tap| {
                let distance = fraction + (tap as f64 - 2.);
                let argument = frequency * distance;
                let wave = argument.sin() / argument;
                wave * (window * distance).cos().max(0.).powf(exponent)
            })
        };
        for py in 0..PHASES {
            let y = axis(sy, py);
            for px in 0..PHASES {
                let x = axis(sx, px);
                let mut sum = 0.;
                for &row in &y {
                    for &column in &x {
                        sum = column.mul_add(row, sum);
                    }
                }
                let reciprocal = 1. / sum;
                let start = (py * PHASES + px) * TAPS * TAPS;
                for row in 0..TAPS {
                    for column in 0..TAPS {
                        values[start + row * TAPS + column] =
                            ((y[row] * x[column]) * reciprocal) as f32;
                    }
                }
            }
        }
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Err("nonfinite photographic resize kernel".into());
    }
    Ok(values)
}

impl Prepared {
    fn new(params: &EncodeParams) -> Result<Option<Self>> {
        let [w, h, rotation, _] = params.source;
        let [left, top, right, bottom] = params.crop;
        let [ow, oh, _, _] = params.output;
        if w == 0
            || h == 0
            || left >= right
            || top >= bottom
            || right > w
            || bottom > h
            || ow == 0
            || oh == 0
            || !matches!(rotation, 0 | 90 | 180 | 270)
        {
            return Err("invalid photographic resize geometry".into());
        }
        let crop = [right - left, bottom - top];
        let natural = if rotation.is_multiple_of(180) {
            crop
        } else {
            [crop[1], crop[0]]
        };
        if natural == [ow, oh] {
            return Ok(None);
        }
        if w > i16::MAX as u32 || h > i16::MAX as u32 {
            return Err("photographic resize exceeds signed 16.16 coordinates".into());
        }
        let logical = if rotation.is_multiple_of(180) {
            [ow, oh]
        } else {
            [oh, ow]
        };
        let scales = std::array::from_fn::<_, 2, _>(|i| {
            f64::from(logical[i]) / f64::from(crop[i].saturating_sub(1).max(1))
        });
        let steps = std::array::from_fn::<_, 2, _>(|i| {
            if crop[i] == 1 {
                0
            } else {
                (65536. / scales[i]) as i32
            }
        });
        let start = |n: u32| ((n as i32) << 16) + 32768;
        // A singleton crop axis has no coordinate advance.
        let end = |n: u32, size: u32| start(n - 1) - if size == 1 { 0 } else { 65536 };
        let [dx, dy] = steps;
        let (x, y, increments) = match rotation {
            0 => (start(left), start(top), [dx, 0, 0, dy]),
            90 => (start(left), end(bottom, crop[1]), [0, -dy, dx, 0]),
            180 => (end(right, crop[0]), end(bottom, crop[1]), [-dx, 0, 0, -dy]),
            270 => (end(right, crop[0]), start(top), [0, dy, -dx, 0]),
            _ => unreachable!(),
        };
        let direct = scales == [1., 1.];
        Ok(Some(Self {
            mapping: Mapping {
                origin: [x, y, i32::from(direct), 0],
                increments,
            },
            coefficients: if direct {
                vec![0.]
            } else {
                coefficients(scales[0], scales[1])?
            },
        }))
    }
}

/// Materialize only resized exports. Native-size encoding retains its exact old path.
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    params: &EncodeParams,
    profile: &mut Profile,
) -> Result<(ReadGuard, EncodeParams)> {
    let Some(prepared) =
        profile.measure("prepare photographic resize", || Prepared::new(params))?
    else {
        return Ok((input.clone(), *params));
    };
    let [w, h, _, _] = params.source;
    if input.size() != u64::from(w) * u64::from(h) * 12 || params.source[3] != 0 {
        return Err("photographic resize requires packed floating RGB".into());
    }
    let [ow, oh, _, _] = params.output;
    let groups = [ow.div_ceil(8), oh.div_ceil(8)];
    if groups
        .iter()
        .any(|&n| n > gpu.device.limits().max_compute_workgroups_per_dimension)
    {
        return Err("photographic resize exceeds GPU dispatch limits".into());
    }
    let mut output = gpu.storage(
        "Resized photographic RGB",
        u64::from(ow) * u64::from(oh) * 12,
    )?;
    let uniform = gpu.uniform(params);
    let mapping = gpu.uniform(&prepared.mapping);
    let weights = gpu.upload("Photographic resize phases", &prepared.coefficients)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::OutputResize,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
            (3, Read(&weights)),
            (4, Read(&mapping)),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok((
        output,
        EncodeParams {
            source: [ow, oh, 0, 0],
            crop: [0, 0, ow, oh],
            output: params.output,
        },
    ))
}
