//! Resolves high-ISO camera metadata into functional reconstruction and color controls.

use super::reference::*;

use super::prepare::{self, ColorInput};
use crate::{
    nodes::color::range_reconstruction::{Controls, Prepared},
    render::Settings,
    source::{Metadata, Result},
};

// Fallback descriptors recovered from the reference implementation. Files with
// these CAMF entries supply their own values. The kernels consume the first two
// rows of the color model and first two coefficients of the intensity model.
const DEFAULT_COLOR_NOISE_MODEL: [f64; 9] =
    [6.3861, 5.1706, 5.3139, 0.0046, 0.0044, 0.004, 0., 0., 0.];
const DEFAULT_INTENSITY_NOISE_MODEL: [f64; 3] = [9.2094, 0.0075, 0.];

fn scalar(m: &Metadata, name: &str, fallback: f64) -> Result<f64> {
    if m.matrix(name).is_some() {
        m.scalar(name)
    } else {
        Ok(fallback)
    }
}
fn vector<const N: usize>(m: &Metadata, name: &str, fallback: [f64; N]) -> Result<[f64; N]> {
    if m.matrix(name).is_some() {
        m.fixed(name)
    } else {
        Ok(fallback)
    }
}

/// This floating-point color path applies its nonlinear corrections after the
/// matrix, unlike the low-ISO signed fixed-point transform.
pub(super) fn color_matrix(
    m: &Metadata,
    input: &ColorInput,
    mode: Option<&str>,
) -> Result<[f64; 9]> {
    let camera = if let Some(mode) = mode {
        prepare::selected_camera(m, input.camera, mode)?
    } else {
        input.camera
    };
    let camera = if let Some((alternate, mixing)) = input.alternate_matrix {
        std::array::from_fn(|i| alternate[i] * (1. - mixing) + camera[i] * mixing)
    } else {
        camera
    };
    working_matrix(camera, input.digital_gain)
}

fn working_matrix(camera: [f64; 9], digital_gain: f32) -> Result<[f64; 9]> {
    let matrix: [f64; 9] = std::array::from_fn(|i| {
        (0..3)
            .map(|k| COLOR_BASIS[i / 3 * 3 + k] * camera[k * 3 + i % 3])
            .sum()
    });
    let response: [f64; 3] =
        std::array::from_fn(|r| matrix[r * 3] + matrix[r * 3 + 1] + matrix[r * 3 + 2]);
    let average = neutral_weight(response);
    if !average.is_finite() || average <= 0. {
        return Err("invalid range color response".into());
    }
    Ok(std::array::from_fn(|i| {
        matrix[i] * average / response[i / 3].max(average * NEUTRAL_ROW_FLOOR)
            * f64::from(digital_gain)
    }))
}

pub(super) fn prepare(
    m: &Metadata,
    input: &ColorInput,
    settings: &Settings,
    unselected: bool,
) -> Result<Prepared> {
    Prepared::new(
        [input.extent[0] as u32, input.extent[1] as u32],
        controls(m, input, settings, unselected)?,
    )
}

fn controls(
    m: &Metadata,
    input: &ColorInput,
    settings: &Settings,
    unselected: bool,
) -> Result<Controls> {
    let iso = prepare::capture_iso(m)? as f32;
    let incandescent = !input.monochrome
        && settings
            .white_balance
            .trim()
            .eq_ignore_ascii_case("Incandescent");
    let sensor = scalar(m, "SensorISO", 100.)? as f32;
    let afe = if sensor == 0. { 1. } else { sensor / 100. };
    let pre = input.ratio.min(2.);
    let noise = vector(m, "ColorNoiseModel", DEFAULT_COLOR_NOISE_MODEL)?;
    let color_noise = std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            let gain = input.final_gain[c] / f64::from(pre);
            let value = if r == 0 {
                noise[c]
                    * f64::from(pre * pre)
                    * f64::from(afe * afe)
                    * if incandescent {
                        1.
                    } else {
                        f64::from((gain * gain) as f32)
                    }
            } else {
                noise[3 + c]
                    * f64::from(pre)
                    * f64::from(afe)
                    * if incandescent {
                        1.
                    } else {
                        f64::from(gain as f32)
                    }
            };
            value as f32
        })
    });
    let noise_scale = scalar(
        m,
        "ResidualAnisoDiffScaleNM",
        match iso as u32 {
            400 => {
                if incandescent {
                    0.75
                } else {
                    0.25
                }
            }
            800 => {
                if incandescent {
                    1.
                } else {
                    0.75
                }
            }
            _ => 1.,
        },
    )? as f32;
    let intensity = vector(m, "IntensityNoiseModel", DEFAULT_INTENSITY_NOISE_MODEL)?;
    let mut dq = vector(m, "ColorDQCamRGB", [0.; 3])?;
    if dq == [0.; 3] {
        dq = input.default_nonlinear_noise();
    }
    let attenuation =
        super::policy::CameraPolicy::from_metadata(m).color_noise_attenuation(afe * input.ratio);
    let channel_noise = std::array::from_fn(|c| {
        input.statistics_factor * input.final_gain[c] * dq[c] * attenuation
    });
    // Each inline fallback below is the reference descriptor default for the
    // adjacent CAMF name. Numerical modes remain independent of camera identity.
    Ok(Controls {
        enabled: settings.reconstruction.is_some(),
        downsample: scalar(
            m,
            "DownSampleFactor",
            if incandescent || iso > RANGE_RECONSTRUCTION_ISO as f32 {
                4.
            } else {
                2.
            },
        )? as u32,
        window: 2 * scalar(m, "BilateralFilterRadius", 5.)? as u32 + 1,
        subsample: scalar(m, "SubSampleKernel", 1.)? != 0.,
        noise_scaling: scalar(m, "NoiseScaling", 0.5)? as f32,
        edge_strength: scalar(m, "EdgeMapScale", 0.5)? as f32,
        local_decay: scalar(m, "LocalMeanDecay", 0.9)? as f32,
        local_iterations: scalar(m, "LocalMeanIters", 1.)? as u32,
        dark_low: scalar(m, "DarkNeutralizeLow", 2.)? as f32,
        dark_high: scalar(m, "DarkNeutralizeHigh", 50.)? as f32,
        constant_iterations: scalar(
            m,
            "ResidualAnisoDiffIters2",
            if incandescent || iso > RANGE_RECONSTRUCTION_ISO as f32 {
                1.
            } else {
                0.
            },
        )? as u32,
        diffusion_iterations: 1,
        diffusion_k: scalar(m, "ResidualAnisoDiffK2", 2.)? as f32,
        diffusion_step: scalar(m, "AnisoLambda", 0.25)? as f32,
        residual_strength: scalar(m, "ResidualScaling", 1.)? as f32,
        residual_epsilon: scalar(m, "ResidualEpsilon", 5.)? as f32,
        color_noise,
        intensity_noise: [
            intensity[0] as f32 * noise_scale,
            intensity[1] as f32 * noise_scale,
        ],
        luma_weights: std::array::from_fn(|c| 1. / ((input.relative_gain[c] as f32 * pre) * 3.)),
        matrix: color_matrix(
            m,
            input,
            (!unselected).then_some(settings.color_mode.as_str()),
        )?,
        channel_noise,
        digital_gain: input.digital_gain,
    })
}

#[cfg(test)]
#[path = "range_test.rs"]
mod tests;
