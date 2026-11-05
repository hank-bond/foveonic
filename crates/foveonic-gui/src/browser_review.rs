//! Handles browser highlighting, import checkboxes, and shared hidden-photo visibility.

use super::{Browser, Mark};
use crate::widgets;
use eframe::egui;
use std::path::{Path, PathBuf};

pub(super) const MARK_GUTTER: f32 = 24.;
const MARK_SIDE: f32 = 16.;

impl Browser {
    pub(super) fn file_menu(&mut self, response: &egui::Response, path: &Path) {
        response.context_menu(|ui| {
            if self.import_mode {
                let label = match self.mark(path) {
                    Mark::Pick => "Uncheck for import",
                    Mark::Unmarked => "Check for import",
                    Mark::Hidden | Mark::Imported => "Restore for review",
                };
                if ui.button(label).clicked() {
                    self.toggle_mark(path);
                    self.review_clicked = true;
                    ui.close_menu();
                }
                ui.separator();
            }
            let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
            if ui.button("Copy file path").clicked() {
                ui.ctx().copy_text(path.to_string_lossy().into_owned());
                ui.close_menu();
            }
            #[cfg(target_os = "macos")]
            if ui.button("Show in Finder").clicked() {
                std::thread::spawn(move || {
                    match std::process::Command::new("open")
                        .arg("-R")
                        .arg(&path)
                        .status()
                    {
                        Ok(status) if status.success() => (),
                        result => {
                            eprintln!("Cannot reveal {} in Finder: {result:?}", path.display())
                        }
                    }
                });
                ui.close_menu();
            }
        });
    }
    /// Export highlights in browser order, with the viewed photo as an empty-selection fallback.
    pub fn processing_selection(&self, active: Option<&Path>) -> Vec<PathBuf> {
        // A full-folder selection is already in the required order in `files`.
        if !self.files.is_empty() && self.highlighted.len() == self.files.len() {
            let paths: Vec<_> = self
                .files
                .iter()
                .filter(|path| self.highlighted.contains(*path) && !self.excluded.contains(*path))
                .cloned()
                .collect();
            if !paths.is_empty() {
                return paths;
            }
        }
        let mut paths: Vec<_> = self
            .highlighted
            .iter()
            .filter_map(|path| {
                self.file_order
                    .get(path)
                    .filter(|_| !self.excluded.contains(path))
                    .map(|index| (*index, path))
            })
            .collect();
        paths.sort_unstable_by_key(|(index, _)| *index);
        if paths.is_empty() {
            active
                .filter(|path| {
                    self.file_order.contains_key(*path) && !self.excluded.contains(*path)
                })
                .map(Path::to_owned)
                .into_iter()
                .collect()
        } else {
            paths.into_iter().map(|(_, path)| path.clone()).collect()
        }
    }

    pub fn review_counts(&self) -> (usize, usize) {
        self.review_counts
    }

    pub fn refresh_filter(&mut self) {
        let mut counts = (0, 0);
        let excluded = self
            .files
            .iter()
            .filter(|path| {
                let mark = self.mark(path);
                counts.0 += usize::from(mark == Mark::Pick);
                counts.1 += usize::from(mark.hidden());
                (self.import_mode && self.hide_reviewed && mark.hidden())
                    || !self
                        .filter
                        .matches(path, self.root.as_deref(), self.records.get(*path))
            })
            .cloned()
            .collect();
        self.review_counts = counts;
        if self.excluded != excluded {
            self.excluded = excluded;
            self.invalidate_organizer();
            self.invalidate_prefetch();
        }
        self.highlighted
            .retain(|path| !self.excluded.contains(path));
    }

    pub fn visibility_button(&mut self, ui: &mut egui::Ui, focus: egui::Id) -> egui::Response {
        let side = widgets::ControlSize::Medium.side();
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
        crate::widgets::eye_icon(
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
            crate::widgets::eye_icon(ui, rect, visuals.fg_stroke.color, false);
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
        self.refresh_filter();
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
