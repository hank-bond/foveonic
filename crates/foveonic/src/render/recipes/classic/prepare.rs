//! Shares Classic preparation contracts and metadata helpers between calibration, color, reconstruction, and appearance.
//!
//! Equations follow the independently recovered reference implementation. GPU operations receive functional parameters.

use super::reference::{D65_WHITE_XYZ, RANGE_RECONSTRUCTION_ISO};

use crate::{
    render::Settings,
    source::{Metadata, Result},
};

pub(crate) use super::highlight::prepare as prepare_highlight;

use crate::nodes::sensor::fixed_point::{Correction, Transform};

pub struct Prepared {
    pub correction: Correction,
    pub final_gain: [f64; 3],
    pub color: ColorInput,
}

/// White-balance-derived inputs retained until the recipe reaches camera color.
pub struct ColorInput {
    pub(super) monochrome: bool,
    pub(super) camera: [f64; 9],
    pub(super) final_gain: [f64; 3],
    pub(super) relative_gain: [f64; 3],
    /// Capture ISO divided by sensor ISO, using sensor ISO 100 when it is zero.
    pub(super) ratio: f32,
    pub(super) digital_gain: f32,
    pub(super) extent: [usize; 2],
    // Cached color calibration scales both input statistics and nonlinear noise.
    pub(super) statistics_factor: f64,
    pub(super) alternate_matrix: Option<([f64; 9], f64)>,
}

impl ColorInput {
    /// Reference fallback amplitude when all ColorDQCamRGB components are zero.
    /// The ISO ratio is capped before the two square roots, preserving rounding.
    pub(super) fn default_nonlinear_noise(&self) -> [f64; 3] {
        [4. / f64::from(self.ratio.min(8.)).sqrt().sqrt(); 3]
    }
}

pub struct Color {
    pub transform: Transform,
    pub matrix_green_sum: f32,
    pub digital_gain: f32,
}

/// Reject unsupported request branches before preparing calibration.
pub(crate) fn validate_request(metadata: &Metadata, settings: &Settings) -> Result<()> {
    crate::render::ControlCapabilities::from_metadata(metadata)?.validate(settings)?;
    if scalar(metadata, "SkipStepFlags", 0.)? as u32 & !(1 << 11) != 0 {
        return Err("Classic rendering encountered an unsupported processing-step flag".into());
    }
    if !settings.is_monochrome()
        && capture_iso(metadata)? < RANGE_RECONSTRUCTION_ISO
        && settings.reconstruction.is_some()
        && scalar(metadata, "FIR_Iterations", 1.)? != 1.
    {
        return Err("Classic reconstruction currently supports a single FIR pass".into());
    }
    Ok(())
}

pub(super) fn capture_iso(metadata: &Metadata) -> Result<f64> {
    let request = metadata
        .properties
        .get("ISO")
        .ok_or("missing ISO")?
        .parse::<f64>()?;
    let capture = scalar(metadata, "CaptureISO", request)?;
    Ok(if capture == 0. { request } else { capture })
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
        inv[r * 3] * D65_WHITE_XYZ[0]
            + inv[r * 3 + 1] * D65_WHITE_XYZ[1]
            + inv[r * 3 + 2] * D65_WHITE_XYZ[2]
    }))
}

mod appearance;
mod calibration;
mod color;
mod reconstruction;

pub(crate) use appearance::{prepare_dng_appearance, prepare_photographic, prepare_root};
pub(crate) use calibration::{
    geometry, prepare, prepare_despeckle, prepare_red_detail, prepare_with_white_balance,
};
pub(crate) use color::{
    prepare_color, prepare_unselected_color, selected_camera, validate_color_mode,
};
pub(crate) use reconstruction::{
    prepare_chroma, prepare_reconstruction, prepare_reconstruction_gain, prepare_recursive_chroma,
};

#[cfg(test)]
use super::reference::Q14_SCALE;
#[cfg(test)]
use crate::nodes::{
    color::chroma::{self, Support},
    sensor::signed_repair,
};
#[cfg(test)]
use calibration::{monochrome_camera, prepare_column_filters};
#[cfg(test)]
use color::{transform, MatrixScale};

#[cfg(test)]
#[path = "prepare_test.rs"]
mod tests;
