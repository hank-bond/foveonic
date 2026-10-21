//! Prepare Merrill spatial gain grids from camera byte tables, aperture, focus, and chief-ray angle.

// Metadata table preparation imported from Foveonic. Image application runs in shaders.
use crate::source::{Metadata, Result};

pub struct Table {
    pub values: Vec<f32>,
    pub width: usize,
    pub height: usize,
}

/// Prepare the selected full-resolution Merrill grids without image fitting.
///
/// The camera stores byte tables plus textual minimum/delta values. Decode each
/// table before reciprocal-aperture interpolation, then combine infinity and
/// minimum-object-distance tables using ScaleENC. Native extrapolation beyond
/// the last aperture is retained. Positive ChiefRayAngle selects the camera's
/// angle-indexed grids instead, with squared-aperture and angular interpolation.
pub fn prepare(metadata: &Metadata, aperture: f32) -> Result<[Table; 3]> {
    if chief_ray_angle(metadata) > 0.0 {
        // The angle-indexed route does not read the focus encoder.
        return prepare_at_focus(metadata, aperture, 0);
    }
    let focus = metadata
        .matrix("ScaleENC")
        .filter(|m| m.encoding != crate::source::MatrixEncoding::Int16 && m.dimensions == [1])
        .map(|m| m.values[0])
        .unwrap_or(0.0);
    if !focus.is_finite() || focus.fract() != 0.0 || !(0.0..=31.0).contains(&focus) {
        return Err("invalid spatial gain focus encoder".into());
    }
    prepare_at_focus(metadata, aperture, focus as i32)
}

fn chief_ray_angle(metadata: &Metadata) -> f32 {
    metadata
        .matrix("ChiefRayAngle")
        .filter(|m| m.encoding == crate::source::MatrixEncoding::Float32 && m.dimensions == [1])
        .map(|m| m.values[0] as f32)
        .unwrap_or(0.0)
}

fn prepare_at_focus(metadata: &Metadata, aperture: f32, focus: i32) -> Result<[Table; 3]> {
    let chief_ray = chief_ray_angle(metadata);
    if !chief_ray.is_finite() || !aperture.is_finite() || aperture <= 0.0 {
        return Err("invalid spatial gain interpolation inputs".into());
    }
    if chief_ray > 0.0 {
        return interpolate_angle(metadata, aperture, chief_ray);
    }
    let stops = metadata
        .matrix("SpatialGain_Fstop")
        .ok_or("missing spatial gain aperture list")?;
    let stops: Vec<f32> = if stops.encoding == crate::source::MatrixEncoding::Float32
        && stops.dimensions == [stops.values.len()]
        && (2..=10).contains(&stops.values.len())
    {
        stops.values.iter().map(|v| *v as f32).collect()
    } else {
        return Err("invalid spatial gain aperture list".into());
    };
    if !aperture.is_finite()
        || aperture <= 0.0
        || !(0..=31).contains(&focus)
        || stops.iter().any(|v| !v.is_finite() || *v <= 0.0)
        || stops.windows(2).any(|v| v[0] >= v[1])
    {
        return Err("invalid spatial gain interpolation inputs".into());
    }
    let high = if aperture <= stops[0] {
        0
    } else {
        stops
            .iter()
            .position(|v| *v >= aperture)
            .unwrap_or(stops.len() - 1)
    };
    let infinity_weight = (31.0 - focus as f64) / 31.0;
    let near_weight = 1.0 - infinity_weight;
    let channel = |color: char| -> Result<Table> {
        let infinity = interpolate_aperture(metadata, color, "INF", &stops, high, aperture)?;
        let near = interpolate_aperture(metadata, color, "MOD", &stops, high, aperture)?;
        if infinity.width != near.width || infinity.height != near.height {
            return Err("spatial gain focus table geometry differs".into());
        }
        let values = infinity
            .values
            .iter()
            .zip(&near.values)
            .map(|(&a, &b)| {
                let a = (a as f64 * infinity_weight) as f32;
                let b = (b as f64 * near_weight) as f32;
                a + b
            })
            .collect();
        Ok(Table {
            values,
            width: infinity.width,
            height: infinity.height,
        })
    };
    Ok([channel('R')?, channel('G')?, channel('B')?])
}

fn interpolate_aperture(
    metadata: &Metadata,
    color: char,
    distance: &str,
    stops: &[f32],
    high: usize,
    aperture: f32,
) -> Result<Table> {
    if high == 0 {
        return decode(metadata, color, 0, distance);
    }
    let mut low_table = decode(metadata, color, high - 1, distance)?;
    let high_table = decode(metadata, color, high, distance)?;
    if low_table.width != high_table.width || low_table.height != high_table.height {
        return Err("spatial gain aperture table geometry differs".into());
    }
    let low_weight = (1.0 / aperture as f64 - 1.0 / stops[high] as f64)
        / (1.0 / stops[high - 1] as f64 - 1.0 / stops[high] as f64);
    let high_weight = (1.0 - low_weight) as f32;
    for (low, &high) in low_table.values.iter_mut().zip(&high_table.values) {
        let product = (*low as f64 * low_weight) as f32;
        *low = high_weight.mul_add(high, product);
    }
    Ok(low_table)
}

/// Reference 0xb058c normalizes the requested angle against the camera's `50`
/// calibration, then blends with either its `30` or `85` calibration.
/// These are recovered angular-sampling constants, not fitted coefficients.
fn interpolate_angle(metadata: &Metadata, aperture: f32, chief_ray: f32) -> Result<[Table; 3]> {
    // The reference checks this calibration list even when blending with 85.
    if metadata
        .lists
        .get("SpatialGainsProps_1.4_30")
        .is_none_or(|p| p.is_empty())
    {
        return Err("missing spatial gain properties".into());
    }
    let half_aperture = 0.5 / aperture as f64;
    let sample_angle = |offset: f64, lower: f32, upper: f64| {
        let low = ((offset - half_aperture) as f32).max(lower) as f64;
        (((half_aperture + offset).min(upper) + low) * 0.5).atan()
    };
    let middle_angle = sample_angle(
        f64::from_bits(0x3fc3_c50a_f08f_bd56),
        f32::from_bits(0xbd28_cf0c),
        f64::from_bits(0x3fdd_7343_4719_bb36),
    );
    // Widening before dividing would change rounding. Match the reference
    // request's float32 division first.
    let angle_coordinate = (std::f64::consts::PI / middle_angle) * (chief_ray / 180.0) as f64;
    let (other_name, middle_weight, other_weight) = if aperture >= 2.0 && angle_coordinate >= 1.0 {
        let other_angle = sample_angle(
            f64::from_bits(0x3fcf_a730_ea29_9de5),
            f32::from_bits(0xbd34_fbe9),
            f64::from_bits(0x3fdd_23b6_93fe_1ec4),
        ) / middle_angle;
        (
            "85",
            (other_angle - angle_coordinate) / (other_angle - 1.0),
            (angle_coordinate - 1.0) / (other_angle - 1.0),
        )
    } else {
        let other_angle = sample_angle(
            f64::from_bits(0x3f8a_cf4e_4e15_9df0),
            f32::from_bits(0xbccc_8007),
            f64::from_bits(0x3fd4_17db_677f_f305),
        ) / middle_angle;
        (
            "30",
            (angle_coordinate - other_angle) / (1.0 - other_angle),
            (1.0 - angle_coordinate) / (1.0 - other_angle),
        )
    };
    let channel = |color| -> Result<Table> {
        let mut middle = interpolate_angle_aperture(metadata, color, "50", aperture)?;
        let other = interpolate_angle_aperture(metadata, color, other_name, aperture)?;
        if middle.width != other.width || middle.height != other.height {
            return Err("spatial gain angle table geometry differs".into());
        }
        for (value, &other) in middle.values.iter_mut().zip(&other.values) {
            // Rounding each product before adding matches the native float32 result.
            // Rounding only the final sum gives different shading coefficients.
            *value = (*value as f64 * middle_weight) as f32 + (other as f64 * other_weight) as f32;
        }
        Ok(middle)
    };
    Ok([channel('R')?, channel('G')?, channel('B')?])
}

fn interpolate_angle_aperture(
    metadata: &Metadata,
    color: char,
    angle: &str,
    aperture: f32,
) -> Result<Table> {
    // Fixed Double knots and names recovered at 0xd5f98 and 0xd5fb8.
    const STOPS: [f64; 4] = [1.4, 2.0, 2.8, 8.0];
    const NAMES: [&str; 4] = ["1.4", "2.0", "2.8", "8.0"];
    let high = STOPS
        .iter()
        .position(|&stop| stop >= aperture as f64)
        .unwrap_or(3);
    let decode = |index: usize| {
        decode_properties(
            metadata,
            color,
            &format!("SpatialGainsProps_{}_{angle}", NAMES[index]),
        )
    };
    if high == 0 || aperture as f64 >= STOPS[3] {
        return decode(high);
    }
    let mut low = decode(high - 1)?;
    let high_table = decode(high)?;
    if low.width != high_table.width || low.height != high_table.height {
        return Err("spatial gain aperture table geometry differs".into());
    }
    // This route uses squared aperture, not the focus route's reciprocal.
    // Keep the request's square rounded before the double-precision blend.
    let high_squared = STOPS[high] * STOPS[high];
    let low_weight = (high_squared - (aperture * aperture) as f64)
        / (high_squared - STOPS[high - 1] * STOPS[high - 1]);
    let high_weight = (1.0 - low_weight) as f32;
    for (value, &high) in low.values.iter_mut().zip(&high_table.values) {
        *value = high_weight.mul_add(high, (*value as f64 * low_weight) as f32);
    }
    Ok(low)
}

fn decode(metadata: &Metadata, color: char, index: usize, distance: &str) -> Result<Table> {
    decode_properties(
        metadata,
        color,
        &format!("SpatialGainsProps_{index}_{distance}"),
    )
}

fn decode_properties(metadata: &Metadata, color: char, name: &str) -> Result<Table> {
    let properties = metadata
        .lists
        .get(name)
        .ok_or("missing spatial gain properties")?;
    let number = |name: &str| -> Result<f64> {
        let value: f64 = properties
            .get(name)
            .ok_or("missing spatial gain property")?
            .replace(',', ".")
            .parse()?;
        if value == 0.0 {
            return Err("invalid spatial gain property".into());
        }
        Ok(value)
    };
    let minimum = number(&format!("MinGains{color}"))? as f32;
    let delta = number(&format!("Delta{color}"))?;
    let name = properties
        .get(&format!("GainsTable{color}"))
        .ok_or("missing spatial gain table selection")?;
    let matrix = metadata
        .matrix(name)
        .ok_or("missing spatial gain byte table")?;
    if matrix.encoding != crate::source::MatrixEncoding::Uint8
        || matrix.dimensions.len() != 2
        || matrix.dimensions.contains(&0)
        || matrix.dimensions[0].checked_mul(matrix.dimensions[1]) != Some(matrix.values.len())
        || matrix
            .values
            .iter()
            .any(|v| !v.is_finite() || v.fract() != 0.0 || !(0.0..=255.0).contains(v))
    {
        return Err("invalid spatial gain byte table".into());
    }
    let values = matrix
        .values
        .iter()
        .map(|&byte| minimum + (byte * delta) as f32)
        .collect();
    Ok(Table {
        values,
        width: matrix.dimensions[1],
        height: matrix.dimensions[0],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Matrix;
    use std::collections::BTreeMap;

    fn metadata() -> Metadata {
        Metadata {
            processing_version: None,
            width: 4928,
            height: 3264,
            rotation: 0,
            properties: BTreeMap::new(),
            matrices: BTreeMap::new(),
            lists: BTreeMap::new(),
        }
    }

    #[test]
    fn missing_angle_grids_are_explicit() {
        let mut m = metadata();
        m.matrices.insert(
            "ChiefRayAngle".into(),
            Matrix {
                encoding: crate::source::MatrixEncoding::Float32,
                representation: "f64",
                dimensions: vec![1],
                values: vec![1.0],
            },
        );
        assert!(prepare(&m, 4.0)
            .err()
            .unwrap()
            .to_string()
            .contains("spatial gain properties"));
    }

    #[test]
    fn angle_route_ignores_focus_encoder() {
        let mut m = metadata();
        for (name, value) in [("ChiefRayAngle", 20.0), ("ScaleENC", 65535.0)] {
            m.matrices.insert(
                name.into(),
                Matrix {
                    encoding: crate::source::MatrixEncoding::Float32,
                    representation: "f64",
                    dimensions: vec![1],
                    values: vec![value],
                },
            );
        }
        assert!(prepare(&m, 4.0)
            .err()
            .unwrap()
            .to_string()
            .contains("spatial gain properties"));
    }

    #[test]
    fn angle_aperture_clamps_and_interpolates_squared_request() {
        let mut m = metadata();
        for (stop, minimum) in [("1.4", 1.0), ("2.0", 2.0), ("2.8", 3.0), ("8.0", 4.0)] {
            m.lists.insert(
                format!("SpatialGainsProps_{stop}_50"),
                crate::source::PropertyList::new(vec![
                    ("MinGainsR".into(), minimum.to_string()),
                    ("DeltaR".into(), "0.01".into()),
                    ("GainsTableR".into(), "grid".into()),
                ])
                .unwrap(),
            );
        }
        m.matrices.insert(
            "grid".into(),
            Matrix {
                encoding: crate::source::MatrixEncoding::Uint8,
                representation: "u32",
                dimensions: vec![1, 1],
                values: vec![0.0],
            },
        );
        for (aperture, expected) in [(0.7, 1.0), (2.0, 2.0), (8.0, 4.0), (11.0, 4.0)] {
            assert_eq!(
                interpolate_angle_aperture(&m, 'R', "50", aperture)
                    .unwrap()
                    .values,
                [expected]
            );
        }
        // The interior knot uses squared aperture, not reciprocal aperture.
        let value = interpolate_angle_aperture(&m, 'R', "50", 1.6)
            .unwrap()
            .values[0];
        assert_eq!(value.to_bits(), 1.2941177_f32.to_bits());
    }

    #[test]
    fn unordered_apertures_fail_before_table_lookup() {
        let mut m = metadata();
        m.matrices.insert(
            "SpatialGain_Fstop".into(),
            Matrix {
                encoding: crate::source::MatrixEncoding::Float32,
                representation: "f64",
                dimensions: vec![3],
                values: vec![2.8, 4.0, 4.0],
            },
        );
        assert!(prepare(&m, 4.0)
            .err()
            .unwrap()
            .to_string()
            .contains("interpolation inputs"));
    }
}
