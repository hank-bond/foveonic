//! Fits browser-selected photographs into bounded comparison tiles with explicit viewport editing targets.

use crate::{
    viewport_selection::{Gesture, Request},
    App,
};
use eframe::egui;
use std::path::PathBuf;

const GAP: f32 = 8.;
const MIN_TILE: egui::Vec2 = egui::vec2(120., 112.);
const PREFERRED_TILE: egui::Vec2 = egui::vec2(320., 260.);
const CAPTION: f32 = 22.;

#[derive(Clone, Copy, Debug)]
struct Layout {
    columns: usize,
    shown: usize,
    overflow: usize,
    tile: egui::Vec2,
    size: egui::Vec2,
}
fn layout(count: usize, available: egui::Vec2, zoom: f32) -> Layout {
    let available = available.max(egui::Vec2::splat(1.));
    let max_columns = (((available.x + GAP) / (MIN_TILE.x + GAP)).floor() as usize).max(1);
    let max_rows = (((available.y + GAP) / (MIN_TILE.y + GAP)).floor() as usize).max(1);
    let capacity = max_columns.saturating_mul(max_rows);
    let slots = count.min(capacity);
    let overflow = if count > capacity {
        count - capacity + 1
    } else {
        0
    };
    let shown = slots.saturating_sub(usize::from(overflow > 0));
    let preferred = (PREFERRED_TILE * zoom.clamp(0.5, 3.)).max(MIN_TILE);
    let mut best = Layout {
        columns: 1,
        shown,
        overflow,
        tile: egui::Vec2::ZERO,
        size: egui::Vec2::ZERO,
    };
    for columns in 1..=slots.min(max_columns).max(1) {
        let rows = slots.max(1).div_ceil(columns);
        if rows > max_rows {
            continue;
        }
        let tile = egui::vec2(
            ((available.x - GAP * (columns - 1) as f32) / columns as f32).min(preferred.x),
            ((available.y - GAP * (rows - 1) as f32) / rows as f32).min(preferred.y),
        );
        if tile.x * tile.y >= best.tile.x * best.tile.y {
            best = Layout {
                columns,
                shown,
                overflow,
                tile,
                size: egui::vec2(
                    tile.x * columns as f32 + GAP * (columns - 1) as f32,
                    tile.y * rows as f32 + GAP * (rows - 1) as f32,
                ),
            };
        }
    }
    best
}

impl App {
    pub(crate) fn comparison_view(&mut self, ui: &mut egui::Ui, paths: &[PathBuf]) -> bool {
        let mut changed = false;
        let available = ui.available_size();
        if ui.rect_contains_pointer(ui.available_rect_before_wrap()) {
            self.comparison_zoom =
                (self.comparison_zoom * ui.input(|i| i.zoom_delta())).clamp(0.5, 3.);
        }
        let layout = layout(paths.len(), available, self.comparison_zoom);
        self.displayed_paths
            .extend_from_slice(&paths[..layout.shown]);
        let (canvas, _) = ui.allocate_exact_size(available, egui::Sense::hover());
        let origin = canvas.min + (canvas.size() - layout.size) * 0.5;
        let mapping = self.displayed_mapping();
        let selecting = crate::photo_tools::selects_photos(ui.ctx())
            && !ui.input(|input| input.key_down(egui::Key::Space));
        for (index, path) in paths.iter().take(layout.shown).enumerate() {
            let rect = egui::Rect::from_min_size(
                origin
                    + egui::vec2(
                        (index % layout.columns) as f32 * (layout.tile.x + GAP),
                        (index / layout.columns) as f32 * (layout.tile.y + GAP),
                    ),
                layout.tile,
            );
            let active = self.selected.as_ref() == Some(path);
            let target = self.edit_selection.contains(path);
            let image_rect = egui::Rect::from_min_max(
                rect.min + egui::Vec2::splat(3.),
                rect.max - egui::vec2(3., CAPTION),
            );
            ui.painter()
                .rect_filled(rect, 2., egui::Color32::from_gray(20));
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(image_rect)
                    .id_salt(("comparison photo content", index)),
            );
            child.set_clip_rect(image_rect.intersect(ui.clip_rect()));
            let has_active_frame = self.frame.as_ref().is_some_and(|frame| frame.path == *path);
            if active && (has_active_frame || self.multi_edit_frame(path).is_none()) {
                let temporary_pan = ui.input(|input| input.key_down(egui::Key::Space));
                let editing = crate::photo_tools::takes_pointer(ui.ctx()) && !temporary_pan;
                if selecting
                    || (editing
                        && !(self.photo_interaction_available(mapping)
                            || self.framing_preview_active()))
                {
                    child.disable();
                }
                changed |= self.show_photo(&mut child, mapping);
                self.before_badge(&child, image_rect);
            } else if let Some(frame) = self.multi_edit_frame(path) {
                let density = crate::rendering::quality::fit_density(
                    image_rect.size().into(),
                    frame.native_display_extent(),
                    ui.ctx().pixels_per_point(),
                );
                crate::viewport::paint_comparison(
                    &mut child,
                    frame.view(false).clone(),
                    frame.display_extent(false),
                    self.srgb_target,
                    index,
                    frame.diagnostic_identity(false),
                );
                self.observe_preview_density(path, density);
            } else {
                // Larger display previews belong only to the bounded editing set.
                let preview = if target {
                    self.browser.preview(path)
                } else {
                    self.browser.thumbnail(path)
                };
                if let Some(preview) = preview {
                    preview.paint(&child, image_rect);
                    let density = crate::rendering::quality::fit_density(
                        image_rect.size().into(),
                        preview.native_extent,
                        ui.ctx().pixels_per_point(),
                    );
                    self.observe_preview_density(path, density);
                }
            }
            let response = ui.interact(
                rect,
                egui::Id::new(("comparison photo", path)),
                if selecting {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            let response = response.on_hover_text(if selecting {
                "Click to edit this photo. Option-click toggles editing. Shift-click selects a range up to the editing limit."
            } else { "Photo tools preserve the viewport editing selection. Press V to select photographs." });
            if crate::timing::enabled()
                && ui.input(|input| {
                    input.pointer.button_released(egui::PointerButton::Primary)
                        && input
                            .pointer
                            .latest_pos()
                            .is_some_and(|pos| rect.contains(pos))
                })
            {
                crate::timing::emit("viewport.tile.release", || {
                    serde_json::json!({
                    "source":path,"photo":foveonic::diagnostics::photo_id(path),
                    "clicked":response.clicked(),"hovered":response.hovered(),
                    "selecting":selecting,"active":active,"widget":response.id.value(),
                    "rect":[rect.min.x,rect.min.y,rect.max.x,rect.max.y]})
                });
            }
            if selecting && response.clicked() {
                let modifiers = ui.input(|input| input.modifiers);
                self.comparison_selection = Some(Request {
                    path: path.clone(),
                    gesture: if modifiers.shift {
                        Gesture::Range
                    } else if modifiers.alt {
                        Gesture::Toggle
                    } else {
                        Gesture::Replace
                    },
                });
            }
            ui.painter().rect_stroke(
                rect,
                2.,
                egui::Stroke::new(
                    if active { 3_f32 } else { 1_f32 },
                    if active {
                        egui::Color32::LIGHT_GRAY
                    } else {
                        egui::Color32::GRAY
                    },
                ),
                egui::StrokeKind::Inside,
            );
            if target {
                ui.painter().rect_stroke(
                    rect.shrink(4.),
                    1.,
                    egui::Stroke::new(2_f32, ui.visuals().selection.stroke.color),
                    egui::StrokeKind::Inside,
                );
            }
            ui.painter().with_clip_rect(rect).text(
                egui::pos2(rect.center().x, rect.bottom() - 11.),
                egui::Align2::CENTER_CENTER,
                path.file_stem().unwrap_or_default().to_string_lossy(),
                egui::TextStyle::Body.resolve(ui.style()),
                egui::Color32::LIGHT_GRAY,
            );
        }
        if layout.overflow > 0 {
            let index = layout.shown;
            let rect = egui::Rect::from_min_size(
                origin
                    + egui::vec2(
                        (index % layout.columns) as f32 * (layout.tile.x + GAP),
                        (index / layout.columns) as f32 * (layout.tile.y + GAP),
                    ),
                layout.tile,
            );
            ui.painter()
                .rect_filled(rect, 2., egui::Color32::from_gray(24));
            let hidden_targets = self
                .edit_selection
                .paths()
                .iter()
                .filter(|path| !paths[..layout.shown].contains(path))
                .count();
            ui.painter().text(
                rect.center() - egui::vec2(0., 10.),
                egui::Align2::CENTER_CENTER,
                format!("{} more", layout.overflow),
                egui::TextStyle::Heading.resolve(ui.style()),
                egui::Color32::LIGHT_GRAY,
            );
            if hidden_targets > 0 {
                ui.painter().text(
                    rect.center() + egui::vec2(0., 14.),
                    egui::Align2::CENTER_CENTER,
                    format!(
                        "{hidden_targets} editing {}",
                        if hidden_targets == 1 {
                            "target"
                        } else {
                            "targets"
                        }
                    ),
                    egui::TextStyle::Body.resolve(ui.style()),
                    ui.visuals().selection.stroke.color,
                );
            }
            ui.interact(
                rect,
                ui.id().with("comparison overflow"),
                egui::Sense::hover(),
            )
            .on_hover_text("Use the browser to choose which photographs appear here.");
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tiles_shrink_to_readable_minimum_before_showing_overflow() {
        let available = egui::vec2(800., 600.);
        let small = layout(2, available, 1.);
        let medium = layout(12, available, 1.);
        let large = layout(100_000, available, 1.);
        assert_eq!(small.overflow, 0);
        assert_eq!(medium.overflow, 0);
        assert!(small.tile.x * small.tile.y >= medium.tile.x * medium.tile.y);
        assert!(large.tile.x >= MIN_TILE.x && large.tile.y >= MIN_TILE.y);
        assert_eq!(large.shown + large.overflow, 100_000);
        assert!(large.shown < 40);
        for plan in [small, medium, large] {
            assert!(plan.size.x <= available.x && plan.size.y <= available.y);
        }
    }
    #[test]
    fn narrow_viewports_stay_finite_and_include_an_overflow_slot() {
        for available in [
            egui::vec2(1., 1.),
            egui::vec2(100., 90.),
            egui::vec2(1280., 120.),
        ] {
            let plan = layout(usize::MAX, available, 1.);
            assert!(plan.tile.x.is_finite() && plan.tile.y.is_finite());
            assert_eq!(plan.shown + plan.overflow, usize::MAX);
            assert!(plan.size.x <= available.x && plan.size.y <= available.y);
        }
    }
}
