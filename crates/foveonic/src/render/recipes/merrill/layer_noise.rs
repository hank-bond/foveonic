//! Prepares native layer noise and shield correction in camera color coordinates.

use crate::{
    nodes::sensor::{calibration, multiscale, row_guard, shield_noise},
    source::{Metadata, Result},
};

const NUMERICAL_FLOOR: f64 = 1e-12;
const OUTPUT_NEUTRAL: [f64; 3] = [1.; 3];
const GREEN_MAGENTA: [f64; 3] = [-0.5, 1., -0.5];

pub(super) fn prepare_row_guard(metadata: &Metadata) -> Result<row_guard::Prepared> {
    let iso = metadata
        .properties
        .get("ISO")
        .ok_or("missing camera ISO for native row correction")?
        .parse::<u32>()?;
    row_guard::Prepared::new(
        [metadata.width as u32, metadata.height as u32],
        calibration::rectangle(metadata, "ActiveImageArea")?,
        metadata.fixed::<9>("ColorNoiseModel")?.map(|v| v as f32),
        iso,
    )
}

pub(super) fn prepare(
    metadata: &Metadata,
    white_balance: &str,
    gains: [f32; 3],
    columns: bool,
) -> Result<(shield_noise::Prepared, multiscale::Prepared)> {
    let extent = [metadata.width as u32, metadata.height as u32];
    let active = calibration::rectangle(metadata, "ActiveImageArea")?;
    let bottom = calibration::rectangle(metadata, "DarkShieldTop")?;
    let ranges = metadata.fixed::<4>("DarkShieldColRange")?;
    if ranges
        .iter()
        .any(|v| !v.is_finite() || *v < 0. || v.fract() != 0.)
        || ranges[2] > ranges[3]
        || ranges[3] >= metadata.width as f64
    {
        return Err("invalid right dark-shield bounds".into());
    }
    let bottom = [active[0], bottom[1], active[2], bottom[3]];
    let right = [ranges[2] as u32, active[1], ranges[3] as u32 + 1, active[3]];
    // Noise correction uses camera coordinates before the selected photographic
    // look. Editable DNG and photographic rendering therefore share this basis.
    let color = calibration::color(metadata, white_balance, &super::dng_base::unselected_mode())?;
    let matrix =
        std::array::from_fn(|i| f64::from(color.matrix[i / 3][i % 3]) * f64::from(gains[i % 3]));
    let (neutral, direction) = basis(matrix)?;
    let noise = metadata.fixed::<9>("ColorNoiseModel")?.map(|v| v as f32);
    let shields = shield_noise::Prepared::new(extent, active, bottom, right, direction, columns)?;
    let filter = multiscale::Prepared::new(extent, active, noise, neutral)?;
    Ok((shields, filter))
}

fn basis(matrix: [f64; 9]) -> Result<([f32; 3], [f32; 3])> {
    let inverse = crate::color_space::inverse(matrix);
    if inverse.iter().any(|v| !v.is_finite()) {
        return Err("singular native noise color basis".into());
    }
    let transform = |v: [f64; 3]| -> [f64; 3] {
        std::array::from_fn(|r| (0..3).map(|c| inverse[r * 3 + c] * v[c]).sum())
    };
    let neutral = transform(OUTPUT_NEUTRAL);
    let mut direction = transform(GREEN_MAGENTA);
    let neutral_sum: f64 = neutral.iter().sum();
    if !neutral_sum.is_finite() || neutral_sum.abs() < NUMERICAL_FLOOR {
        return Err("invalid native noise neutral direction".into());
    }
    let brightness = direction.iter().sum::<f64>() / neutral_sum;
    for c in 0..3 {
        direction[c] -= brightness * neutral[c];
    }
    let neutral = neutral.map(|v| v as f32);
    let direction = direction.map(|v| v as f32);
    if neutral.iter().chain(&direction).any(|v| !v.is_finite()) {
        return Err("invalid native noise color directions".into());
    }
    Ok((neutral, direction))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_basis_preserves_layer_sum_and_neutral_color() {
        let matrix = [1.2, -0.4, 0.1, -0.2, 1.1, -0.1, 0.1, -0.3, 0.9];
        let (neutral, direction) = basis(matrix).unwrap();
        assert!(direction.iter().sum::<f32>().abs() < 1e-6);
        let rgb: [f64; 3] = std::array::from_fn(|r| {
            (0..3)
                .map(|c| matrix[r * 3 + c] * f64::from(neutral[c]))
                .sum()
        });
        assert!(rgb.iter().all(|v| (v - 1.).abs() < 1e-6));
        assert!(basis([0.; 9]).is_err());
        assert!(basis([f64::NAN; 9]).is_err());
    }
}
