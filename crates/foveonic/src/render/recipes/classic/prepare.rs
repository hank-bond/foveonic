//! Prepares Classic camera calibration, reconstruction defaults, and photographic response at each recipe boundary.
//!
//! Equations follow the independently recovered reference implementation. GPU operations receive functional parameters.

use crate::{
    color_space::TransferFunction,
    nodes::{
        color::{
            chroma::{self, Support},
            log_reconstruction,
        },
        finish::{root_detail, signed_response},
        sensor::signed_repair,
    },
    render::Settings,
    source::{Metadata, Result},
};

pub(crate) use super::highlight::prepare as prepare_highlight;

use crate::nodes::sensor::fixed_point::{Correction, CorrectionParams, Transform, TransformParams};

pub struct Prepared {
    pub correction: Correction,
    pub final_gain: [f64; 3],
    pub color: ColorInput,
}

/// White-balance-derived inputs retained until the recipe reaches camera color.
pub struct ColorInput {
    camera: [f64; 9],
    final_gain: [f64; 3],
    ratio: f32,
    digital_gain: f32,
    extent: [usize; 2],
}

pub struct Color {
    pub transform: Transform,
    pub matrix_green_sum: f32,
    pub digital_gain: f32,
}

/// Reject unsupported request branches before preparing calibration.
pub(crate) fn validate_request(metadata: &Metadata, settings: &Settings) -> Result<()> {
    if settings.is_monochrome() {
        return Err("Classic monochrome rendering is not supported by this experiment".into());
    }
    if settings
        .repair
        .is_some_and(|value| value != crate::render::RepairThreshold::Camera)
    {
        return Err("Classic repair uses its exposure-derived cutoff, manual Merrill badness is unsupported".into());
    }
    if settings.highlight_restoration
        && (settings.highlight_mode != crate::render::HighlightMode::Classic
            || settings.highlight_threshold_ev.is_some())
    {
        return Err("Classic rendering currently supports its camera highlight threshold and streaming neutralization".into());
    }
    if !settings.row_correction {
        return Err(
            "Classic calibration currently requires its camera row-correction contract".into(),
        );
    }
    if settings.color_field != crate::render::ColorFieldControls::default() {
        return Err("Classic rendering does not support color-field controls".into());
    }
    if settings.color_dq_gain.is_some() {
        return Err(
            "Classic nonlinear camera color does not yet support a ColorDQ gain override".into(),
        );
    }
    if settings
        .reconstruction
        .is_some_and(|controls| controls != crate::noise::Controls::default())
    {
        return Err(
            "Classic reconstruction currently supports the reference control defaults".into(),
        );
    }
    let iso = capture_iso(metadata)?;
    if iso >= 400. {
        return Err(
            "Classic high-ISO range reconstruction is not supported by this experiment".into(),
        );
    }
    if scalar(metadata, "SkipStepFlags", 0.)? as u32 & !(1 << 11) != 0 {
        return Err("Classic rendering encountered an unsupported processing-step flag".into());
    }
    if scalar(metadata, "RedSharpen", 0.)? != 0. {
        return Err("Classic red sharpening is outside the selected camera route".into());
    }
    if settings.reconstruction.is_some()
        && (scalar(metadata, "ChromaBlur", 0.)? > 100.
            || scalar(metadata, "FIR_Iterations", 1.)? != 1.)
    {
        return Err(
            "Classic reconstruction requires the selected small/box chroma and single FIR pass"
                .into(),
        );
    }
    Ok(())
}

fn capture_iso(metadata: &Metadata) -> Result<f64> {
    let request = metadata
        .properties
        .get("ISO")
        .ok_or("missing ISO")?
        .parse::<f64>()?;
    let capture = scalar(metadata, "CaptureISO", request)?;
    Ok(if capture == 0. { request } else { capture })
}

pub(crate) fn prepare_despeckle(metadata: &Metadata) -> Result<signed_repair::Despeckle> {
    let old_despeckle = scalar(metadata, "CaptureShutter", 0.)?
        > scalar(metadata, "DespeckleExpTimeThresh", 150000.)?;
    Ok(signed_repair::Despeckle::new(
        [metadata.width as u32, metadata.height as u32],
        old_despeckle,
    ))
}

pub(crate) fn prepare_chroma(
    metadata: &Metadata,
    gains: [f64; 3],
) -> Result<Vec<chroma::Prepared>> {
    let extent = [metadata.width as u32, metadata.height as u32];
    let iso = capture_iso(metadata)?;
    let mut stages = Vec::new();
    let blur = scalar(metadata, "ChromaBlur", 0.)?;
    if blur != 0. {
        let dq = if metadata.matrix("ChromaDQ").is_some() {
            metadata.fixed::<3>("ChromaDQ")?
        } else {
            [0.; 3]
        };
        let mean = gains.iter().zip(dq).map(|(g, d)| 1.3 * g * d).sum::<f64>() / 3.;
        let sensor = scalar(metadata, "SensorISO", 100.)?;
        let sensor = if sensor == 0. { 100. } else { sensor };
        let pre = (iso as f32 / sensor as f32).min(2.);
        let regularizer =
            (1.3 * f64::from(pre * 10.)) as u32 * scalar(metadata, "SpatialDQ", 0.)? as u32;
        let shift = scalar(metadata, "ChromaSumShift", 3.)? as u32;
        for (amplitude, support) in [(mean * 2., Support::Binomial3), (mean, Support::Box5)] {
            let p = chroma::Prepared::new(
                extent,
                [amplitude; 3],
                [0.8; 3],
                regularizer,
                shift,
                support,
            )?;
            stages.push(p);
        }
    }
    Ok(stages)
}

pub(crate) fn prepare_reconstruction(
    metadata: &Metadata,
    matrix_green_sum: f32,
    digital_gain: f32,
) -> Result<Option<log_reconstruction::Prepared>> {
    let extent = [metadata.width as u32, metadata.height as u32];
    if scalar(metadata, "FIR_DisableStep", 0.)? == 0. {
        let names = [
            "FIR_ProportionLocalMeanRef",
            "FIR_ProportionLocalMeanEdgeRef",
            "FIR_ProportionLocalMeanImgWithEdgeRef",
            "FIR_Locality",
            "FIR_SigmoidOffset",
            "FIR_ScaleFactorEdgeRef",
            "FIR_ScaleFactorLocalMeanEdgeRef",
            "FIR_EdgeMapExponent",
        ];
        let defaults = [0.0011, 0.0019, 0.019, 1., 0.55, 5., 7., 0.7];
        let mut controls = [0.; 10];
        for i in 0..8 {
            let value = scalar(metadata, names[i], defaults[i])?;
            controls[i] = if value == 0. { defaults[i] } else { value } as f32;
        }
        controls[8] = matrix_green_sum;
        controls[9] = digital_gain;
        let fir = log_reconstruction::Prepared::new(extent, controls)?;
        return Ok(Some(fir));
    }
    Ok(None)
}

pub(crate) fn prepare_root(
    metadata: &Metadata,
    crop: [u32; 4],
    rotation: u32,
) -> Result<root_detail::Prepared> {
    let extent = [metadata.width as u32, metadata.height as u32];
    let mut green = scalar(metadata, "GreenWeight", 0.)?;
    let mut high = scalar(metadata, "GreenHighLimit", 0.)? as i32;
    let low = scalar(metadata, "GreenLowLimit", 0.)? as i32;
    if green == 0. && high == 0 && low == 0 {
        green = 0.001;
        high = 200;
    }
    let root = root_detail::Prepared::new(extent, crop, green, high, 1.)?;
    let root = if matches!(rotation % 360, 90 | 270) {
        root.with_horizontal_first()
    } else {
        root
    };
    Ok(root)
}

pub(crate) fn prepare_photographic(
    metadata: &Metadata,
    settings: &Settings,
) -> Result<signed_response::Prepared> {
    let iso = capture_iso(metadata)?;
    let adjustments = if metadata
        .properties
        .get("CAMMODEL")
        .is_some_and(|m| m == "SIGMA SD15")
    {
        signed_response::ToneAdjustments {
            exposure: 0.6,
            shadows: -0.2,
            contrast: 0.4,
            highlights: 0.2,
        }
    } else {
        signed_response::ToneAdjustments {
            highlights: if iso <= 50. { 0.8 } else { 0. },
            ..Default::default()
        }
    };
    let finish = signed_response::prepare(
        signed_response::Controls {
            exposure: settings.exposure as f64,
            shadows: settings.tone.shadows,
            contrast: settings.tone.contrast,
            highlights: settings.tone.highlights,
            saturation: (settings.saturation as f64 - 1.) * 2.,
            detail: settings
                .sharpen_amount
                .map_or(0., |amount| f64::from(amount) - 1.),
        },
        [1.; 3],
        adjustments,
        // The reference profile uses a pure power response, without the
        // standard ROMM shadow toe. Primaries remain ROMM after decoding.
        TransferFunction::Power(461. / 256.),
    )?;
    Ok(finish)
}

fn vector<const N: usize>(metadata: &Metadata, name: &str, default: [f64; N]) -> Result<[f64; N]> {
    if metadata.matrix(name).is_some() {
        metadata.fixed(name)
    } else {
        Ok(default)
    }
}
fn scalar(metadata: &Metadata, name: &str, default: f64) -> Result<f64> {
    if metadata.matrix(name).is_some() {
        metadata.scalar(name)
    } else {
        Ok(default)
    }
}
fn named<'a>(metadata: &'a Metadata, catalogue: &str, name: &str) -> Result<&'a str> {
    metadata
        .lists
        .get(catalogue)
        .and_then(|list| {
            list.names()
                .find(|key| key.trim().eq_ignore_ascii_case(name.trim()))
                .and_then(|key| list.get(key))
        })
        .map(String::as_str)
        .ok_or_else(|| format!("missing {catalogue} preset {name}").into())
}
fn multiply(a: [f64; 9], b: [f64; 9]) -> [f64; 9] {
    std::array::from_fn(|i| (0..3).map(|k| a[i / 3 * 3 + k] * b[k * 3 + i % 3]).sum())
}
fn inverse(m: [f64; 9]) -> Result<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = m;
    let co = [
        e * i - f * h,
        c * h - b * i,
        b * f - c * e,
        f * g - d * i,
        a * i - c * g,
        c * d - a * f,
        d * h - e * g,
        b * g - a * h,
        a * e - b * d,
    ];
    let det = a * co[0] + b * co[3] + c * co[6];
    if !det.is_finite() || det.abs() < 1e-20 {
        return Err("singular camera matrix".into());
    }
    Ok(co.map(|v| v / det))
}
fn response(matrix: [f64; 9]) -> Result<[f64; 3]> {
    let inv = inverse(matrix)?;
    Ok(std::array::from_fn(|r| {
        inv[r * 3] * 0.3127 + inv[r * 3 + 1] * 0.329 + inv[r * 3 + 2] * 0.3583
    }))
}

pub fn prepare(metadata: &Metadata, white_balance: &str) -> Result<Prepared> {
    if scalar(metadata, "CCMatrix_Flag", 0.)? != 1. {
        return Err("this experiment requires cached camera color calibration".into());
    }
    if scalar(metadata, "BulbFrameCount", 0.)? > 1.
        || scalar(metadata, "UsePreComputedDrift", 0.)? != 1.
    {
        return Err("this experiment requires single-frame precomputed drift".into());
    }
    for name in ["DarkDrift", "RawDrift", "DarkGainScale", "PostPolyMatrix"] {
        if metadata.matrix(name).is_some() && metadata.vector(name)?.iter().any(|v| *v != 0.) {
            return Err(format!("unsupported nonzero {name} in cached-matrix experiment").into());
        }
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
    let basis = [
        3.241, -1.5374, -0.4986, -0.9692, 1.876, 0.0416, 0.0556, -0.204, 1.057,
    ];
    let mut camera = multiply(
        inverse(basis)?,
        metadata.fixed(named(metadata, corrections, wb)?)?,
    );
    let stored_gain: [f64; 3] = metadata.fixed(named(metadata, gains, wb)?)?;
    for (i, v) in camera.iter_mut().enumerate() {
        *v *= stored_gain[i % 3];
    }
    let white = response(camera)?;
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
    let final_gain: [f64; 3] =
        std::array::from_fn(|c| maximum / white[c] * aperture[c] * temperature[c] * pre as f64);
    for (i, v) in camera.iter_mut().enumerate() {
        *v *= white[i % 3] / maximum;
    }
    let grid_name = metadata
        .lists
        .get("SpatialGainTables")
        .and_then(|l| l.get(wb))
        .filter(|n| metadata.matrix(n).is_some())
        .map_or("SpatialGain", String::as_str);
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
    // The selected profiles have integral horizontal knot spacing. Their
    // repeated Float64 caller boundaries are then exactly representable in WGSL.
    if !metadata.width.is_multiple_of(columns - 1) {
        return Err("nonintegral correction-knot spacing is outside this experiment".into());
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
    let filter = scalar(metadata, "ColumnFilter", 0.)?;
    let linear = vector(metadata, "ColumnFilterRGB", [filter; 3])?;
    let quadratic = vector(metadata, "ColumnFilterSquareRGB", linear)?;
    let shield = vector(metadata, "DarkShieldColRange", [0.; 4])?;
    if shield[1] - shield[0] >= 2. || shield[3] - shield[2] >= 2. {
        return Err("row shield estimation is outside this cached-matrix experiment".into());
    }
    let reduction = scalar(metadata, "LinLUT_ReductionFactor", 1.)?;
    let reduction = if reduction == 0. { 1. } else { reduction };
    // Correction coefficients use fourteen fractional bits. Zero disables LUT reduction.
    const CORRECTION_Q14_SCALE: f64 = 16384.;
    let q14 = if reduction == 1. {
        0
    } else {
        (1.3 * reduction * CORRECTION_Q14_SCALE) as u32
    };
    let flare = vector(metadata, "FlareFactor", [0.; 3])?;
    let correction = Correction {
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
        },
        knots,
    };
    Ok(Prepared {
        correction,
        final_gain,
        color: ColorInput {
            camera,
            final_gain,
            ratio,
            digital_gain,
            extent: [metadata.width, metadata.height],
        },
    })
}

/// Resolve selected appearance and nonlinear noise only for color-dependent outputs.
pub fn prepare_color(metadata: &Metadata, input: &ColorInput, color_mode: &str) -> Result<Color> {
    let camera = selected_camera(metadata, input.camera, color_mode)?;
    prepare_color_matrix(metadata, input, camera)
}

fn selected_camera(
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
            camera = multiply(mode, camera);
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
        dq = [4. / (input.ratio.min(8.) as f64).sqrt().sqrt(); 3];
    }
    let channel_noise = std::array::from_fn(|c| 1.3 * input.final_gain[c] * dq[c]);
    let (transform, matrix_green_sum) = transform(
        input.extent[0],
        input.extent[1],
        camera,
        channel_noise,
        input.digital_gain,
    )?;
    Ok(Color {
        transform,
        matrix_green_sum,
        digital_gain: input.digital_gain,
    })
}

#[allow(clippy::approx_constant)] // The recovered cosine cutoff is the literal 3.1416.
fn transform(
    width: usize,
    height: usize,
    camera: [f64; 9],
    noise: [f64; 3],
    digital: f32,
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
    // Fixed color basis from the reference implementation. Its exact standard
    // color-space identity has not been established.
    const REFERENCE_COLOR_BASIS: [f64; 9] = [
        1.4032, -0.2231, -0.1016, -0.5263, 1.4816, 0.017, -0.0112, 0.0183, 0.9113,
    ];
    let mut matrix = multiply(REFERENCE_COLOR_BASIS, camera);
    let mut w: [f64; 3] =
        std::array::from_fn(|r| matrix[r * 3] + matrix[r * 3 + 1] + matrix[r * 3 + 2]);
    // Reference neutral-row weighting and floor preserve the matrix's balance
    // before its green-row normalization. Keep the recovered summation order.
    const NEUTRAL_ROW_WEIGHTS: [f64; 3] = [0.3, 0.55, 0.15];
    const ROW_FLOOR_FRACTION: f64 = 0.25;
    let weighted = |v: [f64; 3]| {
        v[1] * NEUTRAL_ROW_WEIGHTS[1]
            + v[0] * NEUTRAL_ROW_WEIGHTS[0]
            + v[2] * NEUTRAL_ROW_WEIGHTS[2]
    };
    let floor = weighted(w) * ROW_FLOOR_FRACTION;
    w = w.map(|v| v.max(floor));
    let mean = weighted(w);
    for (i, v) in matrix.iter_mut().enumerate() {
        *v *= mean / w[i / 3];
    }
    let matrix_green_sum = (matrix[3] + matrix[4] + matrix[5]) as f32;
    let normalizer = 1.0f32 / matrix_green_sum;
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

#[cfg(test)]
#[path = "prepare_test.rs"]
mod tests;

/// Build an editable camera basis and approximate selected-mode mapping.
pub(crate) fn prepare_dng_appearance(
    metadata: &Metadata,
    settings: &Settings,
) -> Result<crate::dng::appearance::ClassicAppearance> {
    use crate::dng::appearance::{ClassicAppearance, Matrix};
    validate_request(metadata, settings)?;
    let prepared = prepare(metadata, &settings.white_balance)?;
    let base = prepare_unselected_color(metadata, &prepared.color)?;
    let selected = prepare_color(metadata, &prepared.color, &settings.color_mode)?;
    let matrix = |color: &Color| {
        let p = &color.transform.params;
        Matrix(std::array::from_fn(|i| {
            f64::from(p.matrix[i / 3][i % 3]) / f64::from(1u32 << p.source[3])
        }))
    };
    let base = matrix(&base);
    // Undo the calibrated layer gains in the stored coordinates. The inverse
    // effective matrix preserves their camera-like WB controls after color.
    // Nonlinear layer corrections remain baked, as in the technical recipe.
    let unbalance = base
        .inverse()?
        .then(Matrix::diagonal(prepared.final_gain.map(|g| 1. / g)));
    let white = unbalance.apply([1.; 3]);
    let maximum = white.into_iter().fold(0., f64::max);
    if white.iter().any(|v| !v.is_finite() || *v <= 0.) || maximum <= 0. {
        return Err("invalid Classic DNG camera white".into());
    }
    Ok(ClassicAppearance {
        mapping: unbalance.gain(1. / (4096. * maximum)).0,
        neutral: white.map(|v| v / maximum),
        camera_to_linear: unbalance.inverse()?.gain(maximum).0,
        // This fits the matrix effect of mode selection. Changes to the
        // nonlinear corrections and reconstruction cannot be deferred exactly.
        selected_from_base: base.inverse()?.then(matrix(&selected)).0,
        response: prepare_photographic(metadata, settings)?,
    })
}
