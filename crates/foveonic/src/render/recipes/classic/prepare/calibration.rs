//! Prepares Classic sensor correction, geometry, and white-balance-derived camera inputs.

use super::super::reference::{
    D65_WHITE_XYZ, MICROSECONDS_PER_SECOND, Q14_SCALE, RANGE_RECONSTRUCTION_ISO,
};
use super::{
    capture_iso, inverse, multiply, named, response, scalar, vector, ColorInput, Metadata,
    Prepared, Result,
};
use crate::nodes::sensor::{
    fixed_point::{Correction, CorrectionParams},
    signed_repair,
};

pub(crate) fn prepare_red_detail(
    metadata: &Metadata,
) -> Result<Option<crate::nodes::sensor::red_detail::Params>> {
    // Stage 2 doubles the stored strength before the row callback's Q9 scaling.
    let amount = (scalar(metadata, "RedSharpen", 0.)? as u32).wrapping_mul(2);
    if amount == 0 {
        return Ok(None);
    }
    let mut left = scalar(metadata, "RedLeftWeight", 0.)? as i32;
    let right = scalar(metadata, "RedRightWeight", 0.)? as i32;
    if left == 0 && right == 0 {
        left = 2048; // One eighth in signed Q14.
    }
    Ok(Some(crate::nodes::sensor::red_detail::Params {
        shape: [metadata.width as u32, metadata.height as u32, amount, 0],
        weights: [left, right, 0, 0],
    }))
}

pub(crate) fn prepare_despeckle(metadata: &Metadata) -> Result<signed_repair::Despeckle> {
    // The threshold is in microseconds even on bodies storing shutter seconds.
    let model = metadata
        .properties
        .get("CAMMODEL")
        .map_or("", String::as_str);
    let shutter_microseconds = if let Some(exposure) = metadata.properties.get("EXPTIME") {
        // The reference Stage2 caller uses the integer request exposure.
        exposure.parse::<i64>()? as f64
    } else {
        scalar(metadata, "CaptureShutter", 0.)? * MICROSECONDS_PER_SECOND
            / super::super::policy::capture_shutter_units(model.trim())
    };
    let old_despeckle = shutter_microseconds > scalar(metadata, "DespeckleExpTimeThresh", 150000.)?;
    Ok(signed_repair::Despeckle::new(
        [metadata.width as u32, metadata.height as u32],
        old_despeckle,
    ))
}

pub(crate) fn geometry(
    metadata: &Metadata,
) -> Result<crate::nodes::sensor::calibration::EncodeParams> {
    let active = metadata.fixed::<4>("ActiveImageArea")?;
    let keep = vector(metadata, "KeepImageArea", [0.; 4])?;
    let crop = [
        active[0] - keep[0],
        active[1] - keep[1],
        active[2] - keep[0] + 1.,
        active[3] - keep[1] + 1.,
    ];
    if crop
        .iter()
        .any(|v| !v.is_finite() || *v < 0. || v.fract() != 0.)
    {
        return Err("invalid Classic active rectangle".into());
    }
    let crop = crop.map(|v| v as u32);
    if crop[0] >= crop[2]
        || crop[1] >= crop[3]
        || crop[2] > metadata.width as u32
        || crop[3] > metadata.height as u32
    {
        return Err("Classic active rectangle outside kept sensor".into());
    }
    let rotation = metadata.rotation % 360;
    if ![0, 90, 180, 270].contains(&rotation) {
        return Err("unsupported image orientation".into());
    }
    let extent = [crop[2] - crop[0], crop[3] - crop[1]];
    let extent = if matches!(rotation, 90 | 270) {
        [extent[1], extent[0]]
    } else {
        extent
    };
    Ok(crate::nodes::sensor::calibration::EncodeParams {
        source: [metadata.width as u32, metadata.height as u32, rotation, 0],
        crop,
        output: [
            extent[0],
            extent[1],
            crate::color_space::ColorSpace::ProPhotoRgb as u32,
            0,
        ],
    })
}

pub(crate) fn prepare_column_filters(metadata: &Metadata) -> Result<([f64; 3], [f64; 3])> {
    let filter = scalar(metadata, "ColumnFilter", 0.)?;
    let mut linear = vector(metadata, "ColumnFilterRGB", [filter; 3])?;
    if linear[1] == linear[0] {
        linear[1] *= super::super::policy::CameraPolicy::from_metadata(metadata).green_column_scale;
    }
    let quadratic = vector(metadata, "ColumnFilterSquareRGB", linear)?;
    Ok((linear, quadratic))
}

/// Drift-dependent gain adjusts camera color independently of subtracted sensor drift.
fn dark_color_gain(metadata: &Metadata) -> Result<[f64; 3]> {
    let raw = vector(metadata, "RawDrift", [0.; 12])?;
    let precomputed = scalar(metadata, "UsePreComputedDrift", 0.)? != 0.;
    let mean: [f64; 3] = std::array::from_fn(|c| {
        if precomputed {
            (raw[1 + 2 * c] + raw[7 + 2 * c]) * 0.5
        } else {
            raw[7 + 2 * c] + 0.5 * raw[6 + 2 * c]
        }
    });
    let mut factor = vector(metadata, "DarkGainFactor", [0.; 3])?;
    if factor == [0.; 3] {
        factor = [1.25; 3];
    }
    let scale = vector(
        metadata,
        "DarkGainScale",
        [super::super::policy::CameraPolicy::from_metadata(metadata).dark_gain_scale; 3],
    )?;
    let normal = vector(metadata, "DarkNormalOffset", [0.; 3])?;
    // Classic PROP stores the request exposure as integer microseconds.
    let exposure = metadata
        .properties
        .get("EXPTIME")
        .map_or(Ok(0_i64), |v| v.parse::<i64>())? as f64;
    Ok(std::array::from_fn(|c| {
        let others = (mean[(c + 1) % 3] + mean[(c + 2) % 3]) * 0.5;
        (1. + factor[c] * scale[c] * exposure * (mean[c] - others - normal[c]) / 4096.)
            .clamp(0.98, if c == 2 { 1.04 } else { 1.01 })
    }))
}

/// WB9 uses an explicit illuminant when present, otherwise a flash-derived
/// rank-one camera mixture. Its neutral is supplied separately as unit RGB.
pub(crate) fn monochrome_camera(metadata: &Metadata) -> Result<[f64; 9]> {
    if let Ok(illuminant) = named(metadata, "WhiteBalanceIlluminants", "Monochrome") {
        let matrix = metadata.fixed(illuminant)?;
        return match named(metadata, "WhiteBalanceCorrections", "Monochrome") {
            Ok(correction) => Ok(multiply(metadata.fixed(correction)?, matrix)),
            Err(_) => Ok(matrix),
        };
    }
    let flash: [f64; 9] = metadata.fixed("CamToXYZ_Flash")?;
    let white = response(flash)?;
    let maximum = white.into_iter().fold(f64::NEG_INFINITY, f64::max);
    if !maximum.is_finite() || maximum <= 0. {
        return Err("invalid monochrome flash white".into());
    }
    let white = white.map(|v| v / maximum);
    // Preserve the reference rowwise summation order.
    let k = (flash[1] * white[1]
        + white[0] * flash[0]
        + white[2] * flash[2]
        + flash[4] * white[1]
        + white[0] * flash[3]
        + white[2] * flash[5]
        + white[1] * flash[7]
        + white[0] * flash[6]
        + white[2] * flash[8])
        / 9.;
    Ok([k, k + k, k, k, k + k, k, k, k + k, k])
}

pub fn prepare(metadata: &Metadata, white_balance: &str) -> Result<Prepared> {
    prepare_with_white_balance(metadata, white_balance, Default::default())
}

pub(crate) fn prepare_with_white_balance(
    metadata: &Metadata,
    white_balance: &str,
    statistics: crate::render::WhiteBalanceStats,
) -> Result<Prepared> {
    let monochrome = white_balance.trim().eq_ignore_ascii_case("Monochrome");
    // WB9 falls back to the flash mixture rather than the cached color catalogue.
    let cached = !monochrome && scalar(metadata, "CCMatrix_Flag", 0.)? == 1.;
    if scalar(metadata, "BulbFrameCount", 0.)? > 1. {
        return Err("Classic accumulated bulb calibration is not supported".into());
    }
    let corrections = if metadata
        .lists
        .contains_key("DP1_WhiteBalanceColorCorrections")
    {
        "DP1_WhiteBalanceColorCorrections"
    } else {
        "WhiteBalanceColorCorrections"
    };
    let gains = if metadata.lists.contains_key("DP1_WhiteBalanceGains") {
        "DP1_WhiteBalanceGains"
    } else {
        "WhiteBalanceGains"
    };
    let wb = if white_balance.eq_ignore_ascii_case("Daylight") {
        "Sunlight"
    } else {
        white_balance
    };
    // Recovered cached calibration basis. Keep its rounded coefficients.
    const CACHED_CALIBRATION_BASIS: [f64; 9] = [
        3.241, -1.5374, -0.4986, -0.9692, 1.876, 0.0416, 0.0556, -0.204, 1.057,
    ];
    let mut camera = if monochrome {
        monochrome_camera(metadata)?
    } else if cached {
        let mut matrix = multiply(
            inverse(CACHED_CALIBRATION_BASIS)?,
            metadata.fixed(named(metadata, corrections, wb)?)?,
        );
        let stored_gain: [f64; 3] = metadata.fixed(named(metadata, gains, wb)?)?;
        for (i, value) in matrix.iter_mut().enumerate() {
            *value *= stored_gain[i % 3];
        }
        matrix
    } else {
        let mut matrix = multiply(
            metadata.fixed(named(metadata, "WhiteBalanceCorrections", wb)?)?,
            metadata.fixed(named(metadata, "WhiteBalanceIlluminants", wb)?)?,
        );
        let blue_mix = super::super::policy::CameraPolicy::from_metadata(metadata).blue_axis_mix;
        if blue_mix != 0. {
            let white = response(matrix)?;
            let mixed = multiply(
                [
                    1.,
                    0.,
                    0.,
                    0.,
                    1.,
                    0.,
                    blue_mix,
                    blue_mix,
                    1. - 2. * blue_mix,
                ],
                matrix,
            );
            let xyz_white = D65_WHITE_XYZ;
            for row in 0..3 {
                let actual: f64 = (0..3).map(|c| mixed[row * 3 + c] * white[c]).sum();
                let factor = xyz_white[row] / actual;
                for col in 0..3 {
                    matrix[row * 3 + col] = mixed[row * 3 + col] * factor;
                }
            }
        }
        matrix
    };
    let dark = dark_color_gain(metadata)?;
    for (i, value) in camera.iter_mut().enumerate() {
        *value *= dark[i % 3];
    }
    let white = if monochrome {
        [1.; 3]
    } else {
        statistics.neutral.map_or_else(|| response(camera), Ok)?
    };
    let maximum = white.into_iter().fold(f64::NEG_INFINITY, f64::max);
    if white.iter().any(|v| *v <= 0.) {
        return Err("nonpositive camera white response".into());
    }
    let iso = capture_iso(metadata)?;
    let sensor = scalar(metadata, "SensorISO", 100.)?;
    let ratio = iso as f32 / (if sensor == 0. { 100. } else { sensor }) as f32;
    if !ratio.is_finite() || ratio <= 0. {
        return Err("invalid capture ISO ratio".into());
    }
    let pre = ratio.min(2.);
    let digital_gain = ratio / pre;
    let aperture = vector(metadata, "FnumberGainFact", [1.; 3])?;
    let temperature = vector(metadata, "TempGainFact", [1.; 3])?;
    let mut final_gain: [f64; 3] =
        std::array::from_fn(|c| maximum / white[c] * aperture[c] * temperature[c] * pre as f64);
    // High-ISO Incandescent mixes selected WB gains with gray-world gains.
    // Auto retains its selected gain but shares the later matrix-mixing path.
    if let Some(mean) = statistics.mean.filter(|_| !monochrome) {
        if mean.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err("nonpositive camera channel mean".into());
        }
        let mixing = if wb.eq_ignore_ascii_case("Incandescent") {
            f64::from(scalar(metadata, "WBMixing", 0.5)? as f32)
        } else {
            1.
        };
        for c in 0..3 {
            final_gain[c] =
                (1. - mixing) * mean[1] / mean[c] * f64::from(pre) + mixing * final_gain[c];
        }
    }
    for (i, v) in camera.iter_mut().enumerate() {
        *v *= white[i % 3] / maximum;
    }
    // Mix camera matrices before converting to the working RGB basis. The
    // cached catalogue supplies an unbalanced daylight alternative; older
    // catalogues build and white-normalize their illuminant/correction pair.
    let mix_matrix = iso >= RANGE_RECONSTRUCTION_ISO
        && (wb.eq_ignore_ascii_case("Auto") || wb.eq_ignore_ascii_case("Incandescent"));
    let alternate_matrix = if mix_matrix {
        let alternate = if cached {
            let daylight = named(metadata, corrections, "Daylight")
                .or_else(|_| named(metadata, corrections, "Sunlight"))?;
            multiply(
                inverse(CACHED_CALIBRATION_BASIS)?,
                metadata.fixed(daylight)?,
            )
        } else {
            let daylight = named(metadata, "WhiteBalanceCorrections", "Daylight")
                .or_else(|_| named(metadata, "WhiteBalanceCorrections", "Sunlight"))?;
            let mut alternate = multiply(
                metadata.fixed(daylight)?,
                metadata.fixed(named(metadata, "WhiteBalanceIlluminants", wb)?)?,
            );
            let alternate_white = response(alternate)?;
            let maximum = alternate_white
                .into_iter()
                .fold(f64::NEG_INFINITY, f64::max);
            for (i, value) in alternate.iter_mut().enumerate() {
                *value *= alternate_white[i % 3] / maximum;
            }
            alternate
        };
        Some((
            alternate,
            f64::from(scalar(metadata, "MatrixMixing", 0.5)? as f32),
        ))
    } else {
        None
    };
    let nearest_grid = super::super::spatial_gain::nearest(metadata, wb, f64::from(ratio))?;
    let grid_name = nearest_grid
        .as_deref()
        .or_else(|| {
            metadata
                .lists
                .get("SpatialGainTables")
                .and_then(|l| l.get(wb))
                .filter(|n| metadata.matrix(n).is_some())
                .map(String::as_str)
        })
        .unwrap_or("SpatialGain");
    let grid = metadata
        .matrix(grid_name)
        .ok_or("missing spatial gain grid")?;
    let [rows, columns, channels]: [usize; 3] = grid
        .dimensions
        .as_slice()
        .try_into()
        .map_err(|_| "invalid spatial gain rank")?;
    if rows < 2 || columns < 2 || channels != 3 || grid.values.len() != rows * columns * 3 {
        return Err("invalid spatial gain geometry".into());
    }
    let mut knots = Vec::with_capacity(metadata.height * columns * 3);
    for y in 0..metadata.height {
        let q = y as f64 / (metadata.height - 1) as f64 * (rows - 1) as f64;
        let lo = q.floor() as usize;
        let hi = (lo + 1).min(rows - 1);
        let t = (q - lo as f64) as f32;
        for i in 0..columns * 3 {
            let a = grid.values[lo * columns * 3 + i] as f32;
            let b = grid.values[hi * columns * 3 + i] as f32;
            knots.push(a + (b - a) * t);
        }
    }
    let (linear, quadratic) = prepare_column_filters(metadata)?;
    let reduction = scalar(metadata, "LinLUT_ReductionFactor", 1.)?;
    let reduction = if reduction == 0. { 1. } else { reduction };
    // Correction coefficients use fourteen fractional bits. Zero disables LUT reduction.
    const CORRECTION_Q14_SCALE: f64 = Q14_SCALE;
    let q14 = if reduction == 1. {
        0
    } else {
        ((if cached { 1.3 } else { 1. }) * reduction * CORRECTION_Q14_SCALE) as u32
    };
    let flare = vector(metadata, "FlareFactor", [0.; 3])?;
    let mut correction = Correction {
        params: CorrectionParams {
            source: [
                metadata.width as u32,
                metadata.height as u32,
                columns as u32,
                q14,
            ],
            flare: [flare[0] as f32, flare[1] as f32, flare[2] as f32, 0.],
            final_gain: [
                final_gain[0] as f32,
                final_gain[1] as f32,
                final_gain[2] as f32,
                0.,
            ],
            linear: [
                (linear[0] * CORRECTION_Q14_SCALE) as i32,
                (linear[1] * CORRECTION_Q14_SCALE) as i32,
                (linear[2] * CORRECTION_Q14_SCALE) as i32,
                0,
            ],
            quadratic: [
                (quadratic[0] * CORRECTION_Q14_SCALE) as i32,
                (quadratic[1] * CORRECTION_Q14_SCALE) as i32,
                (quadratic[2] * CORRECTION_Q14_SCALE) as i32,
                0,
            ],
            segment_scale: [(scalar(metadata, "SegmentCapacitance", 0.)? * CORRECTION_Q14_SCALE)
                as i32; 4],
            segment_bias: [(scalar(metadata, "SegmentChargeFeedThrough", 0.)? * CORRECTION_Q14_SCALE)
                as i32; 4],
            controls: [
                scalar(metadata, "SegmentBoundaryInterval", 0.)? as u32,
                0,
                0,
                0,
            ],
            ..Default::default()
        },
        knots,
    };
    super::super::correction::prepare_metadata(metadata, &mut correction)?;
    Ok(Prepared {
        correction,
        final_gain,
        color: ColorInput {
            monochrome,
            camera,
            final_gain,
            relative_gain: std::array::from_fn(|c| maximum / white[c]),
            ratio,
            digital_gain,
            extent: [metadata.width, metadata.height],
            statistics_factor: if cached { 1.3 } else { 1. },
            alternate_matrix,
        },
    })
}
