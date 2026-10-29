//! Paints aspect-preserving browser images and shared selectable metadata rows with star controls.

use super::{query::Field, Browser, Mark, STATUS_GUTTER, STATUS_INSET};
use eframe::egui;
use std::path::Path;

const IMAGE_MARGIN: f32 = 6.;
const GRID_STATUS_INSET: f32 = 7.;
pub(super) const PICK_BORDER: egui::Color32 = egui::Color32::from_rgb(103, 190, 126);
const CAPTION_HEIGHT: f32 = 36.;
const CAPTION_LINE: f32 = 18.;
const CAPTION_FONT: f32 = 12.;
const LIST_THUMBNAIL: f32 = 32.;
const LIST_LEAD: f32 = 45.;
pub(super) const LIST_HEADER: f32 = 26.;
const COLUMNS: [(Field, f32); 6] = [
    (Field::Name, 140.),
    (Field::Rating, 84.),
    (Field::Iso, 52.),
    (Field::Aperture, 68.),
    (Field::Shutter, 78.),
    (Field::Date, 132.),
];
fn columns(show_ratings: bool) -> impl Iterator<Item = (Field, f32)> {
    COLUMNS
        .into_iter()
        .filter(move |(field, _)| show_ratings || *field != Field::Rating)
}
pub(super) fn list_width(show_ratings: bool) -> f32 {
    LIST_LEAD + columns(show_ratings).map(|(_, width)| width).sum::<f32>() + STATUS_GUTTER
}
pub(super) fn dense_height(width: f32) -> f32 {
    width * 1.08 + 2. * IMAGE_MARGIN
}

impl Browser {
    fn caption_height(&self) -> f32 {
        if self.import_mode || self.show_ratings {
            CAPTION_HEIGHT
        } else {
            CAPTION_LINE
        }
    }
    pub(super) fn large_height(&self, path: &Path, width: f32) -> f32 {
        let extent = self
            .records
            .get(path)
            .and_then(|record| record.extent)
            .unwrap_or([3, 2]);
        let ratio = extent[1] as f32 / extent[0].max(1) as f32;
        (width - 2. * IMAGE_MARGIN).max(1.) * ratio + self.caption_height() + 2. * IMAGE_MARGIN
    }
    pub(super) fn tile(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        selected: bool,
        active: bool,
        size: egui::Vec2,
        dense: bool,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::CLICK);
        background(ui, &response, selected && !dense, active);
        let image_area = egui::Rect::from_min_max(
            rect.min + egui::Vec2::splat(IMAGE_MARGIN),
            rect.max
                - egui::vec2(
                    IMAGE_MARGIN,
                    IMAGE_MARGIN + if dense { 0. } else { self.caption_height() },
                ),
        );
        let image = if ui.is_rect_visible(rect) {
            if size.x > 300. {
                self.preview(path)
            } else {
                self.thumbnail(path)
            }
        } else {
            None
        };
        let thumbnail_rect = image.map_or(image_area, |image| image.paint(ui, image_area));
        if dense {
            let picked = self.import_mode && self.mark(path) == Mark::Pick;
            if selected || picked {
                ui.painter().rect_stroke(
                    thumbnail_rect.expand(2.),
                    2.,
                    egui::Stroke::new(
                        if active { 2.5_f32 } else { 1_f32 },
                        if picked {
                            PICK_BORDER
                        } else if active {
                            egui::Color32::LIGHT_GRAY
                        } else {
                            egui::Color32::GRAY
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
            }
            self.cache_badge(
                ui,
                path,
                egui::pos2(
                    rect.right() - GRID_STATUS_INSET,
                    rect.bottom() - STATUS_INSET,
                ),
            );
            self.review_menu(&response, path);
            let mut help = self.file_help(path);
            if self.import_mode {
                help.push_str(" Right-click for individual review actions.");
            }
            return response.on_hover_text(help);
        }
        let rating = egui::Rect::from_min_max(
            egui::pos2(rect.left() + IMAGE_MARGIN, rect.bottom() - CAPTION_HEIGHT),
            egui::pos2(rect.right() - IMAGE_MARGIN, rect.bottom() - CAPTION_LINE),
        );
        let rating = self.review_control(ui, path, rating);
        self.stars(ui, path, rating);
        let caption = egui::Rect::from_min_max(
            egui::pos2(rect.left() + IMAGE_MARGIN, rect.bottom() - CAPTION_LINE),
            egui::pos2(rect.right() - STATUS_GUTTER, rect.bottom()),
        );
        ui.painter().with_clip_rect(caption).text(
            caption.center(),
            egui::Align2::CENTER_CENTER,
            path.file_stem().unwrap_or_default().to_string_lossy(),
            egui::FontId::proportional(CAPTION_FONT),
            egui::Color32::LIGHT_GRAY,
        );
        self.cache_badge(
            ui,
            path,
            egui::pos2(rect.right() - STATUS_INSET, caption.center().y),
        );
        response.on_hover_text(self.file_help(path))
    }
    pub(super) fn list_header(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        ui.painter()
            .rect_filled(rect, 0., ui.visuals().faint_bg_color);
        let mut left = rect.left() + LIST_LEAD;
        for (field, width) in columns(self.show_ratings) {
            let cell = egui::Rect::from_min_size(
                egui::pos2(left, rect.top()),
                egui::vec2(width, rect.height()),
            );
            let label = if field == Field::Shutter {
                "Shutter"
            } else {
                field.label()
            };
            if ui
                .put(cell, egui::Button::new(label).frame(false).truncate())
                .clicked()
            {
                if self.sort.field == field {
                    self.sort.reverse = !self.sort.reverse;
                } else {
                    self.sort.field = field;
                    self.sort.reverse = false;
                }
                self.query_dirty = true;
            }
            if self.sort.field == field {
                let center = egui::pos2(cell.right() - 8., cell.center().y);
                let direction = if self.sort.reverse { 1. } else { -1. };
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![
                        center + egui::vec2(-3., -2. * direction),
                        center + egui::vec2(3., -2. * direction),
                        center + egui::vec2(0., 2. * direction),
                    ],
                    egui::Color32::GRAY,
                    egui::Stroke::NONE,
                ));
            }
            left += width;
        }
    }
    pub(super) fn list_item(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        selected: bool,
        active: bool,
        size: egui::Vec2,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        background(ui, &response, selected, active);
        if let Some(thumb) = self.thumbnail(path) {
            thumb.paint(
                ui,
                egui::Rect::from_center_size(
                    egui::pos2(rect.left() + LIST_LEAD / 2., rect.center().y),
                    egui::Vec2::splat(LIST_THUMBNAIL),
                ),
            );
        }
        let properties = self
            .records
            .get(path)
            .map(|entry| crate::metadata::property_rows(&entry.properties))
            .unwrap_or_default();
        let mut left = rect.left() + LIST_LEAD;
        for (field, width) in columns(self.show_ratings) {
            let mut cell = egui::Rect::from_min_size(
                egui::pos2(left, rect.top()),
                egui::vec2(width, rect.height()),
            );
            if field == Field::Rating {
                self.stars(ui, path, cell);
            } else {
                let text = if field == Field::Name {
                    cell = self.review_control(ui, path, cell);
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                } else {
                    properties
                        .iter()
                        .find(|(label, _)| {
                            *label
                                == match field {
                                    Field::Date => "Captured",
                                    Field::Shutter => "Shutter",
                                    _ => field.label(),
                                }
                        })
                        .map(|(_, value)| value.clone())
                        .unwrap_or_else(|| "—".into())
                };
                ui.painter()
                    .with_clip_rect(cell.shrink2(egui::vec2(4., 0.)))
                    .text(
                        egui::pos2(cell.left() + 4., cell.center().y),
                        egui::Align2::LEFT_CENTER,
                        text,
                        egui::FontId::proportional(CAPTION_FONT),
                        egui::Color32::LIGHT_GRAY,
                    );
            }
            left += width;
        }
        self.cache_badge(ui, path, rect.right_center() - egui::vec2(STATUS_INSET, 0.));
        response.on_hover_text(self.file_help(path))
    }
}

impl super::Thumbnail {
    pub fn paint(&self, ui: &egui::Ui, bounds: egui::Rect) -> egui::Rect {
        let extent = egui::vec2(self.native_extent[0] as f32, self.native_extent[1] as f32);
        let scale = (bounds.width() / extent.x).min(bounds.height() / extent.y);
        let rect = egui::Rect::from_center_size(bounds.center(), extent * scale);
        ui.painter()
            .with_clip_rect(bounds.intersect(ui.clip_rect()))
            .image(
                self.texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        rect
    }
}

fn background(ui: &egui::Ui, response: &egui::Response, selected: bool, active: bool) {
    if selected {
        ui.painter().rect_stroke(
            response.rect,
            2.,
            egui::Stroke::new(
                if active { 2_f32 } else { 1_f32 },
                if active {
                    egui::Color32::LIGHT_GRAY
                } else {
                    egui::Color32::GRAY
                },
            ),
            egui::StrokeKind::Inside,
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(response.rect, 2., ui.visuals().widgets.hovered.bg_fill);
    }
}
