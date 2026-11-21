//! Selects Merrill photographic and editable DNG recipes over their shared technical stages.

mod appearance;
mod dng_base;
mod layer_noise;
mod monochrome;
mod native_layers;
mod photographic;
mod preview;
mod reconstruction;
mod region;
mod technical;

#[cfg(test)]
mod tonal_ranges_test;

#[cfg(test)]
mod noise_test;

#[cfg(test)]
mod sharpening_test;

pub(super) use dng_base::build as build_dng;
pub(super) use photographic::build;
pub(in crate::render) use region::available as regional_available;

pub(super) use appearance::prepare as prepare_dng_appearance;

use crate::{
    render::Settings,
    source::{Metadata, Result},
};

/// The original applies less final sharpening to reduced RAW resolutions.
const MEDIUM_RESOLUTION_SHARPENING: f32 = 0.840_896_4; // 2^(-1/4)
const LOW_RESOLUTION_SHARPENING: f32 = 0.707_106_77; // 2^(-1/2)

/// The amount final sharpening applies. Like the original, reduced RAW
/// resolutions and reduced working sizes sharpen less, by the square root of
/// the size ratio, so a reduced preview resembles its full-size result.
/// Export output size is not a working size and does not change it.
fn applied_sharpen_amount(settings: &Settings, metadata: &Metadata) -> Result<f32> {
    let resolution = match metadata.properties.get("RESOLUTION").map(String::as_str) {
        Some("MED") => MEDIUM_RESOLUTION_SHARPENING,
        Some("LOW") => LOW_RESOLUTION_SHARPENING,
        _ => 1.,
    };
    Ok(settings.effective_sharpen_amount(metadata)?
        * resolution
        * settings.working_scale.factor().sqrt())
}
