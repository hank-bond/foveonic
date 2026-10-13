//! Prepares a camera-derived color-mode tone table for application in a shader.
//!
//! Adapted from Foveonic's color tone generation. This module operates on metadata and curve samples.

use crate::source::{MatrixEncoding, Metadata, Result};

const RECORD_VALUES: usize = 7;
const LUT_ENTRIES: usize = 65536;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct Controls {
    pub contrast: f64,
    pub shadows: f64,
    pub highlights: f64,
}

struct Parameters {
    start: f64,
    end: f64,
    lower: f64,
    upper: f64,
    join: f64,
    scale: f64,
}

fn parameters(metadata: &Metadata, mode: u32) -> Result<Parameters> {
    let scalar = |name: &str, default: f64| {
        metadata
            .matrix(name)
            .filter(|m| m.encoding == MatrixEncoding::Float32 && m.dimensions == [1])
            .map_or(default, |m| m.values[0])
    };
    let mut p = Parameters {
        start: scalar("TCFStart", -1.),
        end: scalar("TCFEnd", 1.15),
        lower: scalar("TCFSteepness", 3.),
        upper: scalar("TCFSteepness2", 2.),
        join: scalar("TCFBreak", 0.2),
        scale: scalar("TCScale", 1.),
    };
    if p.start >= p.end {
        p = Parameters {
            start: -1.,
            end: 1.15,
            lower: 3.,
            upper: 2.,
            join: 0.2,
            scale: 1.,
        };
    }
    let matrix = metadata
        .matrix("TCColorModeSettings")
        .filter(|m| m.encoding == MatrixEncoding::Float32 && m.dimensions.len() == 2);
    let mut fallback = true;
    if let Some(m) = matrix {
        if m.dimensions[1] != RECORD_VALUES
            || m.dimensions[0] == 0
            || m.values.len() != m.dimensions[0] * RECORD_VALUES
        {
            return Err("invalid camera tone records".into());
        }
        let index = if (mode as usize) < m.dimensions[0] {
            mode as usize
        } else {
            0
        };
        let row = &m.values[index * RECORD_VALUES..(index + 1) * RECORD_VALUES];
        p = Parameters {
            scale: row[0],
            start: row[1],
            end: row[2],
            lower: row[3],
            join: row[5],
            upper: row[6],
        };
        fallback = m.values[m.values.len() - RECORD_VALUES] == 8.;
    }
    if fallback {
        p = Parameters {
            scale: 1.,
            start: -1.17,
            end: 1.55,
            lower: 3.,
            upper: 1.85,
            join: 0.1,
        };
    }
    Ok(p)
}

pub fn prepare(
    metadata: &Metadata,
    mode: &crate::color_mode::Settings,
    mut controls: Controls,
) -> Result<Vec<f32>> {
    let depth = metadata
        .matrix("ImageDepth")
        .filter(|m| m.encoding == MatrixEncoding::Uint16 && m.dimensions == [1])
        .map_or(12, |m| m.values[0] as u32);
    if !(1..=16).contains(&depth) {
        return Err("unsupported camera image depth".into());
    }
    controls.contrast += mode.contrast;
    generate(
        ((1_u32 << depth) - 1) as f64,
        &parameters(metadata, mode.tone_index)?,
        controls,
    )
}

fn generate(saturation: f64, p: &Parameters, controls: Controls) -> Result<Vec<f32>> {
    if [
        saturation,
        p.start,
        p.end,
        p.lower,
        p.upper,
        p.join,
        p.scale,
        controls.contrast,
        controls.shadows,
        controls.highlights,
    ]
    .iter()
    .any(|v| !v.is_finite())
        || saturation <= 0.
        || saturation >= LUT_ENTRIES as f64
    {
        return Err("invalid tone parameters".into());
    }
    let curve_saturation = saturation.max(4095.);
    let last = curve_saturation as usize;
    let contrast = (controls.contrast.clamp(-2., 2.) * 0.5).exp2();
    let lower = p.lower * contrast;
    let upper = p.upper * contrast;
    // Reference color controls warp the curve endpoints by eighth-stop exponents.
    let start = p.start * (-controls.shadows.clamp(-2., 2.) / 8.).exp2();
    let end = p.end * (controls.highlights.clamp(-2., 2.) / 8.).exp2();
    let sigmoid = |steep: f64, t: f64| {
        let warped = (end - start).mul_add(t.powf(1. / 2.2), start);
        1. / (1. + (-steep * warped).exp())
    };
    let mut raw = vec![0_f32; last + 1];
    if p.join < 1. {
        let step = 1. / saturation;
        let mut t = 0.;
        let mut index = 0;
        loop {
            raw[index] = sigmoid(lower, t) as f32;
            t += step;
            index += 1;
            if t >= p.join || index > last {
                break;
            }
        }
        let delta = sigmoid(lower, t) - sigmoid(upper, t);
        for value in &mut raw[index..] {
            *value = (sigmoid(upper, t) + delta) as f32;
            t += step;
        }
    } else {
        let peak = sigmoid(lower, curve_saturation / saturation);
        for (index, value) in raw.iter_mut().enumerate() {
            *value = (sigmoid(lower, index as f64 / saturation) / peak) as f32;
        }
    }
    // The reference smooths the sampled curve as a Bezier curve with control samples every 50 codes.
    let mut controls: Vec<f32> = (0..=last).step_by(50).map(|i| raw[i]).collect();
    controls.push(raw[last]);
    let mut work = vec![0.; controls.len()];
    let reciprocal = 1. / curve_saturation as f32;
    for (i, value) in raw.iter_mut().enumerate() {
        let t = reciprocal * i as f32;
        work.copy_from_slice(&controls);
        for n in (1..work.len()).rev() {
            for k in 0..n {
                work[k] = (1. - t).mul_add(work[k], t * work[k + 1]);
            }
        }
        *value = work[0];
    }
    let low = raw[0] as f64;
    let span = raw[last] as f64 - low;
    let reference = (raw[saturation as usize] as f64 - low) / span;
    let normalization = saturation / reference;
    for value in &mut raw {
        let encoded = (*value as f64 - low) / span;
        let linear = if encoded < 0.04045 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        };
        *value = (normalization * linear) as f32;
    }
    let factor = p.scale / raw[last] as f64;
    let mut table = vec![0_f32; LUT_ENTRIES];
    for (value, &sample) in table[..last].iter_mut().zip(&raw) {
        *value = (sample as f64 * factor).clamp(0., 1.) as f32;
    }
    let tail = table[last - 1];
    table[last..].fill(tail);
    if table.iter().any(|v| !v.is_finite()) {
        return Err("invalid generated tone curve".into());
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curve_has_black_and_monotonic_bounded_samples() {
        let p = Parameters {
            start: -1.17,
            end: 1.65,
            lower: 3.,
            upper: 1.7,
            join: 0.1,
            scale: 1.,
        };
        let curve = generate(4095., &p, Controls::default()).unwrap();
        assert_eq!(curve[0], 0.);
        assert!(curve.windows(2).all(|v| v[0] <= v[1]));
        assert!(curve[4094] > 0.99);
        assert_eq!(curve[4095], curve[65535]);
        for controls in [
            Controls {
                contrast: 1.,
                ..Controls::default()
            },
            Controls {
                shadows: 1.,
                ..Controls::default()
            },
            Controls {
                highlights: 1.,
                ..Controls::default()
            },
            Controls {
                contrast: -1.,
                shadows: -1.,
                highlights: -1.,
            },
        ] {
            let adjusted = generate(4095., &p, controls).unwrap();
            assert_eq!(adjusted[0], 0.);
            assert!(adjusted
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)));
            assert!(adjusted.windows(2).all(|v| v[0] <= v[1]));
            assert!(curve
                .iter()
                .zip(adjusted)
                .any(|(a, b)| (a - b).abs() > 1e-4));
        }
    }
}
