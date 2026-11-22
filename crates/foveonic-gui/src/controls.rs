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
pub(crate) mod saturation;
mod sections;

use crate::preferences::EditingSection;
use eframe::egui;
use foveonic::{
    render::{ControlCapabilities, Settings},
    source::{Metadata, MetadataSummary, ProcessingFamily},
};

/// Reset adjustments the photo's camera family does not offer.
pub fn normalize(settings: &mut Settings, family: Option<ProcessingFamily>) -> bool {
    family.is_some_and(|family| ControlCapabilities::from_family(family).normalize(settings))
}

/// The panel for the photo's camera family, from its summary; full `metadata`
/// adds camera presets once a render has read it.
pub fn processing(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    summary: Option<&MetadataSummary>,
    order: &mut Vec<EditingSection>,
    menus: &mut crate::editor::Menus<'_>,
) -> bool {
    let exposure = summary.map(|summary| {
        foveonic::source::CaptureExposureCompensation::from_properties(&summary.properties)
    });
    match summary.and_then(|summary| summary.family) {
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
                .or_else(|| summary?.properties.get("ROTATION")?.parse().ok())
                .unwrap_or(0),
            order,
            menus,
        ),
        Some(ProcessingFamily::Quattro) => {
            quattro::show(ui, settings, metadata, summary, exposure, order, menus)
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
