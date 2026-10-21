//! Selects the camera family's editing panel and shares numerical editing widgets.

mod classic;
mod common;
mod merrill;

pub use common::{adjustment_with_nudge, automatic_adjustment};
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
) -> bool {
    match capabilities(metadata, capture).map(|c| c.family) {
        Some(ProcessingFamily::Classic) => classic::show(ui, settings, metadata),
        Some(ProcessingFamily::Merrill) => merrill::show(ui, settings, metadata),
        None => {
            let mut changed = common::presets(
                ui,
                settings,
                metadata,
                common::PresetControls {
                    white_balance: true,
                    color_mode: !settings.is_monochrome(),
                },
            );
            ui.separator();
            changed |= common::tone(ui, settings, settings.is_monochrome());
            changed
        }
    }
}

#[cfg(test)]
mod tests;
