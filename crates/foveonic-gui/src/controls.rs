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

/// The capture's exposure compensation, which Auto exposure follows.
fn capture_exposure(
    summary: Option<&MetadataSummary>,
) -> Option<foveonic::source::CaptureExposureCompensation> {
    summary.map(|summary| {
        foveonic::source::CaptureExposureCompensation::from_properties(&summary.properties)
    })
}

/// The panel for the photo's camera family, from its summary; full `metadata`
/// adds camera presets once a render has read it. Controls draw `settings`, the
/// selected photo's, and push their edits. `range_preview` is the viewed
/// Advanced color range, which is view state rather than an edit.
pub fn processing(
    ui: &mut egui::Ui,
    settings: &Settings,
    range_preview: &mut Option<usize>,
    metadata: Option<&Metadata>,
    summary: Option<&MetadataSummary>,
    order: &mut Vec<EditingSection>,
    menus: &mut crate::editor::Menus<'_>,
) -> bool {
    match summary.and_then(|summary| summary.family) {
        Some(ProcessingFamily::Classic) => {
            classic::show(ui, settings, range_preview, metadata, summary, order, menus)
        }
        Some(ProcessingFamily::Merrill) => {
            merrill::show(ui, settings, range_preview, metadata, summary, order, menus)
        }
        Some(ProcessingFamily::Quattro) => {
            quattro::show(ui, settings, range_preview, metadata, summary, order, menus)
        }
        None => {
            let mut changed = common::white_balance(ui, settings, metadata, true);
            changed |= common::color_mode(ui, settings, metadata, !settings.is_monochrome());
            ui.separator();
            changed |= common::tone(ui, settings, capture_exposure(summary));
            changed |= saturation::show(ui, settings);
            changed
        }
    }
}

#[cfg(test)]
mod tests;
