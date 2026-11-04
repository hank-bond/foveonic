//! Presents highlighted photos together while the active photo keeps the shared editor and RAW renderer.

use crate::App;
use eframe::egui;
use std::path::PathBuf;

// Leave one larger-preview slot for the currently rendered photo.
const VISIBLE_PREVIEWS: usize = 7;

impl App {
    pub(crate) fn comparison_view(&mut self, ui: &mut egui::Ui, paths: &[PathBuf]) -> bool {
        let mut changed = false;
        let available = ui.available_size();
        if ui.rect_contains_pointer(ui.available_rect_before_wrap()) {
            self.comparison_zoom =
                (self.comparison_zoom * ui.input(|i| i.zoom_delta())).clamp(0.5, 3.);
        }
        let columns = (available.x / 200.).floor().max(1.) as usize;
        let gap = 8.;
        let width = ((available.x - (columns - 1) as f32 * gap) / columns as f32).max(100.)
            * self.comparison_zoom;
        let height = width * 1.6;
        let rows = paths.len().div_ceil(columns);
        let mapping = self.displayed_mapping();
        egui::ScrollArea::both()
            .id_salt("photo comparison")
            .show_viewport(ui, |ui, visible| {
                let (canvas, _) = ui.allocate_exact_size(
                    egui::vec2(
                        columns as f32 * (width + gap) - gap,
                        rows as f32 * (height + gap),
                    ),
                    egui::Sense::hover(),
                );
                let mut large_previews = 0;
                for (index, path) in paths.iter().enumerate() {
                    let rect = egui::Rect::from_min_size(
                        canvas.min
                            + egui::vec2(
                                (index % columns) as f32 * (width + gap),
                                (index / columns) as f32 * (height + gap),
                            ),
                        egui::vec2(width, height),
                    );
                    if !rect.intersects(visible.translate(canvas.min.to_vec2())) {
                        continue;
                    }
                    let active = self.selected.as_ref() == Some(path);
                    let image_rect = egui::Rect::from_min_max(
                        rect.min + egui::Vec2::splat(3.),
                        rect.max - egui::vec2(3., 23.),
                    );
                    ui.painter()
                        .rect_filled(rect, 2., egui::Color32::from_gray(20));
                    if active {
                        let mut child = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(image_rect)
                                .id_salt("active comparison"),
                        );
                        child.set_clip_rect(image_rect.intersect(ui.clip_rect()));
                        // Ordinary comparison navigation belongs to the canvas. The
                        // active photo alone accepts direct color gestures.
                        if !(crate::editor::direct_color::active(ui.ctx()) || crate::editor::dehaze::active(ui.ctx()))
                            || !self.color_tool_available(mapping, ui.ctx())
                        {
                            child.disable();
                        }
                        changed |= self.show_photo(&mut child, mapping);
                        self.before_badge(&child, image_rect);
                    } else {
                        let preview = if large_previews < VISIBLE_PREVIEWS {
                            large_previews += 1;
                            self.browser.preview(path)
                        } else {
                            self.browser.thumbnail(path)
                        };
                        if let Some(preview) = preview {
                            preview.paint(ui, image_rect);
                        }
                        if ui
                            .interact(
                                rect,
                                ui.id().with(("comparison photo", path)),
                                egui::Sense::click(),
                            )
                            .on_hover_text("Click to edit this photo. Other photos use cached or embedded previews.")
                            .clicked()
                        {
                            self.comparison_selection = Some(path.clone());
                        }
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
                    ui.painter().with_clip_rect(rect).text(
                        egui::pos2(rect.center().x, rect.bottom() - 11.),
                        egui::Align2::CENTER_CENTER,
                        path.file_stem().unwrap_or_default().to_string_lossy(),
                        egui::TextStyle::Body.resolve(ui.style()),
                        egui::Color32::LIGHT_GRAY,
                    );
                }
            });
        changed
    }
}
