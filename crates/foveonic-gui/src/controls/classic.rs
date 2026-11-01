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
    menus: &mut crate::editor::Menus<'_>,
) -> bool {
    let mut changed = common::rendering(ui, settings);
    let capabilities = ControlCapabilities::from_family(ProcessingFamily::Classic);
    let mono = settings.is_monochrome();
    changed |= common::color_mode(
        ui,
        settings,
        metadata,
        capabilities.color_mode_active(settings),
    );
    changed |= sections::show(
        ui,
        order,
        &[
            EditingSection::WhiteBalance,
            EditingSection::Tone,
            EditingSection::ColorEditor,
            EditingSection::Noise,
            EditingSection::Sharpening,
            EditingSection::Corrections,
        ],
        menus,
        |ui, section| match section {
            EditingSection::WhiteBalance => common::white_balance(
                ui,
                settings,
                metadata,
                foveonic::render::ControlCapabilities::from_family(ProcessingFamily::Classic)
                    .white_balance_active(settings),
            ),
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
    let advanced = crate::widgets::disclosure(
        ui,
        egui::CollapsingHeader::new("Advanced").id_salt("advanced-corrections"),
        |ui| changed |= common::pixel_corrections(ui, settings),
    );
    help::control(advanced.header_response, "Advanced corrections");
    changed
}
