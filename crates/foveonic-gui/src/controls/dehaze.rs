//! Presents Dehaze strength, automatic shadow tone, a color swatch and on-image picking.

use super::common;
use crate::{editor, help, widgets};
use eframe::egui;
use foveonic::render::Settings;

pub(super) fn show(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut changed = false;
    widgets::parameter_group(ui, |ui| {
        let column = common::LabelColumn::new(ui);
        changed |= column.adjustment(
            ui,
            ("Dehaze", "Dehaze"),
            &mut settings.dehaze.amount,
            -100. ..=100.,
            0.,
        );
        ui.horizontal(|ui| {
            column.label(ui, "Shadow tone");
            let estimated = editor::dehaze::auto_color(ui.ctx(), settings);
            let automatic = settings.dehaze.shadow.is_none();
            let auto = widgets::auto_button(ui, automatic);
            if auto
                .on_hover_text("Estimate the haze color from the whole photograph. Turn off to adjust this color manually.")
                .clicked()
            {
                changed |= editor::dehaze::set_auto(ui.ctx(), settings, !automatic);
            }
            if let Some(color) = color_swatch(ui, settings.dehaze.shadow.or(estimated)) {
                settings.dehaze.shadow = Some(color);
                changed = true;
                editor::dehaze::exit(ui.ctx());
            }
            let active = editor::dehaze::active(ui.ctx());
            let button = widgets::icon_button(ui, "Pick haze shadow tone", active);
            widgets::eyedropper_icon(ui, &button, active);
            if help::control(button, "Haze shadow tone").clicked() {
                editor::direct_color::exit(ui.ctx());
                editor::dehaze::toggle(ui.ctx());
            }
        });
        let s = editor::dehaze::state(ui.ctx());
        if !s.notice.is_empty() {
            ui.weak(s.notice);
        }
    });
    changed
}

/// The swatch edits display-encoded values while recipes retain linear sRGB.
fn color_swatch(ui: &mut egui::Ui, color: Option<[f32; 3]>) -> Option<[f32; 3]> {
    let height = widgets::control_height(ui);
    ui.scope(|ui| {
        ui.spacing_mut().interact_size = egui::vec2(2. * height, height);
        if let Some(color) = color {
            let mut rgb = color.map(egui::ecolor::gamma_u8_from_linear_f32);
            let response = ui
                .color_edit_button_srgb(&mut rgb)
                .on_hover_text("Set a manual haze color");
            if response.changed() {
                let rgb = rgb.map(egui::ecolor::linear_f32_from_gamma_u8);
                return rgb.iter().any(|v| *v > 0.).then_some(rgb);
            }
        } else {
            let (rect, response) =
                ui.allocate_exact_size(ui.spacing().interact_size, egui::Sense::hover());
            let stroke = egui::Stroke::new(1_f32, ui.visuals().weak_text_color());
            ui.painter().rect(
                rect,
                2.,
                egui::Color32::TRANSPARENT,
                stroke,
                egui::StrokeKind::Inside,
            );
            let corners = rect.shrink(0.5);
            ui.painter()
                .line_segment([corners.left_top(), corners.right_bottom()], stroke);
            ui.painter()
                .line_segment([corners.right_top(), corners.left_bottom()], stroke);
            response.on_hover_text("The automatic haze color is not available yet.");
        }
        None
    })
    .inner
}
