//! Fits browser-selected photographs into bounded comparison tiles with explicit viewport editing targets.

use crate::{
    viewport_selection::{Gesture, Request},
    App,
};
use eframe::egui;
use std::path::PathBuf;

mod layout;
pub(crate) use layout::State;
use layout::CAPTION;

impl App {
    pub(crate) fn comparison_view(&mut self, ui: &mut egui::Ui, paths: &[PathBuf]) -> bool {
        let mut changed = false;
        let available = ui.available_size();
        let (shown, _) = layout::capacity(paths.len(), available);
        let aspects: Vec<_> = paths[..shown]
            .iter()
            .map(|path| {
                let frame = self
                    .frame
                    .as_ref()
                    .filter(|frame| frame.path == *path)
                    .or_else(|| self.multi_edit.frame(path));
                frame
                    .map(|frame| {
                        let settings = if self.selected.as_ref() == Some(path) {
                            &self.settings
                        } else {
                            self.multi_edit.settings(path).unwrap_or(&frame.requested)
                        };
                        // Camera/Defaults and framing previews share the camera's
                        // dimensions but must use this photo's authored composition.
                        composition_aspect(frame.camera_extent(), settings)
                    })
                    .unwrap_or_else(|| {
                        let extent = self.browser.known_extent(path).unwrap_or([3, 2]);
                        extent[0] as f32 / extent[1].max(1) as f32
                    })
            })
            .collect();
        // Keep the release frame on the gesture's geometry so its click cannot
        // land on another photograph when dimensions arrive during a drag.
        let freeze = ui.input(|input| input.pointer.any_down() || input.pointer.any_released());
        let layout = self
            .comparison_layout
            .get(paths, &aspects, paths.len(), available, freeze)
            .clone();
        self.displayed_paths.extend(
            layout
                .tiles
                .iter()
                .filter(|tile| tile.index < layout.shown)
                .map(|tile| paths[tile.index].clone()),
        );
        let (canvas, _) = ui.allocate_exact_size(available, egui::Sense::hover());
        let mapping = self.displayed_mapping();
        let selecting = crate::photo_tools::selects_photos(ui.ctx())
            && !ui.input(|input| input.key_down(egui::Key::Space));
        for tile in layout.tiles.iter().filter(|tile| tile.index < layout.shown) {
            let path = &paths[tile.index];
            let rect = tile.rect.translate(canvas.min.to_vec2());
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
                    .id_salt(("comparison photo content", path)),
            );
            child.set_clip_rect(image_rect.intersect(ui.clip_rect()));
            let has_active_frame = self.frame.as_ref().is_some_and(|frame| frame.path == *path);
            if active && (has_active_frame || self.multi_edit_frame(path).is_none()) {
                let temporary_pan = ui.input(|input| input.key_down(egui::Key::Space));
                let editing = crate::photo_tools::takes_pointer(ui.ctx()) && !temporary_pan;
                if editing
                    && !(self.photo_interaction_available(mapping) || self.framing_preview_active())
                {
                    child.disable();
                }
                changed |= self.show_photo(&mut child, mapping);
                self.before_badge(&child, image_rect);
            } else {
                let view = self.viewports.view_mut(Some(path));
                view.editing = crate::photo_tools::takes_pointer(ui.ctx());
                view.framing = None;
                view.zoom_snap = self.preferences.zoom_snap;
                let native_extent = if let Some(frame) = self.multi_edit.frame(path) {
                    view.show(
                        &mut child,
                        frame.view(false).clone(),
                        frame.display_extent(false),
                        None,
                        crate::viewport::Mapping::normal(-1),
                        self.srgb_target,
                        frame.diagnostic_identity(false),
                    );
                    Some(frame.native_display_extent())
                } else {
                    let preview = if target {
                        self.browser.preview(path)
                    } else {
                        self.browser.thumbnail(path)
                    };
                    if let Some(preview) = preview {
                        // Browser previews already include their prepared composition.
                        view.show_preview(
                            &mut child,
                            &preview.texture,
                            preview.native_extent,
                            Default::default(),
                        );
                        Some(preview.native_extent)
                    } else {
                        view.show_empty(&mut child);
                        None
                    }
                };
                if let Some((extent, (_, image))) = native_extent.zip(view.image_rectangles()) {
                    let density = crate::rendering::quality::display_density(
                        [image.width(), image.height()],
                        extent,
                        ui.ctx().pixels_per_point(),
                    );
                    self.observe_preview_density(path, density);
                }
            }
            // The image owns its gestures. Only the non-overlapping caption
            // needs a separate selection response.
            let caption = ui.interact(
                egui::Rect::from_min_max(egui::pos2(rect.left(), image_rect.bottom()), rect.max),
                egui::Id::new(("comparison photo caption", path)),
                if selecting {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            let response = self
                .viewports
                .view(Some(path))
                .response()
                .unwrap_or_else(|| caption.clone());
            let clicked = response.clicked() || caption.clicked();
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
                    "clicked":clicked,"hovered":response.hovered(),
                    "selecting":selecting,"active":active,"widget":response.id.value(),
                    "rect":[rect.min.x,rect.min.y,rect.max.x,rect.max.y]})
                });
            }
            if selecting && clicked {
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
            let rect = layout
                .tiles
                .iter()
                .find(|tile| tile.index == layout.shown)
                .unwrap()
                .rect
                .translate(canvas.min.to_vec2());
            ui.painter()
                .rect_filled(rect, 2., egui::Color32::from_gray(24));
            let hidden_targets = self
                .edit_selection
                .paths()
                .iter()
                .filter(|path| !self.displayed_paths.contains(path))
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

fn composition_aspect(camera_extent: [u32; 2], settings: &foveonic::render::Settings) -> f32 {
    let extent = settings.rotation.extent(camera_extent);
    let [left, top, right, bottom] = settings.crop.pixels(extent);
    (right - left) as f32 / (bottom - top).max(1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_composition_aspect_uses_camera_dimensions_crop_and_rotation() {
        let mut authored = foveonic::render::Settings::default();
        authored.rotation.turns = 1;
        authored.crop.bounds = [0., 0., 0.5, 1.];
        let current = composition_aspect([6000, 4000], &authored);
        assert!((current - 1. / 3.).abs() < 1e-6);
        assert_eq!(composition_aspect([6000, 4000], &Default::default()), 1.5);
    }
}
