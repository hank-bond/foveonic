//! Estimates processing ISO from camera histograms and reference metadata.
//!
//! Adapted from Foveonic estimated_iso.rs at e47af35 with owner permission.

use crate::source::{Metadata, Result};

/// Camera-selected options for histogram estimation and optional ISO snapping.
pub struct Controls {
    pub bit_depth: u32,
    pub use_wb: bool,
    pub reject_outliers: bool,
    pub outlier_count: usize,
    pub snap: bool,
}

/// Calculate the estimate with native float32 accumulation and tails.
pub fn calculate(
    histogram: &[u8],
    percent: &[f32],
    reference: &[[f32; 3]],
    gains: [f64; 3],
    controls: Controls,
) -> Result<f64> {
    let Controls {
        bit_depth,
        use_wb,
        reject_outliers,
        outlier_count,
        snap,
    } = controls;
    let bins = percent.len();
    if bins == 0
        || histogram.len() < bins.checked_mul(3).ok_or("ISO histogram size overflow")?
        || reference.is_empty()
        || reference.len() > 15
        || bit_depth == 0
        || bit_depth > 15
        || gains.iter().any(|v| !v.is_finite() || *v <= 0.0)
        || percent.iter().any(|v| !v.is_finite())
        || percent.windows(2).any(|w| w[0] > w[1])
        || reference.iter().any(|r| {
            r.iter().any(|v| !v.is_finite()) || r[0] < percent[0] || r[0] > percent[bins - 1]
        })
        || reference.windows(2).any(|r| r[0][0] > r[1][0])
    {
        return Err("invalid estimated ISO inputs".into());
    }
    let minimum = gains[0].min(gains[1]).min(gains[2]);
    let factors = if use_wb {
        gains.map(|g| (g / minimum) as f32)
    } else {
        [1.0; 3]
    };
    if factors.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err("invalid normalized histogram WB factors".into());
    }
    let mono: Vec<u16> = (0..bins)
        .map(|i| {
            let r = factors[0] * histogram[i] as f32;
            let g = factors[1] * histogram[bins + i] as f32;
            let b = factors[2] * histogram[2 * bins + i] as f32;
            (r.max(g).max(b) as u32) as u16
        })
        .collect();
    let maximum = ((1_u32 << bit_depth) as f32) - 1.0;
    let mut cursor = 0;
    let mut ratios = Vec::with_capacity(reference.len());
    let mut valid = Vec::with_capacity(reference.len());
    for r in reference {
        while percent[cursor] < r[0] {
            cursor += 1;
        }
        let value = mono[cursor] as f32;
        ratios.push(if value == 0.0 {
            1.0
        } else {
            (maximum * r[1]) / value
        });
        valid.push(value < maximum);
    }
    let valid_count = valid.iter().filter(|&&v| v).count();
    if valid_count == 0 {
        return Ok(0.0);
    }
    let mut weights: Vec<f64> = reference
        .iter()
        .zip(&valid)
        .map(|(r, &ok)| (r[2] as f64) * if ok { 1.0 } else { 0.0 })
        .collect();
    if reject_outliers && valid_count > outlier_count {
        let mean = weighted_sum(&ratios, &weights) / weight_sum(&weights);
        let mut deviations: Vec<f32> = ratios
            .iter()
            .zip(&valid)
            .map(|(&v, &ok)| (v - mean).abs() * if ok { 1.0 } else { 0.0 })
            .collect();
        for _ in 0..outlier_count {
            let mut index = 0;
            let mut largest = -1000.0_f32;
            for (i, &d) in deviations.iter().enumerate() {
                if d > largest {
                    largest = d;
                    index = i;
                }
            }
            deviations[index] = 0.0;
            weights[index] = 0.0;
        }
    }
    let numerator = weighted_sum(&ratios, &weights) as f64;
    let denominator = weight_sum(&weights) as f64;
    if denominator == 0.0 {
        return Ok(0.0);
    }
    let estimate = (numerator * 100.0) / denominator;
    Ok(if snap { snap_iso(estimate) } else { estimate })
}

/// Choose the nearest native ISO stop, retaining table order for ties. Each
/// accepted distance narrows to float32 before the next double comparison.
/// The native search starts with a 10000-ISO radius. Outside that radius it
/// keeps the first stop, which is not the same as clamping to the largest stop.
pub fn snap_iso(estimate: f64) -> f64 {
    let stops = [
        100., 200., 400., 800., 1600., 3200., 6400., 133.33, 266.67, 533.33, 1066.7, 2133.3,
        4266.7, 8533.3, 166.67, 333.33, 666.67, 1333.3, 2666.7, 5333.3, 10667.,
    ];
    let initial = (estimate - stops[0]).abs();
    let mut distance = if initial < 10000. {
        initial as f32
    } else {
        10000.
    };
    let mut selected = 0;
    for (i, &stop) in stops.iter().enumerate().skip(1) {
        let candidate = (estimate - stop).abs();
        if candidate < distance as f64 {
            selected = i;
            distance = candidate as f32;
        }
    }
    stops[selected]
}

fn weight_sum(weights: &[f64]) -> f32 {
    weights.iter().fold(0.0_f32, |sum, &w| sum + w as f32)
}
fn weighted_sum(values: &[f32], weights: &[f64]) -> f32 {
    let vector_end = values.len() / 4 * 4;
    let mut sum = 0.0_f32;
    for i in 0..vector_end {
        sum += values[i] * weights[i] as f32;
    }
    for i in vector_end..values.len() {
        sum = values[i].mul_add(weights[i] as f32, sum);
    }
    sum
}

/// Read the supplied Merrill histogram/reference metadata and camera controls.
pub fn prepare(metadata: &Metadata, gains: [f64; 3]) -> Result<f64> {
    let scalar = |key: &str, default: f64| -> Result<f64> {
        if metadata.matrix(key).is_some() {
            metadata.scalar(key)
        } else {
            Ok(default)
        }
    };
    let hist = metadata.matrix("Histogram").ok_or("missing histogram")?;
    let refs = metadata
        .matrix("EstimatedISOreference")
        .ok_or("missing ISO references")?;
    // The decoder widens CAMF byte and float matrices to u32 and f64 storage.
    if hist.representation != "u32"
        || hist.dimensions.len() != 2
        || hist.dimensions[0] < 3
        || hist.dimensions[1] != metadata.vector("HistPercent")?.len()
        || hist.dimensions[0].checked_mul(hist.dimensions[1]) != Some(hist.values.len())
        || refs.representation != "f64"
        || refs.dimensions.len() != 2
        || refs.dimensions[1] != 3
        || refs.dimensions[0].checked_mul(3) != Some(refs.values.len())
    {
        return Err("invalid estimated ISO camera matrix layout".into());
    }
    let percent: Vec<f32> = metadata
        .vector("HistPercent")?
        .iter()
        .map(|&v| v as f32)
        .collect();
    let histogram: Vec<u8> = metadata
        .vector("Histogram")?
        .iter()
        .map(|&v| {
            if !(0.0..=255.0).contains(&v) || v.fract() != 0.0 {
                return Err("invalid byte histogram".into());
            }
            Ok(v as u8)
        })
        .collect::<Result<_>>()?;
    let values = metadata.vector("EstimatedISOreference")?;
    if values.len() % 3 != 0 {
        return Err("invalid ISO reference triples".into());
    }
    let reference: Vec<[f32; 3]> = values
        .as_chunks::<3>()
        .0
        .iter()
        .map(|r| r.map(|v| v as f32))
        .collect();
    let depth = metadata.scalar("HistBitDepth")?;
    let outliers = scalar("EstimatedISOoutliersNum", 3.0)?;
    if depth.fract() != 0.0 || outliers.fract() != 0.0 || outliers < 0.0 {
        return Err("invalid estimated ISO integer control".into());
    }
    calculate(
        &histogram,
        &percent,
        &reference,
        gains,
        Controls {
            bit_depth: depth as u32,
            use_wb: scalar("EstimatedISOWBGains", 1.0)? != 0.0,
            reject_outliers: scalar("EstimatedISOoutliers", 1.0)? != 0.0,
            outlier_count: outliers as usize,
            snap: scalar("EstimatedISOsnap", 0.)? != 0.,
        },
    )
}
