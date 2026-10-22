//! Applies highlight neutralization, adaptive camera-color response and chroma outlier protection.

use super::uv_repair;
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

#[derive(Clone)]
pub(crate) struct Controls {
    pub extent: [u32; 2],
    pub white_balance: [f32; 3],
    pub normalization: f32,
    pub neutral_low: f32,
    pub neutral_high: f32,
    pub matrix: [f64; 9],
    pub input_divisor: f32,
    pub output_gain: f32,
    pub brightness_factor: f32,
    pub brightness_offset: f32,
    pub brightness_gain: f32,
    pub second_exponent: f32,
    pub residual_scale: f32,
    pub hue_strength: [f32; 7],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    neutral: [f32; 4],
    gains: [f32; 4],
    appearance: [f32; 4],
    response: [f32; 4],
    hue_first: [f32; 4],
    hue_last: [f32; 4],
}

pub(crate) struct Prepared {
    params: Params,
    matrix: [[f32; 2]; 9],
    tables: Vec<f32>,
}

pub(crate) struct Output {
    pub(crate) rgb: ReadGuard,
    pub(crate) neutralized: ReadGuard,
    pub(crate) raw_color: ReadGuard,
}

impl Prepared {
    /// Keeps highlight neutralization and chroma protection, while the response
    /// pass only restores output-code scaling.
    pub(crate) fn without_color_response(mut self) -> Self {
        self.params.response[1] = 1.;
        self
    }

    /// Tables contain 901 brightness, 3072 signed-response and 4096 adaptive values.
    pub(crate) fn new(
        controls: Controls,
        brightness: Vec<f32>,
        signed_response: Vec<f32>,
        adaptive: Vec<f32>,
    ) -> Result<Self> {
        let c = controls;
        if c.extent.contains(&0)
            || u64::from(c.extent[0]) * u64::from(c.extent[1]) > u64::from(u32::MAX) / 3
            || c.white_balance.iter().any(|v| !v.is_finite() || *v == 0.)
            || !c.normalization.is_finite()
            || c.normalization <= 0.
            || !c.neutral_low.is_finite()
            || !c.neutral_high.is_finite()
            || c.neutral_high <= c.neutral_low
            || c.matrix
                .iter()
                .any(|v| !v.is_finite() || !(*v as f32).is_finite())
            || [
                c.input_divisor,
                c.output_gain,
                c.brightness_factor,
                c.brightness_offset,
                c.brightness_gain,
                c.second_exponent,
                c.residual_scale,
            ]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.)
            || c.hue_strength.iter().any(|v| !v.is_finite() || *v < 0.)
            || brightness.len() != 901
            || signed_response.len() != 3072
            || adaptive.len() != 4096
            || brightness
                .iter()
                .chain(&signed_response)
                .chain(&adaptive)
                .any(|v| !v.is_finite())
            || adaptive.iter().any(|v| *v <= 0.)
        {
            return Err("invalid adaptive raw-color parameters or tables".into());
        }
        let mut tables = brightness;
        tables.extend(signed_response);
        tables.extend(adaptive);
        Ok(Self {
            params: Params {
                shape: [c.extent[0], c.extent[1], 0, 0],
                neutral: [
                    c.normalization,
                    c.neutral_low,
                    c.neutral_high - c.neutral_low,
                    0.,
                ],
                gains: [
                    c.white_balance[0],
                    c.white_balance[1],
                    c.white_balance[2],
                    0.,
                ],
                appearance: [
                    c.input_divisor,
                    c.output_gain,
                    c.brightness_factor,
                    c.brightness_offset,
                ],
                response: [c.brightness_gain, 0., c.second_exponent, c.residual_scale],
                hue_first: [
                    c.hue_strength[0],
                    c.hue_strength[1],
                    c.hue_strength[2],
                    c.hue_strength[3],
                ],
                hue_last: [c.hue_strength[4], c.hue_strength[5], c.hue_strength[6], 0.],
            },
            matrix: c.matrix.map(|v| {
                let high = v as f32;
                [high, (v - f64::from(high)) as f32]
            }),
            tables,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.matrix)
            .slice(&self.tables)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<Output> {
    let extent = prepared.extent();
    if input.size() != u64::from(extent[0]) * u64::from(extent[1]) * 12 {
        return Err("raw-color input does not match its extent".into());
    }
    let uniform = gpu.uniform(&prepared.params);
    let matrix = gpu.upload("Adaptive camera-color matrix", &prepared.matrix)?;
    let tables = gpu.upload("Adaptive raw-color response tables", &prepared.tables)?;
    let mut neutralized = gpu.storage("Highlight neutralized RGB", input.size())?;
    let mut raw_color = gpu.storage("Adaptive camera RGB", input.size())?;
    let groups = [extent[0].div_ceil(8), extent[1].div_ceil(8)];
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::RawColorNeutralize,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(neutralized.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RawColorResponse,
        &mut [
            (0, Read(&neutralized)),
            (1, Read(&uniform)),
            (2, Read(&matrix)),
            (3, Read(&tables)),
            (4, Write(raw_color.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let yuv = uv_repair::convert(gpu, &raw_color, extent, false, profile)?;
    let mut clamped = gpu.storage("Outlier protected YUV", input.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::RawColorChroma,
        &mut [
            (0, Read(&yuv)),
            (1, Read(&uniform)),
            (2, Write(clamped.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let rgb = uv_repair::convert(gpu, &clamped, extent, true, profile)?;
    Ok(Output {
        rgb,
        neutralized,
        raw_color,
    })
}

#[cfg(test)]
#[path = "raw_color_blend_test.rs"]
mod tests;
