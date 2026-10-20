//! Pinned CPU oracle from the reference implementation at 2b0dd443.
//! Copyright 2026 Sang Lee and contributors. Apache-2.0. See ../NOTICE.
//! Estimate a starting brightness for each affected Merrill pixel.
//! Use camera luminance where possible and layer-based estimates for severe clipping.
//! Nearby pixels supply no color at this stage. Missing brightness is estimated,
//! not measured.

pub(super) struct Result {
    pub samples: [f64; 3],
    pub amplitude: f64,
    pub strength: f64,
    pub available: bool,
}

pub(super) fn recover(measured: [f64; 3], mask: [u8; 3], neutral: [f64; 3]) -> Result {
    let mut result = Result {
        samples: measured,
        amplitude: 0.0,
        strength: 0.0,
        available: false,
    };
    if mask == [255; 3]
        || measured.iter().any(|v| !v.is_finite())
        || neutral.iter().any(|v| !v.is_finite() || *v <= 0.0)
    {
        return result;
    }
    let weights = mask.map(|v| (v as f64 / 255.0).powi(2));
    let support: f64 = weights.iter().sum();
    if support == 0.0 {
        // No reliable layer remains. This estimate cannot supply texture or color.
        return result;
    }
    let amplitude = (0..3)
        .map(|c| weights[c] * (measured[c] / neutral[c]))
        .sum::<f64>()
        / support;
    if !amplitude.is_finite() || amplitude < 0.0 {
        return result;
    }
    // Recovery strength rises smoothly as the lowest layer reliability falls.
    // Nearby color and the rendered RGB values do not set this strength.
    let damage = 1.0 - mask.into_iter().min().unwrap_or(255) as f64 / 255.0;
    let t = (2.0 * damage).clamp(0.0, 1.0);
    let strength = t * t * (3.0 - 2.0 * t);
    result.samples = std::array::from_fn(|c| {
        if strength == 1.0 {
            amplitude * neutral[c]
        } else {
            measured[c] + strength * (amplitude * neutral[c] - measured[c])
        }
    });
    result.amplitude = amplitude;
    result.strength = strength;
    result.available = true;
    result
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

fn recover_luminance(measured: [f64; 3], mask: [u8; 3], neutral: [f64; 3], y: [f64; 3]) -> Result {
    let mut result = recover(measured, mask, neutral);
    if !result.available {
        return result;
    }
    let neutral_y = dot(y, neutral);
    let target_y = dot(y, measured);
    if !neutral_y.is_finite()
        || neutral_y <= 1e-12
        || !target_y.is_finite()
        || target_y <= 0.0
        || !(target_y / neutral_y).is_finite()
    {
        return Result {
            samples: measured,
            amplitude: 0.0,
            strength: 0.0,
            available: false,
        };
    }
    let amplitude = target_y / neutral_y;
    result.samples = std::array::from_fn(|c| {
        if result.strength == 1.0 {
            amplitude * neutral[c]
        } else {
            measured[c] + result.strength * (amplitude * neutral[c] - measured[c])
        }
    });
    result.amplitude = amplitude;
    result
}

// Severe clipping uses the neutral layer ratios to estimate brightness.
// That estimate is not a physical brightness bound for colored material.
pub(super) fn recover_severe(
    measured: [f64; 3],
    mask: [u8; 3],
    neutral: [f64; 3],
    y: [f64; 3],
) -> Result {
    let hold = recover_luminance(measured, mask, neutral, y);
    let mut sorted = mask;
    sorted.sort_unstable();
    if sorted[1] >= 128
        || measured.iter().any(|v| !v.is_finite() || *v < 0.0)
        || neutral.iter().any(|v| !v.is_finite() || *v <= 0.0)
    {
        return hold;
    }
    let neutral_y = dot(y, neutral);
    let measured_y = dot(y, measured);
    if !neutral_y.is_finite() || neutral_y <= 1e-12 || !measured_y.is_finite() {
        return hold;
    }
    let h = |x: f64| {
        let x = x.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    let severity = h(1.0 - 2.0 * sorted[1] as f64 / 255.0);
    let trust = h(2.0 * sorted[2] as f64 / 255.0);
    let amplitudes: [f64; 3] = std::array::from_fn(|c| measured[c] / neutral[c]);
    let envelope = amplitudes.into_iter().fold(0.0_f64, f64::max);
    let weights = mask.map(|v| (v as f64 / 255.0).powi(2));
    let support: f64 = weights.iter().sum();
    let mean = if support > 0.0 {
        dot(weights, amplitudes) / support
    } else {
        envelope
    };
    let guarded = trust * mean + (1.0 - trust) * envelope;
    // Severe clipping can make matrix luminance negative. A zero floor
    // prevents that value from reducing the layer-based brightness estimate
    // and keeps the blend continuous at zero luminance.
    let held = (measured_y / neutral_y).max(0.0);
    let amplitude = held + severity * (guarded - held);
    if !amplitude.is_finite() || amplitude <= 0.0 {
        return hold;
    }
    // With two layers below half reliability, recovery strength is exactly one.
    // Include fully clipped pixels so the estimate remains continuous.
    Result {
        samples: neutral.map(|v| amplitude * v),
        amplitude,
        strength: 1.0,
        available: true,
    }
}
