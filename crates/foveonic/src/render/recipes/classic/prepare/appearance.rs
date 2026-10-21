//! Prepares Classic root detail, photographic response, and editable DNG appearance.

use super::super::reference::{PROFILE_POWER, RANGE_RECONSTRUCTION_ISO};
use super::{
    capture_iso, named, prepare_color, prepare_unselected_color, prepare_with_white_balance,
    scalar, validate_request, Color, Metadata, Result, Settings,
};
use crate::{
    color_space::TransferFunction,
    nodes::finish::{root_detail, signed_response},
};

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
    let mut adjustments =
        super::super::policy::CameraPolicy::from_metadata(metadata).tone_adjustments(iso);
    // The reference adds the selected mode's contrast compensation to the
    // user contrast before constructing its shared photographic tone curve.
    if let Ok(name) = named(
        metadata,
        "ColorModeContrastCompensations",
        &settings.color_mode,
    ) {
        adjustments.contrast += f64::from(metadata.scalar(name)? as f32);
    }
    let finish = signed_response::prepare(
        signed_response::Controls {
            exposure: settings.exposure as f64,
            shadows: settings.tone.shadows,
            contrast: settings.tone.contrast,
            highlights: settings.tone.highlights,
            saturation: if settings.is_monochrome() {
                0.
            } else {
                (settings.saturation as f64 - 1.) * 2.
            },
            detail: f64::from(settings.effective_sharpen_amount(metadata)?) - 1.,
        },
        [1.; 3],
        adjustments,
        // The reference profile uses a pure power response, without the
        // standard ROMM shadow toe. Primaries remain ROMM after decoding.
        TransferFunction::Power(PROFILE_POWER),
    )?;
    Ok(finish)
}

/// Build an editable camera basis and approximate selected-mode mapping.
pub(crate) fn prepare_dng_appearance(
    metadata: &Metadata,
    settings: &Settings,
    statistics: crate::render::WhiteBalanceStats,
) -> Result<crate::dng::appearance::ClassicAppearance> {
    use crate::dng::appearance::{ClassicAppearance, Matrix};
    validate_request(metadata, settings)?;
    if settings.is_monochrome() {
        let identity = Matrix::diagonal([1.; 3]);
        // The selected monochrome mixture is singular and already baked into
        // the technical base. Replicate its green signal for exactly neutral
        // stored RGB and defer only its response.
        return Ok(ClassicAppearance {
            mapping: [0., 1. / 4096., 0., 0., 1. / 4096., 0., 0., 1. / 4096., 0.],
            neutral: [1.; 3],
            camera_to_linear: identity.0,
            selected_from_base: identity.0,
            response: prepare_photographic(metadata, settings)?,
        });
    }
    let prepared = prepare_with_white_balance(metadata, &settings.white_balance, statistics)?;
    let matrix = |color: &Color| {
        let p = &color.transform.params;
        Matrix(std::array::from_fn(|i| {
            f64::from(p.matrix[i / 3][i % 3]) / f64::from(1u32 << p.source[3])
        }))
    };
    let (base, selected) = if capture_iso(metadata)? >= RANGE_RECONSTRUCTION_ISO {
        (
            Matrix(super::super::range::color_matrix(
                metadata,
                &prepared.color,
                None,
            )?),
            Matrix(super::super::range::color_matrix(
                metadata,
                &prepared.color,
                Some(&settings.color_mode),
            )?),
        )
    } else {
        let base = prepare_unselected_color(metadata, &prepared.color)?;
        let selected = prepare_color(metadata, &prepared.color, &settings.color_mode)?;
        (matrix(&base), matrix(&selected))
    };
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
        selected_from_base: base.inverse()?.then(selected).0,
        response: prepare_photographic(metadata, settings)?,
    })
}
