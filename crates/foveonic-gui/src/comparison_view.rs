//! Fits browser-selected photographs into bounded comparison tiles with explicit viewport editing targets.

use crate::{viewport_selection::Request, App};
use eframe::egui;
use std::path::PathBuf;

mod layout;
pub(crate) use layout::State;
use layout::CAPTION;

/// The shape a tile takes while its photo's size is still being read.
const PLACEHOLDER_ASPECT: [u32; 2] = [3, 2];

impl App {
    pub(crate) fn comparison_view(&mut self, ui: &mut egui::Ui, paths: &[PathBuf]) -> bool {
        let mut changed = false;
        let available = ui.available_size();
        let (shown, _) = layout::capacity(paths.len(), available);
        let aspects: Vec<_> = paths[..shown]
            .iter()
            .map(|path| {
                let frame = self.photo_layers(path)[0];
                let settings = self
                    .multi_edit
                    .settings(path)
                    .or(frame.map(|frame| &frame.request.settings));
                // The photo's recorded size gives its shape before any image of
                // it, with its authored composition.
                let extent = self
                    .browser
                    .metadata_summary(path)
                    .and_then(|summary| summary.extent)
                    .unwrap_or(PLACEHOLDER_ASPECT);
                settings.map_or(extent[0] as f32 / extent[1].max(1) as f32, |settings| {
                    composition_aspect(extent, settings)
                })
            })
            .collect();
        // Keep the release frame on the gesture's geometry so its click cannot
        // land on another photograph when dimensions arrive during a drag.
        let freeze = layout::pointer_gesture(
            ui.ctx(),
            paths
                .iter()
                .take(shown.max(self.comparison_layout.shown()))
                .flat_map(|path| {
                    [
                        self.viewports.view(Some(path)).id(),
                        egui::Id::new(("comparison photo caption", path)),
                    ]
                }),
        );
        // Crop and Rotate change photographs' shapes one after another. The
        // arrangement keeps the shapes it was made for until the tool is put
        // down, so no photograph moves under the pointer while they are being
        // framed. It still follows the viewport's size.
        let held = crate::photo_tools::framing(ui.ctx())
            .then(|| self.comparison_layout.held_aspects(paths))
            .flatten();
        let aspects = held.unwrap_or(aspects);
        let layout = self
            .comparison_layout
            .get(
                paths,
                &aspects,
                paths.len(),
                available,
                self.preferences.equal_viewport_grid,
                freeze,
            )
            .clone();
        self.viewport_selection.extend(
            layout
                .tiles
                .iter()
                .filter(|tile| tile.index < layout.shown)
                .map(|tile| paths[tile.index].clone()),
        );
        let (canvas, _) = ui.allocate_exact_size(available, egui::Sense::hover());
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
            changed |= self.show_photo(path, &mut child);
            self.before_badge(path, &child, image_rect);
            // The image owns its gestures. Only the non-overlapping caption
            // needs a separate selection response.
            let caption = ui.interact(
                egui::Rect::from_min_max(egui::pos2(rect.left(), image_rect.bottom()), rect.max),
                egui::Id::new(("comparison photo caption", path)),
                if selecting {
                    egui::Sense::click_and_drag()
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
                "Click to edit this photo. Drag to pan without changing editing selection. Option-click toggles editing. Shift links viewing on blue photos. Shift-click a white photo or caption to select a range."
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
            if let Some(gesture) = crate::photo_tools::selection_gesture(
                &response,
                self.viewports
                    .view(Some(path))
                    .selection_eligible(selecting),
            )
            .or_else(|| crate::photo_tools::selection_gesture(&caption, selecting))
            {
                self.comparison_selection = Some(Request {
                    path: path.clone(),
                    gesture,
                });
                // Selection applies before the next frame draws.
                ui.ctx().request_repaint();
            }
            ui.painter().rect_stroke(
                rect,
                2.,
                egui::Stroke::new(
                    if target { 3_f32 } else { 1_f32 },
                    if target {
                        ui.visuals().selection.stroke.color
                    } else {
                        egui::Color32::WHITE
                    },
                ),
                egui::StrokeKind::Inside,
            );
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
                .filter(|path| !self.viewport_selection.contains(path))
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
        if self.viewports.apply_group_input(
            self.edit_selection.paths(),
            &self.viewport_selection,
            ui.ctx().pixels_per_point(),
        ) {
            ui.ctx().request_repaint();
        }
        changed
    }
}

fn composition_aspect(camera_extent: [u32; 2], settings: &foveonic::render::Settings) -> f32 {
    let [width, height] = crate::rendering::quality::shown_extent(camera_extent, settings, false);
    width as f32 / height.max(1) as f32
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
