//! Collects scoped recipe actions from header buttons and a named-preset dialog.

use super::{Library, Scope};
use eframe::egui;
use foveonic::{render::Settings, source::ProcessingFamily};

pub enum Action {
    NoiseCalibration,
    AdaptiveLimits,
    PreventSaturationClipping(bool),
    ExposureAutoCompensation(foveonic::render::AutoExposureCompensation),
    Reset(Scope, bool),
    Apply(Scope, Box<Settings>),
    Save(Scope),
    Default(Scope),
    ClearDefaults,
    Delete(usize),
}

#[derive(Default)]
pub struct State {
    pub actions: Vec<Action>,
    save: Option<Save>,
}
struct Save {
    scope: Scope,
    family: ProcessingFamily,
    settings: Settings,
    name: String,
    error: Option<String>,
    focused: bool,
}

pub struct Menus<'a> {
    pub prevent_color_clipping: bool,
    pub exposure_auto_compensation: foveonic::render::AutoExposureCompensation,
    pub family: Option<ProcessingFamily>,
    pub library: &'a Library,
    pub state: &'a mut State,
}
impl Menus<'_> {
    /// Overlay the two reserved header slots immediately before the drag grip.
    pub fn header(&mut self, ui: &mut egui::Ui, scope: Scope, rect: egui::Rect) {
        let bounds = egui::Rect::from_center_size(
            egui::pos2(
                rect.right() - 48.,
                if scope == Scope::All {
                    rect.center().y
                } else {
                    rect.top() + crate::widgets::control_height(ui) * 0.5
                },
            ),
            egui::vec2(44., 20.),
        );
        let mut overlay = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bounds)
                .id_salt(format!("recipe actions {scope:?}")),
        );
        {
            let ui = &mut overlay;
            ui.set_min_size(bounds.size());
            ui.spacing_mut().item_spacing.x = 2.;
            ui.horizontal(|ui| {
                ui.add_enabled_ui(self.family.is_some(), |ui| {
                    let (r, response) =
                        ui.allocate_exact_size(egui::vec2(20., 20.), egui::Sense::click());
                    crate::widgets::paint(ui, &response, false);
                    let color = crate::widgets::visuals(ui, &response, false)
                        .fg_stroke
                        .color;
                    let center = r.center();
                    let mut points = vec![center + egui::vec2(-1.4, -1.8)];
                    points.extend((0..=16).map(|i| {
                        let angle =
                            -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 16.;
                        center + egui::vec2(1.4, 0.8) + egui::vec2(angle.cos(), angle.sin()) * 2.6
                    }));
                    points.push(center + egui::vec2(-1.4, 3.4));
                    ui.painter()
                        .add(egui::Shape::line(points, egui::Stroke::new(1.5_f32, color)));
                    ui.painter().add(egui::Shape::convex_polygon(
                        vec![
                            center + egui::vec2(-4.6, -1.8),
                            center + egui::vec2(-1.4, -4.4),
                            center + egui::vec2(-1.4, 0.8),
                        ],
                        color,
                        egui::Stroke::NONE,
                    ));
                    if response.clicked() {
                        self.state
                            .actions
                            .push(Action::Reset(scope, ui.input(|i| i.modifiers.alt)));
                    }
                    response
                        .on_hover_text("Reset to defaults · Option/Alt-click for camera As Shot");
                    let menu = crate::widgets::menu_button(
                        ui,
                        egui::Button::new("").min_size(egui::vec2(20., 20.)),
                        |ui| self.menu(ui, scope),
                    )
                    .response;
                    let color = ui.style().interact(&menu).fg_stroke.color;
                    for y in [-3., 0., 3.] {
                        ui.painter().line_segment(
                            [
                                menu.rect.center() + egui::vec2(-4., y),
                                menu.rect.center() + egui::vec2(4., y),
                            ],
                            (1.2_f32, color),
                        );
                    }
                    menu.on_hover_text("Settings, presets and defaults");
                });
            });
        }
    }
    fn menu(&mut self, ui: &mut egui::Ui, scope: Scope) {
        let Some(family) = self.family else { return };
        let editing = scope == Scope::All;
        if editing {
            let mut protect = self.prevent_color_clipping;
            if crate::help::control(
                crate::widgets::checkbox(ui, &mut protect, "Prevent saturation clipping"),
                "Prevent color clipping",
            )
            .changed()
            {
                self.state
                    .actions
                    .push(Action::PreventSaturationClipping(protect));
                ui.close_menu();
            }
            ui.separator();
        }
        let tone = scope == Scope::Tool(super::Section::Tone);
        let calibration =
            family == ProcessingFamily::Merrill && scope == Scope::Tool(super::Section::Noise);
        let has_presets = self.library.choices(family, scope).next().is_some();
        if tone {
            if ui.button("Adaptive limits…").clicked() {
                self.state.actions.push(Action::AdaptiveLimits);
                ui.close_menu();
            }
            ui.separator();
        }
        if tone {
            use foveonic::render::AutoExposureCompensation::{Both, Bracket, Dial};
            let mut selected = self.exposure_auto_compensation;
            if crate::widgets::menu_choices(
                ui,
                "Auto exposure source",
                &mut selected,
                &[(Bracket, "Bracket"), (Dial, "Dial"), (Both, "Both")],
            ) {
                self.state
                    .actions
                    .push(Action::ExposureAutoCompensation(selected));
            }
        }
        if calibration && ui.button("Calibrate noise…").clicked() {
            self.state.actions.push(Action::NoiseCalibration);
            ui.close_menu();
        }
        if (tone || calibration) && has_presets {
            ui.separator();
        }
        for (index, preset) in self.library.choices(family, scope) {
            ui.horizontal(|ui| {
                if ui.button(&preset.name).clicked() {
                    self.state
                        .actions
                        .push(Action::Apply(scope, Box::new(preset.settings.clone())));
                    ui.close_menu();
                }
                if ui
                    .small_button("×")
                    .on_hover_text("Delete preset")
                    .clicked()
                {
                    self.state.actions.push(Action::Delete(index));
                    ui.close_menu();
                }
            });
        }
        if tone || calibration || has_presets {
            ui.separator();
        }
        let family_name = match family {
            ProcessingFamily::Classic => "Classic",
            ProcessingFamily::Merrill => "Merrill",
            ProcessingFamily::Quattro => "Quattro",
        };
        let default_title = format!("Set as default for {family_name}");
        for (title, action) in [
            ("Save preset…", Action::Save(scope)),
            (default_title.as_str(), Action::Default(scope)),
        ] {
            if ui.button(title).clicked() {
                self.state.actions.push(action);
                ui.close_menu();
            }
        }
        if scope == Scope::All
            && self.library.defaults.configured(family)
            && ui
                .button(format!("Clear defaults for {family_name}"))
                .clicked()
        {
            self.state.actions.push(Action::ClearDefaults);
            ui.close_menu();
        }
    }
}
impl State {
    pub fn start_save(&mut self, family: ProcessingFamily, scope: Scope, settings: &Settings) {
        self.save = Some(Save {
            family,
            scope,
            settings: settings.photographic_recipe(),
            name: String::new(),
            error: None,
            focused: false,
        });
    }
    pub fn dialog(&mut self, ctx: &egui::Context, library: &mut Library) {
        let Some(save) = &mut self.save else { return };
        let mut close = false;
        egui::Modal::new(egui::Id::new("save recipe preset")).show(ctx, |ui| {
            ui.heading("Save preset");
            let name = ui.text_edit_singleline(&mut save.name);
            if !save.focused {
                name.request_focus();
                save.focused = true;
            }
            if let Some(error) = &save.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !save.name.trim().is_empty(),
                        egui::Button::new(
                            if library.presets.iter().any(|p| {
                                p.family == save.family
                                    && p.scope == save.scope
                                    && p.name == save.name.trim()
                            }) {
                                "Replace"
                            } else {
                                "Save"
                            },
                        ),
                    )
                    .clicked()
                {
                    match library.save(save.family, save.scope, &save.name, &save.settings) {
                        Ok(()) => close = true,
                        Err(error) => save.error = Some(error.to_string()),
                    }
                }
                close |= ui.button("Cancel").clicked();
            });
        });
        close |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if close {
            self.save = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipping_policy_is_checked_in_the_parent_menu_for_every_family() {
        for family in [
            ProcessingFamily::Classic,
            ProcessingFamily::Merrill,
            ProcessingFamily::Quattro,
        ] {
            let ctx = egui::Context::default();
            let mut state = State::default();
            let library = Library::default();
            let mut point = egui::Pos2::ZERO;
            for (time, pressed) in [(0., None), (1., Some(true)), (1.1, Some(false))] {
                let output = ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        events: pressed
                            .map(|pressed| {
                                vec![
                                    egui::Event::PointerMoved(point),
                                    egui::Event::PointerButton {
                                        pos: point,
                                        button: egui::PointerButton::Primary,
                                        pressed,
                                        modifiers: Default::default(),
                                    },
                                ]
                            })
                            .unwrap_or_default(),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            Menus {
                                prevent_color_clipping: true,
                                exposure_auto_compensation: Default::default(),
                                family: Some(family),
                                library: &library,
                                state: &mut state,
                            }
                            .menu(ui, Scope::All);
                        });
                    },
                );
                point = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text)
                            if text.galley.job.text == "Prevent saturation clipping" =>
                        {
                            Some(text.pos + text.galley.size() * 0.5)
                        }
                        _ => None,
                    })
                    .expect("shared policy belongs in Editing");
            }
            assert!(matches!(
                state.actions.as_slice(),
                [Action::PreventSaturationClipping(false)]
            ));
        }
    }

    #[test]
    fn reset_buttons_keep_normal_and_option_camera_actions_distinct() {
        let ctx = egui::Context::default();
        for alt in [false, true] {
            let mut state = State::default();
            for pressed in [None, Some(true), Some(false)] {
                let modifiers = egui::Modifiers {
                    alt,
                    ..Default::default()
                };
                let p = egui::pos2(50., 20.);
                let _ = ctx.run(
                    egui::RawInput {
                        modifiers,
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(200., 100.),
                        )),
                        events: pressed
                            .map(|pressed| {
                                vec![
                                    egui::Event::PointerMoved(p),
                                    egui::Event::PointerButton {
                                        pos: p,
                                        button: egui::PointerButton::Primary,
                                        pressed,
                                        modifiers,
                                    },
                                ]
                            })
                            .unwrap_or_default(),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            Menus {
                                prevent_color_clipping: true,
                                exposure_auto_compensation: Default::default(),
                                family: Some(ProcessingFamily::Merrill),
                                library: &Library::default(),
                                state: &mut state,
                            }
                            .header(
                                ui,
                                Scope::All,
                                egui::Rect::from_min_size(
                                    egui::pos2(0., 10.),
                                    egui::vec2(110., 20.),
                                ),
                            );
                        });
                    },
                );
            }
            assert!(
                matches!(state.actions.as_slice(), [Action::Reset(Scope::All, camera)] if *camera == alt)
            );
        }
    }
}
