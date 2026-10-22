//! Prepares interchangeable-lens Quattro sparse phase-detection correction tables.

use crate::{
    nodes::sensor::{blue_pattern, fppd},
    source::{Metadata, Result},
};

// The gain catalogue supplies one extra endpoint beyond the corrected sites.
const CORRECTION_COLUMNS: usize = 340;
const CORRECTION_ROWS: u32 = 90;
// Origin x/y and spacing x/y, in each plane's physical pixel coordinates.
const CHROMA_GRID: [u32; 4] = [108, 232, 8, 16];
const LUMA_GRID: [u32; 4] = [217, 464, 16, 32];
const LENS_PROFILE_INDEX: usize = 8;
const CHIEF_RAY_UNITS_PER_DEGREE: f32 = 512.;
const ANGLE_KNOTS_DEGREES: [f32; 4] = [1.1, 4.7, 8.3, 11.9];
// Illumination catalogues have knots at 100%, 85%, and 70%.
const ILLUMINATION_KNOT_INTERVAL: f32 = 15.;
const GAIN_KNOTS: usize = 21;
const SUBDIVISIONS: usize = 17;
const GAIN_SAMPLES: usize = (GAIN_KNOTS - 1) * SUBDIVISIONS + 1;

fn expand(knots: &[f32]) -> Result<Vec<f32>> {
    if knots.len() != GAIN_KNOTS || knots.iter().any(|v| !v.is_finite()) {
        return Err("invalid phase-detection gain knots".into());
    }
    Ok((0..GAIN_SAMPLES)
        .map(|i| {
            let knot = i / SUBDIVISIONS;
            let remainder = i % SUBDIVISIONS;
            if remainder == 0 {
                knots[knot]
            } else {
                let difference = knots[knot + 1] - knots[knot];
                let product = difference * remainder as f32;
                knots[knot] + product / SUBDIVISIONS as f32
            }
        })
        .collect())
}
fn vector(m: &Metadata, name: &str) -> Result<Vec<f32>> {
    let table = m.matrix(name).ok_or_else(|| format!("missing {name}"))?;
    if table.values.len() != GAIN_KNOTS {
        return Err(format!("invalid {name} length").into());
    }
    Ok(table.values.iter().map(|&v| v as f32).collect())
}
fn blend(a: f32, b: f32, weight: f32) -> f32 {
    a.mul_add(weight, b * (1. - weight))
}

pub(super) fn chroma(m: &Metadata) -> Result<fppd::Prepared> {
    let mut gains = Vec::new();
    for channel in ['R', 'G', 'B'] {
        gains.extend(
            expand(&vector(m, &format!("StillFPPDGain_{channel}"))?)?
                .into_iter()
                .take(CORRECTION_COLUMNS),
        );
    }
    fppd::Prepared::new(
        [(m.width / 2) as u32, (m.height / 2) as u32],
        CHROMA_GRID,
        [CORRECTION_COLUMNS as u32, CORRECTION_ROWS],
        gains,
        false,
    )
}

pub(super) fn luma(m: &Metadata) -> Result<fppd::Prepared> {
    let value = |name: &str, default: f32| -> Result<f32> {
        match m.matrix(name) {
            Some(t) => t
                .values
                .get(LENS_PROFILE_INDEX)
                .map(|&v| v as f32)
                .ok_or_else(|| format!("short {name}").into()),
            None => Ok(default),
        }
    };
    let illumination = value("RelativeIlluminanceForInCamera", 85.)?.clamp(70., 100.);
    // CAMF stores chief-ray angle in 1/512-degree units.
    let angle =
        (value("ChiefRayAngleForInCamera", 4096.)? / CHIEF_RAY_UNITS_PER_DEGREE).clamp(1.1, 11.9);
    let (lo, hi, endpoint) = if illumination >= 85. {
        (0, 1, 100.)
    } else {
        (1, 2, 85.)
    };
    let illumination_weight = (endpoint - illumination) / ILLUMINATION_KNOT_INTERVAL;
    let angle_knots = ANGLE_KNOTS_DEGREES;
    let first = if angle < 4.7 {
        0
    } else if angle < 8.3 {
        1
    } else {
        2
    };
    let angle_weight =
        (angle_knots[first + 1] - angle) / (angle_knots[first + 1] - angle_knots[first]);
    let mut gains = Vec::new();
    for phase in ["B1", "B3"] {
        let mut at = Vec::new();
        for column in [first, first + 1] {
            let lower = vector(m, &format!("StillFPPDGain_{phase}_C{column}_R{lo}"))?;
            let upper = vector(m, &format!("StillFPPDGain_{phase}_C{column}_R{hi}"))?;
            at.push(
                lower
                    .iter()
                    .zip(upper)
                    .map(|(&a, b)| blend(b, a, illumination_weight))
                    .collect::<Vec<_>>(),
            );
        }
        let knots = at[0]
            .iter()
            .zip(&at[1])
            .map(|(&a, &b)| blend(a, b, angle_weight))
            .collect::<Vec<_>>();
        gains.extend(expand(&knots)?.into_iter().take(CORRECTION_COLUMNS));
    }
    let extent = [u32::try_from(m.width)?, u32::try_from(m.height)?];
    // This mask uses image severity before BPBadnessOffset. Later defect
    // replacement uses the separately adjusted pixel severity.
    let severity = super::defects::severity(m)?.image;
    let records = super::defects::expand_records(m, "BadPixelsLumaF23")?;
    let positions = records
        .into_iter()
        .filter_map(|[y, x, flags]| {
            (y > 1
                && y < extent[1] - 2
                && x > 1
                && x < extent[0] - 1
                && severity <= ((flags >> 8) & 7))
                .then_some(y * extent[0] + x)
        })
        .collect();
    fppd::Prepared::new(
        extent,
        LUMA_GRID,
        [CORRECTION_COLUMNS as u32, CORRECTION_ROWS],
        gains,
        true,
    )?
    .with_defects(positions)
}

/// The reference reads capture metadata here, independently of edit controls.
pub(super) fn pattern(m: &Metadata) -> Result<Option<blue_pattern::Prepared>> {
    let aperture = m
        .matrix("CaptureAperture")
        .map(|v| v.values[0] as f32)
        .unwrap_or(8.);
    if !aperture.is_finite() || aperture <= 0. {
        return Err("invalid blue-pattern capture aperture".into());
    }
    if aperture >= 6. {
        return Ok(None);
    }
    let strength = if aperture <= 3. {
        1.
    } else {
        1. + (aperture - 3.) / -3.
    };
    Ok(Some(blue_pattern::Prepared::new(
        [u32::try_from(m.width)?, u32::try_from(m.height)?],
        strength,
        m.scalar("SensorISO")? as f32,
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires QUATTRO_X3F and FPPD_GAIN_FIXTURES"]
    fn original_camera_gain_tables_match_independent_oracle() {
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let directory = std::path::PathBuf::from(std::env::var_os("FPPD_GAIN_FIXTURES").unwrap());
        for (name, prepared) in [
            ("chroma", chroma(&source.metadata).unwrap()),
            ("luma", luma(&source.metadata).unwrap()),
        ] {
            let expected = std::fs::read(directory.join(format!("{name}.bin"))).unwrap();
            assert_eq!(
                bytemuck::cast_slice::<f32, u8>(prepared.gains()),
                expected,
                "{name}"
            );
        }
    }
    #[test]
    fn interpolation_retains_endpoints_and_rounding() {
        let knots = (0..21).map(|v| v as f32 / 8.).collect::<Vec<_>>();
        let values = expand(&knots).unwrap();
        assert_eq!(values.len(), 341);
        assert_eq!(values[0], 0.);
        assert_eq!(values[340], 2.5);
        assert_eq!(values[1], 0.125 / 17.);
    }
}
