//! Describes the decoded-plane inspection available before Quattro rendering.

use eframe::egui;

pub(super) fn show(ui: &mut egui::Ui) -> bool {
    ui.label("Quattro rendering is not available yet.");
    ui.label("Use Inspect to inspect the decoded RGB and blue planes.");
    false
}
