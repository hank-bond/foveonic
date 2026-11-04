//! Selects the camera family's editing panel and shares numerical editing widgets.

mod advanced_color;
mod classic;
mod color_editor;
mod color_wheel;
mod common;
pub use common::adaptive_limits;
mod dehaze;
mod merrill;
mod quattro;
mod saturation;
mod sections;

use crate::preferences::EditingSection;
use eframe::egui;
use foveonic::{
    render::{ControlCapabilities, Settings},
    source::{Metadata, ProcessingFamily},
};

/// Reset unsupported adjustments when the selected camera's metadata arrives.
pub fn normalize_incompatible(settings: &mut Settings, metadata: Option<&Metadata>) -> bool {
    metadata
        .and_then(|metadata| ControlCapabilities::from_metadata(metadata).ok())
        .is_some_and(|capabilities| capabilities.normalize(settings))
}

/// Use bounded capture properties until complete processing metadata is available.
fn capabilities(
    metadata: Option<&Metadata>,
    capture: Option<&std::collections::BTreeMap<String, String>>,
) -> Option<ControlCapabilities> {
    metadata
        .and_then(|m| ControlCapabilities::from_metadata(m).ok())
        .or_else(|| {
            capture?
                .get("CAMMODEL")
                .and_then(|model| ProcessingFamily::from_camera_model(model))
                .map(ControlCapabilities::from_family)
        })
}

pub fn normalize_with_capture(
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    capture: Option<&std::collections::BTreeMap<String, String>>,
) -> bool {
    if metadata.is_some() {
        normalize_incompatible(settings, metadata)
    } else {
        capabilities(None, capture).is_some_and(|c| c.normalize(settings))
    }
}

pub fn processing(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    capture: Option<&std::collections::BTreeMap<String, String>>,
    order: &mut Vec<EditingSection>,
    menus: &mut crate::editor::Menus<'_>,
) -> bool {
    let exposure = metadata
        .map(|m| foveonic::source::CaptureExposureCompensation::from_properties(&m.properties))
        .or_else(|| capture.map(foveonic::source::CaptureExposureCompensation::from_properties));
    match capabilities(metadata, capture).map(|c| c.family) {
        Some(ProcessingFamily::Classic) => {
            classic::show(ui, settings, metadata, exposure, order, menus)
        }
        Some(ProcessingFamily::Merrill) => merrill::show(
            ui,
            settings,
            metadata,
            exposure,
            metadata
                .map(|m| m.rotation)
                .or_else(|| capture?.get("ROTATION")?.parse().ok())
                .unwrap_or(0),
            order,
            menus,
        ),
        Some(ProcessingFamily::Quattro) => {
            quattro::show(ui, settings, metadata, capture, exposure, order, menus)
        }
        None => {
            let mut changed = common::white_balance(ui, settings, metadata, true);
            changed |= common::color_mode(ui, settings, metadata, !settings.is_monochrome());
            ui.separator();
            changed |= common::tone(ui, settings, exposure);
            changed |= saturation::show(ui, settings);
            changed
        }
    }
}

#[cfg(test)]
mod tests;
