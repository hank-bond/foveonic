//! Prepares Classic selected camera matrices and fixed-point nonlinear color conversion.

use super::super::reference::{neutral_weight, COLOR_BASIS, NEUTRAL_ROW_FLOOR};
use super::{multiply, scalar, vector, Color, ColorInput, Metadata, Result};
use crate::nodes::sensor::fixed_point::{CodeScale, Transform, TransformParams};

/// Resolve selected appearance and nonlinear noise only for color-dependent outputs.
pub fn prepare_color(metadata: &Metadata, input: &ColorInput, color_mode: &str) -> Result<Color> {
    let camera = selected_camera(metadata, input.camera, color_mode)?;
    prepare_color_matrix(metadata, input, camera)
}

pub(crate) fn selected_camera(
    metadata: &Metadata,
    mut camera: [f64; 9],
    color_mode: &str,
) -> Result<[f64; 9]> {
    if let Some(name) = metadata
        .lists
        .get("ColorModeCompensations")
        .and_then(|list| {
            list.names()
                .find(|key| key.trim().eq_ignore_ascii_case(color_mode.trim()))
                .and_then(|key| list.get(key))
        })
    {
        let mode: [f64; 9] = metadata.fixed(name)?;
        if mode[0] != 0. {
            camera = multiply(camera, mode);
        }
    } else if metadata.lists.contains_key("ColorModeCompensations")
        || !color_mode.eq_ignore_ascii_case("Standard")
    {
        return Err("unavailable color mode".into());
    }
    Ok(camera)
}

pub(crate) fn validate_color_mode(metadata: &Metadata, color_mode: &str) -> Result<()> {
    selected_camera(metadata, [0.; 9], color_mode).map(|_| ())
}

/// Keep selected appearance out of editable DNG samples.
pub(crate) fn prepare_unselected_color(metadata: &Metadata, input: &ColorInput) -> Result<Color> {
    prepare_color_matrix(metadata, input, input.camera)
}

fn prepare_color_matrix(
    metadata: &Metadata,
    input: &ColorInput,
    camera: [f64; 9],
) -> Result<Color> {
    let mut dq = vector(metadata, "ColorDQCamRGB", [0.; 3])?;
    if dq == [0.; 3] {
        dq = input.default_nonlinear_noise();
    }
    let channel_noise =
        std::array::from_fn(|c| input.statistics_factor * input.final_gain[c] * dq[c]);
    let (mut transform, matrix_green_sum) = transform(
        input.extent[0],
        input.extent[1],
        camera,
        channel_noise,
        input.digital_gain,
        if input.monochrome {
            MatrixScale::PreserveGain
        } else {
            MatrixScale::UnitGreen
        },
    )?;
    // A disabled FIR stage intentionally leaves the normalized coefficients
    // unchanged. No later gain restoration is part of that native route.
    if scalar(metadata, "FIR_DisableStep", 0.)? != 0. {
        transform.output_scale = CodeScale::Ready;
    }
    Ok(Color {
        transform,
        matrix_green_sum,
        digital_gain: input.digital_gain,
    })
}

/// Whether a later reconstruction stage restores the matrix gain.
#[derive(Clone, Copy)]
pub(super) enum MatrixScale {
    UnitGreen,
    PreserveGain,
}

#[allow(clippy::approx_constant)] // The recovered cosine cutoff is the literal 3.1416.
pub(crate) fn transform(
    width: usize,
    height: usize,
    camera: [f64; 9],
    noise: [f64; 3],
    digital: f32,
    scale: MatrixScale,
) -> Result<(Transform, f32)> {
    // The selected caller supplies these final GetTanhLUT amplitudes.
    let amplitudes = noise;
    if amplitudes.iter().any(|a| !a.is_finite() || *a <= 0.) {
        return Err("invalid nonlinear noise amplitude".into());
    }
    const TANH_SLOPE: f64 = 0.8;
    const PHASE_INCREMENT_FACTOR: f64 = 0.25;
    // These are recovered literals, not mathematical TAU and PI. The support
    // determines allocation while the cutoff ends the cosine window.
    const TABLE_PHASE_SUPPORT: f64 = 6.2853;
    const WINDOW_PHASE_CUTOFF: f64 = 3.1416;
    const TABLE_EXTENTS: [usize; 4] = [512, 1024, 2048, 4096];
    const MAX_TABLE_EXTENT: usize = 8192;
    let step =
        PHASE_INCREMENT_FACTOR / amplitudes.iter().map(|a| a / TANH_SLOPE).fold(0., f64::max);
    let requested = (TABLE_PHASE_SUPPORT / step) as usize;
    let extent = TABLE_EXTENTS
        .into_iter()
        .find(|n| requested <= *n)
        .unwrap_or(MAX_TABLE_EXTENT);
    let mut tables = vec![0i32; extent * 3];
    for (c, amplitude) in amplitudes.iter().enumerate() {
        let table = &mut tables[c * extent..(c + 1) * extent];
        for (i, v) in table[..extent / 2].iter_mut().enumerate() {
            let x = step * i as f64;
            let window = if x < WINDOW_PHASE_CUTOFF {
                (x.cos() + 1.) * 0.5
            } else {
                0.
            };
            *v = ((TANH_SLOPE * i as f64 / amplitude).tanh() * amplitude * window + 0.5) as i32;
        }
        for i in 1..extent / 2 {
            table[extent - i] = -table[i];
        }
        table[extent / 2] = table[extent / 2 + 1];
    }
    let mut matrix = multiply(COLOR_BASIS, camera);
    let mut w: [f64; 3] =
        std::array::from_fn(|r| matrix[r * 3] + matrix[r * 3 + 1] + matrix[r * 3 + 2]);
    let floor = neutral_weight(w) * NEUTRAL_ROW_FLOOR;
    w = w.map(|v| v.max(floor));
    let mean = neutral_weight(w);
    for (i, v) in matrix.iter_mut().enumerate() {
        *v *= mean / w[i / 3];
    }
    let matrix_green_sum = (matrix[3] + matrix[4] + matrix[5]) as f32;
    let normalizer = match scale {
        MatrixScale::UnitGreen => 1.0f32 / matrix_green_sum,
        MatrixScale::PreserveGain => digital,
    };
    // Higher digital gains select fewer fractional coefficient bits: Q12 above
    // four, Q13 from two through four, and Q14 below two.
    let shift = if digital > 4. {
        12
    } else if digital >= 2. {
        13
    } else {
        14
    };
    let coefficients =
        matrix.map(|v| ((1u32 << shift) as f64).mul_add(v * normalizer as f64, 0.5) as i32);
    Ok((
        Transform {
            output_scale: match scale {
                MatrixScale::UnitGreen => CodeScale::Normalized,
                MatrixScale::PreserveGain => CodeScale::Ready,
            },
            params: TransformParams {
                source: [width as u32, height as u32, extent as u32, shift],
                matrix: std::array::from_fn(|r| {
                    [
                        coefficients[r * 3],
                        coefficients[r * 3 + 1],
                        coefficients[r * 3 + 2],
                        0,
                    ]
                }),
                // Signed output code limits from the reference transform.
                bounds: [-500, 24000, 0, 0],
            },
            tables,
        },
        matrix_green_sum,
    ))
}
