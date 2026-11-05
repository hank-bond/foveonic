//! Draws the cap calibration modal using shared photographic readouts and controller actions.

use super::{Slot, State};
use crate::widgets;
use eframe::egui;
use foveonic::noise_calibration as calibration;

const MODAL_WIDTH: f32 = 560.;
const MODAL_PADDING_SCALE: f32 = 2.;
const PROFILE_GROUP_PADDING: i8 = 12;
const TABLE_HEIGHT: f32 = 340.;
const TABLE_SPACING: [f32; 2] = [16., 8.];

impl State {
    fn profile_picker(&mut self, ui: &mut egui::Ui, slot: &mut Slot) {
        ui.horizontal(|ui| {
            ui.label("Active profile");
            egui::ComboBox::from_id_salt("noise calibration slot")
                .selected_text(slot.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(slot, Slot::Provided, "Provided");
                    ui.add_enabled_ui(self.user.is_some(), |ui| {
                        ui.selectable_value(slot, Slot::User, "User");
                    });
                });
            if let Some(user) = &self.user {
                ui.weak(format!("User: {} captures", user.captures().len()));
            }
        });
        profile_description(
            ui,
            "Provided:",
            " bundled correction weights for Multiscale noise reduction and Directional banding.",
        );
        profile_description(
            ui,
            "User:",
            " weights learned from your cap shots, plus camera-specific hot-pixel maps.",
        );
        ui.label(
            "User weights may better match your cameras. Untrained ISOs use Provided weights.",
        );
        ui.label("With Speckles enabled, hot-pixel maps add correction only for matching camera serials.");
    }

    fn file_buttons(&mut self, ui: &mut egui::Ui) {
        let busy = self.job.is_some();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Add files…"))
                .clicked()
            {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Merrill X3F", &["x3f", "X3F"])
                    .pick_files()
                {
                    self.import(paths);
                }
            }
            if ui
                .add_enabled(!busy, egui::Button::new("Add directory…"))
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.import(vec![path]);
                }
            }
            if ui
                .add_enabled(
                    !busy && !self.dataset.captures.is_empty(),
                    egui::Button::new("Clear list"),
                )
                .clicked()
            {
                self.dataset.clear();
                self.dataset_changed();
                self.save_dataset();
            }
        });
    }

    fn capture_table(&mut self, ui: &mut egui::Ui) -> bool {
        let grouped = calibration::groups(&self.dataset.captures);
        let mut remove = None;
        let busy = self.job.is_some();
        egui::ScrollArea::vertical()
            .id_salt("noise cap groups")
            .max_height(TABLE_HEIGHT)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("noise calibration table")
                    .striped(true)
                    .min_col_width(0.)
                    .spacing(TABLE_SPACING)
                    .show(ui, |ui| {
                        for title in ["", "Camera", "ISO", "Shutter", "Count", ""] {
                            ui.strong(title);
                        }
                        ui.end_row();
                        for (condition, paths) in &grouped {
                            let rows = paths
                                .iter()
                                .find_map(|p| self.dataset.properties.get(p))
                                .map(crate::metadata::property_rows)
                                .unwrap_or_default();
                            let readout = |key| {
                                rows.iter()
                                    .find(|(label, _)| *label == key)
                                    .map(|(_, value)| value.as_str())
                            };
                            let missing = paths
                                .iter()
                                .filter(|p| self.availability.missing.contains(*p))
                                .count();
                            let ready = self.trained || self.completed.contains(condition);
                            let (rect, response) =
                                ui.allocate_exact_size(egui::vec2(12., 24.), egui::Sense::hover());
                            let color = if missing > 0 {
                                ui.visuals().error_fg_color
                            } else if ready {
                                egui::Color32::from_rgb(112, 167, 124)
                            } else {
                                egui::Color32::from_rgb(220, 181, 72)
                            };
                            ui.painter().circle_filled(rect.center(), 4., color);
                            response.on_hover_text(if missing > 0 {
                                format!(
                                    "{missing} of {} source files are unavailable. Restore the files or remove this group to retrain.",
                                    paths.len()
                                )
                            } else if ready {
                                "Training measurements complete".into()
                            } else {
                                "Not yet trained".into()
                            });
                            ui.vertical(|ui| {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                                ui.label(readout("Camera").unwrap_or(&condition.camera_model));
                                ui.weak(format!(
                                    "Serial: {}",
                                    condition
                                        .serial
                                        .as_deref()
                                        .unwrap_or("unavailable · no fixed-pixel map")
                                ));
                            });
                            ui.label(readout("ISO").unwrap_or("—"));
                            ui.label(readout("Shutter").unwrap_or("—"));
                            ui.label(paths.len().to_string()).on_hover_text(
                                if paths.len() < calibration::MINIMUM_CAPTURES {
                                    "Add at least seven captures"
                                } else {
                                    "Fourteen captures are recommended"
                                },
                            );
                            if ui
                                .add_enabled(!busy, egui::Button::new("×"))
                                .on_hover_text("Remove this group from the list")
                                .clicked()
                            {
                                remove = Some(condition.clone());
                            }
                            ui.end_row();
                        }
                    });
                if grouped.is_empty() {
                    ui.weak("No cap captures added.");
                }
            });
        if let Some(condition) = remove {
            self.dataset.remove(&condition);
            self.dataset_changed();
            self.save_dataset();
        }
        self.can_train()
    }

    fn job_status(&self, ui: &mut egui::Ui) {
        let height = widgets::control_height(ui);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                if let Some(error) = &self.error {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(error).color(egui::Color32::LIGHT_RED),
                        )
                        .truncate(),
                    )
                    .on_hover_text(error);
                } else if self.is_importing() {
                    ui.spinner();
                    ui.label("Reading metadata…");
                } else if let Some(p) = &self.progress {
                    ui.add(
                        egui::ProgressBar::new(p.completed as f32 / p.total.max(1) as f32)
                            .desired_height(height)
                            .text(format!(
                                "{} · ISO {} · {} / {}",
                                p.condition.camera_model, p.condition.iso, p.completed, p.total
                            )),
                    );
                } else if self.trained {
                    ui.label("User profile saved. All listed groups are trained.");
                }
            },
        );
    }

    pub fn show(&mut self, ctx: &egui::Context, slot: &mut Slot) {
        if !self.open {
            return;
        }
        self.availability
            .refresh(&self.dataset.captures, &self.cache, ctx);
        let previous = *slot;
        let mut frame = egui::Frame::popup(&ctx.style());
        frame.inner_margin *= MODAL_PADDING_SCALE;
        let response = egui::Modal::new(egui::Id::new("noise calibration"))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width(MODAL_WIDTH);
                ui.spacing_mut().button_padding = widgets::button_padding(ui);
                ui.spacing_mut().interact_size.y = widgets::control_height(ui);
                ui.heading("Noise calibration");
                ui.label(
                "Add lens-cap X3Fs: at least 7 per camera/ISO/shutter combination, 14 recommended.",
            );
                egui::Frame::group(ui.style())
                    .fill(ui.visuals().widgets.inactive.bg_fill)
                    .inner_margin(PROFILE_GROUP_PADDING)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        self.profile_picker(ui, slot);
                    });
                ui.separator();
                self.file_buttons(ui);
                let can_train = self.capture_table(ui);
                self.job_status(ui);
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let close = ui
                        .button("Close")
                        .on_hover_text("Training continues when this dialog is closed.")
                        .clicked();
                    if self.is_training() && ui.button("Cancel training").clicked() {
                        self.cancel_training();
                    }
                    if ui
                        .add_enabled(can_train, egui::Button::new("Train"))
                        .clicked()
                    {
                        self.train();
                    }
                    close
                })
                .inner
            });
        if previous != *slot {
            self.changed = true;
        }
        if response.inner || response.should_close() {
            self.open = false;
        }
    }
}

fn profile_description(ui: &mut egui::Ui, label: &str, description: &str) {
    let mut text = egui::text::LayoutJob::default();
    egui::RichText::new(label).strong().append_to(
        &mut text,
        ui.style(),
        egui::FontSelection::Default,
        egui::Align::LEFT,
    );
    egui::RichText::new(description).append_to(
        &mut text,
        ui.style(),
        egui::FontSelection::Default,
        egui::Align::LEFT,
    );
    ui.label(text);
}
