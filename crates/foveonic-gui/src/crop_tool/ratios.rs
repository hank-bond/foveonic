//! Presents shared crop constraints and stores named custom ratios in application preferences.

use crate::widgets::{self, ControlSize};
use eframe::egui;
use foveonic::crop::{Aspect, Crop};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NamedRatio {
    pub name: String,
    pub dimensions: [f64; 2],
}
impl NamedRatio {
    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.chars().count() <= 64
            && Crop {
                aspect: Aspect::Ratio(self.dimensions),
                ..Default::default()
            }
            .validate()
            .is_ok()
    }
}

#[derive(Clone)]
pub(super) enum Dialog {
    Add(NamedRatio),
    Remove(Vec<bool>),
}
pub(super) enum Action {
    Aspect(Aspect),
    Flip,
    Reset,
}

pub(super) fn menu(
    ui: &mut egui::Ui,
    crop: Crop,
    custom: &[NamedRatio],
    dialog: &mut Option<Dialog>,
) -> Option<Action> {
    let mut action = None;
    if widgets::menu_row(ui, "Reset crop", false).clicked() {
        action = Some(Action::Reset);
        ui.close_menu();
    }
    if ui
        .add_enabled_ui(crop.aspect != Aspect::Unconstrained, |ui| {
            widgets::menu_row(ui, "Switch crop orientation", false)
        })
        .inner
        .clicked()
    {
        action = Some(Action::Flip);
        ui.close_menu();
    }
    ui.separator();
    let mut selected = crop.aspect;
    for (aspect, label) in [
        (Aspect::Unconstrained, "Unconstrained"),
        (Aspect::Original, "Original"),
    ] {
        if widgets::menu_value(ui, &mut selected, aspect, label).changed() {
            action = Some(Action::Aspect(aspect));
            ui.close_menu();
        }
    }
    ui.separator();
    for (label, w, h) in [
        ("Square", 1., 1.),
        ("1 × 2", 1., 2.),
        ("1 × 3", 1., 3.),
        ("2 × 3", 2., 3.),
        ("3 × 4", 3., 4.),
        ("4 × 5", 4., 5.),
        ("5 × 7", 5., 7.),
        ("16 × 9", 16., 9.),
        ("65 × 22 Xpan", 65., 22.),
        ("21 × 9 Cinema", 21., 9.),
    ] {
        let aspect = Aspect::Ratio([w, h]);
        if widgets::menu_value(ui, &mut selected, aspect, label).changed() {
            action = Some(Action::Aspect(aspect));
            ui.close_menu();
        }
    }
    if !custom.is_empty() {
        ui.separator();
        for ratio in custom {
            let aspect = Aspect::Ratio(ratio.dimensions);
            if widgets::menu_value(ui, &mut selected, aspect, &ratio.name).changed() {
                action = Some(Action::Aspect(aspect));
                ui.close_menu();
            }
        }
    }
    ui.separator();
    if widgets::menu_row(ui, "Add aspect ratio…", false).clicked() {
        *dialog = Some(Dialog::Add(NamedRatio {
            name: String::new(),
            dimensions: [1., 1.],
        }));
        ui.close_menu();
    }
    if ui
        .add_enabled_ui(!custom.is_empty(), |ui| {
            widgets::menu_row(ui, "Remove aspect ratios…", false)
        })
        .inner
        .clicked()
    {
        *dialog = Some(Dialog::Remove(vec![false; custom.len()]));
        ui.close_menu();
    }
    action
}

pub(super) fn show_dialog(
    ctx: &egui::Context,
    dialog: &mut Option<Dialog>,
    custom: &mut Vec<NamedRatio>,
) -> Option<Aspect> {
    let mut open = true;
    let mut close = false;
    let mut apply = None;
    let current = dialog.as_mut()?;
    let title = match current {
        Dialog::Add(_) => "Add aspect ratio",
        Dialog::Remove(_) => "Remove aspect ratios",
    };
    egui::Window::new(title)
        .id(egui::Id::new("Crop ratio dialog"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| match current {
            Dialog::Add(ratio) => {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(
                        egui::TextEdit::singleline(&mut ratio.name)
                            .char_limit(64)
                            .desired_width(180.),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Width");
                    ui.add(
                        widgets::drag_value(&mut ratio.dimensions[0])
                            .range(0.01..=10000.)
                            .speed(0.1),
                    );
                    ui.label("Height");
                    ui.add(
                        widgets::drag_value(&mut ratio.dimensions[1])
                            .range(0.01..=10000.)
                            .speed(0.1),
                    );
                });
                let valid = ratio.valid() && !custom.iter().any(|r| r.name == ratio.name.trim());
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled_ui(valid, |ui| {
                            widgets::button_sized(ui, "Add", false, ControlSize::Large)
                        })
                        .inner
                        .clicked()
                    {
                        let mut ratio = ratio.clone();
                        ratio.name = ratio.name.trim().into();
                        apply = Some(Aspect::Ratio(ratio.dimensions));
                        custom.push(ratio);
                        close = true;
                    }
                    if widgets::button_sized(ui, "Cancel", false, ControlSize::Large).clicked() {
                        close = true;
                    }
                });
            }
            Dialog::Remove(selected) => {
                selected.resize(custom.len(), false);
                for (ratio, remove) in custom.iter().zip(selected.iter_mut()) {
                    widgets::checkbox(ui, remove, &ratio.name);
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled_ui(selected.iter().any(|v| *v), |ui| {
                            widgets::button_sized(ui, "Remove", false, ControlSize::Large)
                        })
                        .inner
                        .clicked()
                    {
                        let mut index = 0;
                        custom.retain(|_| {
                            let keep = !selected[index];
                            index += 1;
                            keep
                        });
                        close = true;
                    }
                    if widgets::button_sized(ui, "Cancel", false, ControlSize::Large).clicked() {
                        close = true;
                    }
                });
            }
        });
    if !open || close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *dialog = None;
    }
    apply
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_ratios_are_plain_preferences_and_the_crop_retains_values_after_removal() {
        let ratios = vec![NamedRatio {
            name: "Panorama".into(),
            dimensions: [2.39, 1.],
        }];
        let bytes = serde_json::to_vec(&ratios).unwrap();
        let mut restored: Vec<NamedRatio> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored, ratios);
        assert!(restored[0].valid());
        let crop = Crop {
            aspect: Aspect::Ratio(restored.remove(0).dimensions),
            ..Default::default()
        };
        assert_eq!(crop.ratio([600, 400]), Some(2.39));
    }
}
