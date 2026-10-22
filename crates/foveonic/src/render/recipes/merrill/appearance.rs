//! Resolves Merrill camera coordinates and selected photographic appearance for DNG.

use crate::{
    color_space::ColorSpace,
    dng::appearance::{srgb_to_pcs, Matrix, MatrixAppearance, Prepared},
    nodes::{color::color_mode, finish::tone, sensor::calibration},
    render::Settings,
    source::{Metadata, Result},
};

// Technical linear RGB uses the recovered 12-bit nominal-white code.
const LINEAR_CODE_WHITE: f64 = 4095.;

pub(in crate::render::recipes) fn prepare(m: &Metadata, settings: &Settings) -> Result<Prepared> {
    if settings.is_monochrome() {
        return Prepared::monochrome(
            tone::prepare_monochrome(m, settings.tone)?,
            f64::from(settings.exposure),
            LINEAR_CODE_WHITE,
        );
    }
    let key = m
        .lists
        .get("WhiteBalanceGains")
        .and_then(|x| x.get(&settings.white_balance))
        .ok_or("missing DNG WB gains")?;
    let gains = m.fixed::<3>(key)?;
    if gains.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("invalid DNG WB gains".into());
    }
    let n = gains.map(|v| 1. / v);
    let maximum = n.into_iter().fold(0., f64::max);
    // Readers see rounded rationals, not our full-precision WB values.
    // Use those same values here so the fitted look matches their input.
    let neutral = n.map(|v| (v / maximum * 1_000_000.).round() / 1_000_000.);
    if neutral.iter().any(|v| *v <= 0.) {
        return Err("unrepresentable DNG neutral".into());
    }
    let unselected = color_mode::Settings {
        tone_index: 0,
        contrast: 0.,
        compensation: None,
    };
    let rows = calibration::color(m, &settings.white_balance, &unselected)?.matrix;
    let base = Matrix(std::array::from_fn(|i| f64::from(rows[i / 3][i % 3])));
    let mode = color_mode::prepare(m, &settings.color_mode)?;
    let selected = Matrix(mode.camera_matrix(base.0));
    let mapping = base
        .inverse()?
        .then(Matrix::diagonal(gains.map(|v| 1. / v / LINEAR_CODE_WHITE)));
    let space = ColorSpace::from_metadata(m)?;
    let pcs = match space {
        ColorSpace::ProPhotoRgb => return Err("ROMM is not a camera metadata color space".into()),
        ColorSpace::Srgb => srgb_to_pcs()?,
        ColorSpace::AdobeRgb => Matrix([
            0.6097559, 0.2052401, 0.1492240, 0.3111242, 0.6256560, 0.0632197, 0.0194811, 0.0608902,
            0.7448387,
        ])
        .white_normalized()?,
    };
    let camera_to_linear = Matrix::diagonal(gains).then(selected);
    let sat = f64::from(settings.saturation);
    let saturation = Matrix(std::array::from_fn(|i| {
        [0.299, 0.587, 0.114][i % 3] * (1. - sat) + if i / 3 == i % 3 { sat } else { 0. }
    }));
    let (_, iso) = crate::noise::processing_gain(m)?;
    Prepared::matrix(MatrixAppearance {
        mapping,
        neutral,
        camera_to_linear,
        linear_to_pcs: pcs,
        after_response: saturation,
        tone: tone::prepare(m, &mode, settings.tone)?,
        calibration_exposure: (f64::from(iso) / maximum).log2(),
        exposure: f64::from(settings.exposure),
    })
}
