//! Builds reference-derived ColorDQ lookup tables with an explicitly selected gain.
//!
//! Table construction is adapted from Foveonic's ColorDQ generator. Automatic gain selection in the reference depends on its noise setup.

use crate::source::{Metadata, Result};

pub struct Table {
    pub period: u32,
    pub values: Vec<i32>,
}

pub fn prepare(metadata: &Metadata, gain: f32) -> Result<Table> {
    table(
        metadata
            .fixed::<3>("ColorDQCamRGB")?
            .map(|v| v * gain as f64),
    )
}

pub fn table(amplitude: [f64; 3]) -> Result<Table> {
    if amplitude
        .iter()
        .any(|v| !v.is_finite() || *v <= 0. || *v > 256.)
    {
        return Err("ColorDQ amplitudes must be positive and at most 256 codes".into());
    }
    let tanh_slope = 0.8;
    let window_period_angle = f64::from_bits(0x40192425aee631f9);
    let window_cutoff_angle = f64::from_bits(0x400921ff2e48e8a7);
    let maximum = amplitude
        .iter()
        .map(|v| v / tanh_slope)
        .fold(0_f64, f64::max);
    let scale = 0.25 / maximum;
    let span = (window_period_angle / scale) as usize;
    let period = [512, 1024, 2048, 4096, 8192]
        .into_iter()
        .find(|p| *p >= span)
        .unwrap_or(8192);
    let mut values = vec![0_i32; period * 3];
    for i in 0..period / 2 {
        let angle = scale * i as f64;
        let window = if angle < window_cutoff_angle {
            (angle.cos() + 1.) * 0.5
        } else {
            0.
        };
        for c in 0..3 {
            let curved = (tanh_slope * i as f64 / amplitude[c]).tanh();
            values[i * 3 + c] = (amplitude[c] * window * curved + 0.5) as i32;
        }
    }
    for i in 1..period / 2 {
        for c in 0..3 {
            values[(period - i) * 3 + c] = -values[i * 3 + c];
        }
    }
    for c in 0..3 {
        values[period / 2 * 3 + c] = values[(period / 2 + 1) * 3 + c];
    }
    Ok(Table {
        period: period as u32,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lookup_is_signed_and_periodic() {
        let table = table([8., 12., 16.]).unwrap();
        let period = table.period as usize;
        assert!(period.is_power_of_two());
        for i in 1..period / 2 {
            for c in 0..3 {
                assert_eq!(table.values[i * 3 + c], -table.values[(period - i) * 3 + c]);
            }
        }
        assert!(super::table([0., 8., 8.]).is_err());
        let metadata = Metadata {
            width: 1,
            height: 1,
            rotation: 0,
            properties: Default::default(),
            lists: Default::default(),
            matrices: [(
                "ColorDQCamRGB".into(),
                crate::source::Matrix {
                    encoding: crate::source::MatrixEncoding::Float32,
                    representation: "f32",
                    dimensions: vec![3],
                    values: vec![8., 12., 16.],
                },
            )]
            .into(),
        };
        assert_eq!(prepare(&metadata, 1.).unwrap().values, table.values);
        assert_eq!(
            prepare(&metadata, 2.).unwrap().values,
            super::table([16., 24., 32.]).unwrap().values
        );
        for gain in [0., -1., f32::NAN, f32::INFINITY, 100.] {
            assert!(prepare(&metadata, gain).is_err());
        }
    }
}
