//! Handles browser highlighting, import checkboxes, and shared hidden-photo visibility.

use super::{Browser, Mark};
use crate::{preferences::Organizer, toolbar, widgets};
use eframe::egui;
use std::path::{Path, PathBuf};

const MARK_GUTTER: f32 = 24.;
const MARK_SIDE: f32 = 16.;
const EYE_STROKE: f32 = 1.3;

pub(super) enum Shortcut {
    SelectAll,
    TogglePick,
    Hide,
    Arrow(egui::Key, egui::Modifiers),
}

impl Browser {
    /// Route workspace commands before egui interprets Space as a button click
    /// or arrows as focus movement. Focused text editors keep the original input.
    pub fn capture_shortcuts(
        &mut self,
        ctx: &egui::Context,
        input: &mut egui::RawInput,
        enabled: bool,
    ) {
        self.shortcuts.clear();
        if !enabled || self.files.is_empty() || !input.focused {
            return;
        }
        let (focused, popup) = ctx.memory(|memory| {
            (
                memory.focused(),
                memory.any_popup_open() || memory.top_modal_layer().is_some(),
            )
        });
        // DragValue uses TextEdit with the same ID while its value is focused.
        let typing = focused.is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        if typing || popup {
            return;
        }
        input.events.retain(|event| {
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                return true;
            };
            let shortcut = match *key {
                egui::Key::A if modifiers.matches_exact(egui::Modifiers::COMMAND) => {
                    Some(Shortcut::SelectAll)
                }
                egui::Key::Space if self.import_mode && modifiers.is_none() => {
                    Some(Shortcut::TogglePick)
                }
                egui::Key::Delete | egui::Key::Backspace
                    if self.import_mode && modifiers.is_none() =>
                {
                    Some(Shortcut::Hide)
                }
                egui::Key::ArrowLeft
                | egui::Key::ArrowRight
                | egui::Key::ArrowUp
                | egui::Key::ArrowDown
                    if modifiers.is_none() || modifiers.shift_only() =>
                {
                    Some(Shortcut::Arrow(*key, *modifiers))
                }
                _ => None,
            };
            if let Some(shortcut) = shortcut {
                self.shortcuts.push(shortcut);
                false
            } else {
                true
            }
        });
    }

    pub fn review_counts(&self) -> (usize, usize) {
        self.files.iter().fold((0, 0), |(picked, hidden), path| {
            let mark = self.mark(path);
            (
                picked + usize::from(mark == Mark::Pick),
                hidden + usize::from(mark.hidden()),
            )
        })
    }

    pub fn header(
        &mut self,
        ui: &mut egui::Ui,
        organizer: &mut Organizer,
        enabled: bool,
        focus: egui::Id,
    ) {
        let files = format!("{} files", self.files.len());
        if !self.import_mode {
            widgets::control_row(ui, |ui| Self::view_controls(ui, organizer, &files));
            return;
        }
        let (picked, hidden) = self.review_counts();
        let summary = format!("{picked} picked, {hidden} hidden");
        let text_width = |label: &str, style| {
            egui::WidgetText::from(label)
                .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, style)
                .size()
                .x
        };
        let gap = ui.spacing().item_spacing.x;
        let views_width = text_width(&files, egui::TextStyle::Body)
            + ["Icons", "List"]
                .iter()
                .map(|label| {
                    text_width(label, egui::TextStyle::Button) + 2. * widgets::button_padding(ui).x
                })
                .sum::<f32>()
            + 2. * gap;
        let review_width = widgets::control_height(ui)
            + gap
            + text_width(&summary, egui::TextStyle::Body)
            + 2. * f32::from(widgets::BUTTON_GROUP_MARGIN);
        let divider_width = text_width("|", egui::TextStyle::Body);
        ui.add_space(ui.spacing().item_spacing.y);
        if views_width + review_width + divider_width + 2. * gap <= ui.available_width() {
            toolbar::row(ui, |ui| {
                Self::view_controls(ui, organizer, &files);
                ui.weak("|");
                self.review_summary(ui, &summary, enabled, focus);
            });
        } else {
            widgets::centered_control_row(ui, |ui| Self::view_controls(ui, organizer, &files));
            self.review_summary(ui, &summary, enabled, focus);
        }
    }

    fn view_controls(ui: &mut egui::Ui, organizer: &mut Organizer, files: &str) {
        ui.label(files);
        widgets::selectable_value(ui, organizer, Organizer::Icons, "Icons");
        widgets::selectable_value(ui, organizer, Organizer::List, "List");
    }

    fn review_summary(&mut self, ui: &mut egui::Ui, summary: &str, enabled: bool, focus: egui::Id) {
        widgets::button_group(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    self.visibility_button(ui, focus);
                });
                ui.label(summary);
            });
        });
    }

    pub fn refresh_filter(&mut self) {
        let excluded = self
            .files
            .iter()
            .filter(|path| self.import_mode && self.hide_reviewed && self.mark(path).hidden())
            .cloned()
            .collect();
        if self.excluded != excluded {
            self.excluded = excluded;
            self.invalidate_prefetch();
        }
        self.highlighted
            .retain(|path| !self.excluded.contains(path));
    }

    pub fn visibility_button(&mut self, ui: &mut egui::Ui, focus: egui::Id) -> egui::Response {
        let side = widgets::control_height(ui);
        let (rect, response) =
            ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::click());
        if response.clicked() {
            self.hide_reviewed = !self.hide_reviewed;
            self.advance_hidden = self.hide_reviewed;
            self.refresh_filter();
            self.focus_requested = true;
            ui.memory_mut(|memory| memory.request_focus(focus));
        }
        widgets::paint(ui, &response, self.hide_reviewed);
        let color = widgets::visuals(ui, &response, self.hide_reviewed)
            .fg_stroke
            .color;
        eye(
            ui,
            egui::Rect::from_center_size(rect.center(), egui::vec2(MARK_SIDE, MARK_SIDE)),
            color,
            self.hide_reviewed,
        );
        response.on_hover_text(if self.hide_reviewed {
            "Imported and manually hidden photos are concealed. Click to show them."
        } else {
            "Showing imported and manually hidden photos. Click to conceal them."
        })
    }

    pub(super) fn review_control(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        caption: egui::Rect,
    ) -> egui::Rect {
        if !self.import_mode {
            return caption;
        }
        let hit = egui::Rect::from_min_max(
            caption.min,
            egui::pos2(caption.left() + MARK_GUTTER, caption.bottom()),
        );
        let response = ui.interact(
            hit,
            ui.make_persistent_id(("import mark", path)),
            egui::Sense::CLICK,
        );
        if response.clicked() {
            self.toggle_mark(path);
            self.review_clicked = true;
        }
        let mark = self.mark(path);
        let rect = egui::Rect::from_center_size(hit.center(), egui::Vec2::splat(MARK_SIDE));
        let visuals = widgets::visuals(ui, &response, false);
        if mark.hidden() {
            eye(ui, rect, visuals.fg_stroke.color, false);
        } else {
            ui.painter().rect(
                rect,
                visuals.corner_radius,
                visuals.bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
            if mark == Mark::Pick {
                let point = |x, y| rect.min + egui::vec2(x * rect.width(), y * rect.height());
                ui.painter().add(egui::Shape::line(
                    vec![point(0.22, 0.51), point(0.43, 0.72), point(0.79, 0.27)],
                    visuals.fg_stroke,
                ));
            }
        }
        response.on_hover_text(match mark {
            Mark::Imported => "Already imported. Click to make eligible again, unchecked. Import history is retained.",
            Mark::Hidden => "Hidden. Click to restore unchecked. Process saves this change.",
            Mark::Pick => "Checked for import. Click to uncheck.",
            Mark::Unmarked => "Check for import. Space toggles the highlighted photos.",
        });
        egui::Rect::from_min_max(egui::pos2(hit.right(), caption.top()), caption.max)
    }

    pub(super) fn toggle_mark(&mut self, path: &Path) {
        let next = if self.mark(path) == Mark::Unmarked {
            Mark::Pick
        } else {
            Mark::Unmarked
        };
        self.marks.insert(path.to_owned(), next);
        self.refresh_filter();
    }

    pub(super) fn highlight(&mut self, path: &Path, files: &[PathBuf], modifiers: egui::Modifiers) {
        if modifiers.shift {
            let anchor = self
                .anchor
                .as_ref()
                .and_then(|anchor| files.iter().position(|p| p == anchor));
            let end = files.iter().position(|p| p == path);
            if let (Some(start), Some(end)) = (anchor, end) {
                if !modifiers.command {
                    self.highlighted.clear();
                }
                self.highlighted
                    .extend(files[start.min(end)..=start.max(end)].iter().cloned());
                return;
            }
        }
        if modifiers.command {
            if !self.highlighted.remove(path) {
                self.highlighted.insert(path.to_owned());
            }
        } else {
            self.highlighted.clear();
            self.highlighted.insert(path.to_owned());
        }
        self.anchor = Some(path.to_owned());
    }

    pub(super) fn toggle_highlighted(&mut self, files: &[PathBuf]) {
        let eligible: Vec<_> = files
            .iter()
            .filter(|path| self.highlighted.contains(*path) && !self.mark(path).hidden())
            .collect();
        let check = eligible.iter().any(|path| self.mark(path) != Mark::Pick);
        for path in eligible {
            self.marks.insert(
                path.clone(),
                if check { Mark::Pick } else { Mark::Unmarked },
            );
        }
    }

    pub(super) fn hide_highlighted(&mut self, files: &[PathBuf]) {
        let targets: Vec<_> = files
            .iter()
            .filter(|path| self.highlighted.contains(*path))
            .cloned()
            .collect();
        if targets.is_empty() {
            return;
        }
        for path in targets {
            self.marks.insert(path, Mark::Hidden);
        }
        self.hide_reviewed = true;
        self.advance_hidden = true;
        self.refresh_filter();
    }

    pub(super) fn next_visible(&self, active: &Path) -> Option<PathBuf> {
        let index = self.files.iter().position(|path| path == active)?;
        self.files[index + 1..]
            .iter()
            .find(|path| !self.excluded.contains(*path))
            .or_else(|| {
                self.files[..index]
                    .iter()
                    .rev()
                    .find(|path| !self.excluded.contains(*path))
            })
            .cloned()
    }
}

// Normalized outline and pupil coordinates scale inside the caption icon.
fn eye(ui: &egui::Ui, rect: egui::Rect, color: egui::Color32, crossed: bool) {
    let point = |x, y| rect.min + egui::vec2(x * rect.width(), y * rect.height());
    let stroke = egui::Stroke::new(EYE_STROKE, color);
    for height in [0.04, 0.96] {
        ui.painter().add(egui::Shape::CubicBezier(
            egui::epaint::CubicBezierShape::from_points_stroke(
                [
                    point(0., 0.5),
                    point(0.28, height),
                    point(0.72, height),
                    point(1., 0.5),
                ],
                false,
                egui::Color32::TRANSPARENT,
                stroke,
            ),
        ));
    }
    ui.painter()
        .circle_stroke(rect.center(), rect.height() * 0.15, stroke);
    if crossed {
        ui.painter()
            .line_segment([point(0., 1.), point(1., 0.)], stroke);
    }
}
