//! Pinned CPU oracle from the reference implementation at 2b0dd443.
//! Copyright 2026 Sang Lee and contributors. Apache-2.0. See ../NOTICE.
//! Add nearby color ratios without changing the chosen highlight brightness.
//! Without usable color evidence, keep the tone result.

use super::tone_anchor;

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

pub(super) fn apply(
    measured: [f64; 3],
    tone: &tone_anchor::Result,
    neutral: [f64; 3],
    y: [f64; 3],
    direction: [f64; 3],
) -> [f64; 3] {
    if !tone.available
        || tone.strength <= 0.0
        || direction.iter().any(|v| !v.is_finite() || *v <= 0.0)
    {
        return tone.samples;
    }
    let target = neutral.map(|v| v * tone.amplitude);
    let target_y = dot(y, target);
    let donor_y = dot(y, direction);
    if !target_y.is_finite() || target_y <= 0.0 || !donor_y.is_finite() || donor_y <= 1e-12 {
        return tone.samples;
    }
    let colored = direction.map(|v| v * (target_y / donor_y));
    if colored.iter().any(|v| !v.is_finite()) {
        return tone.samples;
    }
    // The field resolves color confidence. Apply recovery onset only once.
    // Keep the target + (colored - target) expression to preserve rounding.
    // Replacing that expression with the `colored` value changes floating-point results.
    std::array::from_fn(|c| {
        let target = target[c] + (colored[c] - target[c]);
        if tone.strength == 1.0 {
            target
        } else {
            measured[c] + tone.strength * (target - measured[c])
        }
    })
}
