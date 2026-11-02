//! Stores direct-color gesture preferences and draws the eyedropper and configuration popover.

use super::Channel;
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gesture {
    Horizontal,
    Vertical,
    AltHorizontal,
    AltVertical,
}
impl Gesture {
    const ALL: [Self; 4] = [
        Self::Horizontal,
        Self::Vertical,
        Self::AltHorizontal,
        Self::AltVertical,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "Horizontal",
            Self::Vertical => "Vertical",
            Self::AltHorizontal => "Option + Horizontal",
            Self::AltVertical => "Option + Vertical",
        }
    }
    pub(super) fn from_axis(horizontal: bool, alt: bool) -> Self {
        match (horizontal, alt) {
            (true, false) => Self::Horizontal,
            (false, false) => Self::Vertical,
            (true, true) => Self::AltHorizontal,
            (false, true) => Self::AltVertical,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Configuration {
    /// Hue, Saturation and Lightness. Each gesture controls at most one adjustment.
    pub gestures: [Gesture; 3],
    pub sensitivity: f32,
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            gestures: [Gesture::Horizontal, Gesture::AltVertical, Gesture::Vertical],
            sensitivity: 50.,
        }
    }
}
impl Configuration {
    pub fn normalize(&mut self) {
        if !self.sensitivity.is_finite() {
            self.sensitivity = 50.;
        }
        self.sensitivity = self.sensitivity.clamp(1., 100.);
        for index in 0..3 {
            if self.gestures[..index].contains(&self.gestures[index]) {
                self.gestures[index] = *Gesture::ALL
                    .iter()
                    .find(|g| !self.gestures.contains(g))
                    .unwrap();
            }
        }
    }
    pub(super) fn channel(self, gesture: Gesture) -> Option<Channel> {
        self.gestures
            .iter()
            .position(|g| *g == gesture)
            .map(|i| Channel::ALL[i])
    }
    pub(super) fn assign(&mut self, index: usize, gesture: Gesture) {
        let previous = self.gestures[index];
        if let Some(other) = self.gestures.iter().position(|g| *g == gesture) {
            self.gestures[other] = previous;
        }
        self.gestures[index] = gesture;
    }
}

pub(super) fn controls(ui: &mut egui::Ui, config: &mut Configuration) {
    let width = crate::widgets::dropdown_width(ui, Gesture::ALL.map(Gesture::label));
    let label_width = ui
        .painter()
        .layout_no_wrap(
            "Sensitivity".into(),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().text_color(),
        )
        .size()
        .x;
    ui.set_min_width(width + label_width + ui.spacing().item_spacing.x);
    ui.label("Direct color settings");
    ui.separator();
    egui::Grid::new("direct color mappings")
        .spacing(egui::vec2(10., 6.))
        .show(ui, |ui| {
            for (index, label) in ["Hue", "Saturation", "Lightness"].into_iter().enumerate() {
                ui.label(label);
                let mut gesture = config.gestures[index];
                crate::widgets::dropdown(
                    ui,
                    egui::ComboBox::from_id_salt(label)
                        .selected_text(gesture.label())
                        .width(width),
                    |ui| {
                        for choice in Gesture::ALL {
                            crate::widgets::menu_value(ui, &mut gesture, choice, choice.label());
                        }
                    },
                );
                if gesture != config.gestures[index] {
                    config.assign(index, gesture);
                }
                ui.end_row();
            }
            ui.label("Sensitivity");
            ui.add(crate::widgets::slider(&mut config.sensitivity, 1. ..=100.));
            ui.end_row();
        });
    ui.add_space(4.);
    ui.add(
        egui::Label::new(
            egui::RichText::new("Right-click the photograph to open these settings.").weak(),
        )
        .wrap(),
    );
}

pub(super) fn tools(ui: &mut egui::Ui, on: bool, config: &mut Configuration) -> (bool, bool) {
    let mut toggle = false;
    let mut menu_open = false;
    crate::widgets::centered_control_row(ui, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let response = crate::widgets::icon_button(ui, "Direct color", on);
            let icon = crate::widgets::icon_rect(ui, &response);
            let scale = icon.width() / 16.;
            let at = |x, y| icon.min + egui::vec2(x, y) * scale;
            let color = crate::widgets::visuals(ui, &response, on).fg_stroke.color;
            let stroke = egui::Stroke::new(1.5 * scale, color);
            let barrel = vec![
                at(1., 15.),
                at(2., 11.),
                at(10., 3.),
                at(13., 6.),
                at(5., 14.),
            ];
            ui.painter().add(egui::Shape::closed_line(barrel, stroke));
            ui.painter().line_segment([at(8., 1.), at(15., 8.)], stroke);
            ui.painter()
                .circle_filled(at(13.5, 2.5), 2.5 * scale, color);
            toggle = response.clicked();
            response.on_hover_text(
                "Direct color (D) · Click and drag a subject color. Escape exits; Space pans. \
                 Configure the axes with the settings button or right-click in the photograph.",
            );
            let menu = crate::widgets::icon_menu_button(ui, "Direct color settings", |ui| {
                controls(ui, config)
            });
            menu_open = menu.inner.is_some();
            let response = menu.response;
            let icon = crate::widgets::icon_rect(ui, &response);
            let scale = icon.width() / 16.;
            let at = |x, y| icon.min + egui::vec2(x, y) * scale;
            let color = crate::widgets::visuals(ui, &response, menu_open)
                .fg_stroke
                .color;
            for (y, x) in [(3., 5.), (8., 11.), (13., 6.)] {
                ui.painter()
                    .line_segment([at(1., y), at(15., y)], (1.2 * scale, color));
                ui.painter().circle_filled(at(x, y), 1.8 * scale, color);
            }
            response.on_hover_text("Direct color settings");
        });
    });
    (toggle, menu_open)
}
