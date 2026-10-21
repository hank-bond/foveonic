//! Composes Classic controls using the camera's built-in reconstruction and corrections.

use super::common;
use crate::help;
use eframe::egui;
use foveonic::{
    render::{ControlCapabilities, Settings},
    source::{Metadata, ProcessingFamily},
};

pub(super) fn show(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
) -> bool {
    let mut changed = common::rendering(ui, settings);
    let capabilities = ControlCapabilities::from_family(ProcessingFamily::Classic);
    let mono = settings.is_monochrome();
    changed |= common::presets(
        ui,
        settings,
        metadata,
        common::PresetControls {
            white_balance: capabilities.white_balance_active(settings),
            color_mode: capabilities.color_mode_active(settings),
        },
    );
    ui.separator();
    changed |= common::tone(ui, settings, mono);
    ui.separator();
    changed |= common::reconstruction(ui, settings);
    ui.separator();
    changed |= common::refinement(ui, settings, metadata);
    ui.separator();
    ui.weak("Corrections");
    changed |= ui
        .add_enabled_ui(capabilities.highlights_active(settings), |ui| {
            crate::widgets::checkbox(ui, &mut settings.highlight_restoration, "Highlights")
                .changed()
        })
        .inner;
    let advanced = egui::CollapsingHeader::new("Advanced")
        .id_salt("advanced-corrections")
        .show(ui, |ui| changed |= common::pixel_corrections(ui, settings));
    help::control(advanced.header_response, "Advanced corrections");
    changed
}
