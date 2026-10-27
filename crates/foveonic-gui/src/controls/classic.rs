//! Composes Classic controls using the camera's built-in reconstruction and corrections.

use super::{common, sections};
use crate::help;
use crate::preferences::EditingSection;
use eframe::egui;
use foveonic::{
    render::{ControlCapabilities, Settings},
    source::{Metadata, ProcessingFamily},
};

pub(super) fn show(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    order: &mut Vec<EditingSection>,
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
    changed |= sections::show(
        ui,
        order,
        &[
            EditingSection::Tone,
            EditingSection::ColorEditor,
            EditingSection::Noise,
            EditingSection::Sharpening,
            EditingSection::Corrections,
        ],
        |ui, section| match section {
            EditingSection::Tone => common::tone(ui, settings, mono),
            EditingSection::ColorEditor => super::color_editor::show(ui, settings),
            EditingSection::Noise => common::reconstruction(ui, settings),
            EditingSection::Sharpening => common::sharpening(ui, settings, metadata),
            EditingSection::Corrections => corrections(ui, settings, capabilities),
            _ => false,
        },
    );
    changed
}

fn corrections(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    capabilities: ControlCapabilities,
) -> bool {
    let mut changed = false;
    common::section_header(ui, EditingSection::Corrections, "Corrections", None);
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
