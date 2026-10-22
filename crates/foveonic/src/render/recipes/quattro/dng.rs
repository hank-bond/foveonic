//! Prepares camera-like Quattro DNG coordinates and a deferred sampled appearance.

use crate::{
    dng::appearance::{srgb_to_pcs, srgb_to_romm, Matrix, ProfileBasis, SampledAppearance},
    render::Settings,
    source::{Metadata, Result},
};

// Native photographic response reaches its nominal white at this code scale.
// The technical RGB base can retain higher integral values before the response.
const RESPONSE_WHITE_CODE: f64 = 16384.;

pub(crate) fn prepare_appearance(
    metadata: &Metadata,
    settings: &Settings,
) -> Result<SampledAppearance> {
    super::validate_photographic(settings)?;
    let identity = Matrix::diagonal([1.; 3]);
    if settings.is_monochrome() {
        return Ok(SampledAppearance {
            basis: ProfileBasis {
                mapping: identity.gain(1. / RESPONSE_WHITE_CODE).0,
                neutral: [1.; 3],
                camera_to_linear: identity.0,
                selected_from_base: identity.0,
                linear_to_pcs: srgb_to_pcs()?.0,
            },
            response: super::appearance::prepare_monochrome(
                f64::from(settings.exposure),
                settings.tone,
            )?,
            code_white: RESPONSE_WHITE_CODE,
            after_response: srgb_to_romm()?.0,
            notes: Some("Monochrome channel mix and spatial corrections are baked. Technical samples retain preceding 0..65535 code clamps; photographic tone is deferred."),
        });
    }
    let recorded_balance;
    let balance = if settings.white_balance.is_empty() {
        recorded_balance = super::default_white_balance(metadata)?;
        &recorded_balance
    } else {
        &settings.white_balance
    };
    let recorded_mode;
    let mode = if settings.color_mode.is_empty() {
        recorded_mode = super::default_color_mode(metadata)?;
        &recorded_mode
    } else {
        &settings.color_mode
    };
    let gains = super::calibration::white_balance(metadata, balance)?;
    let base = Matrix(super::calibration::color_matrix(
        metadata, balance, "Standard",
    )?);
    let selected = Matrix(super::calibration::color_matrix(metadata, balance, mode)?);
    // This reverses the linear WB/color part of the canonical technical recipe.
    // Nonlinear color corrections and spatial reconstruction remain baked. The
    // resulting coordinates are an approximate camera basis, not sensor spectra.
    let unbalance = base
        .inverse()?
        .then(Matrix::diagonal(gains.map(|v| 1. / v)));
    let white = unbalance.apply([1.; 3]);
    let maximum = white.into_iter().fold(0., f64::max);
    if white.iter().any(|v| !v.is_finite() || *v <= 0.) || maximum <= 0. {
        return Err("invalid Quattro DNG camera white".into());
    }
    let saturation = f64::from(settings.saturation);
    let saturation = Matrix(std::array::from_fn(|i| {
        [0.299, 0.587, 0.114][i % 3] * (1. - saturation)
            + if i / 3 == i % 3 { saturation } else { 0. }
    }));
    Ok(SampledAppearance {
        basis: ProfileBasis {
            mapping: unbalance.gain(1. / (RESPONSE_WHITE_CODE * maximum)).0,
            neutral: white.map(|v| v / maximum),
            camera_to_linear: unbalance.inverse()?.gain(maximum).0,
            selected_from_base: base.inverse()?.then(selected).0,
            linear_to_pcs: srgb_to_pcs()?.0,
        },
        response: super::appearance::prepare(
            metadata,
            mode,
            f64::from(settings.exposure),
            settings.tone,
        )?,
        code_white: RESPONSE_WHITE_CODE,
        // Quattro's sampled curve returns linear sRGB. Saturation follows that
        // curve, then D50 adaptation/ROMM conversion puts the fitted look in PCS.
        after_response: saturation.then(srgb_to_romm()?).0,
        notes: Some("Canonical Standard nonlinear color and spatial corrections are baked; the selected look is approximate. Technical samples retain preceding 0..65535 code clamps; photographic tone is deferred."),
    })
}

#[cfg(test)]
#[path = "dng_test.rs"]
mod tests;
