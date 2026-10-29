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
const CELL_PADDING: f32 = 4.;
const SORT_GUTTER: f32 = 14.;
pub(super) const LIST_HEADER: f32 = 26.;
const COLUMNS: [Field; 6] = [
    Field::Name,
    Field::Rating,
    Field::Iso,
    Field::Aperture,
    Field::Shutter,
    Field::Date,
];
#[derive(Clone)]
pub(super) struct ListColumns {
    widths: [f32; 6],
    import: bool,
    ratings: bool,
    pixels_per_point: f32,
}
impl ListColumns {
    fn columns(&self) -> impl Iterator<Item = (Field, f32)> + '_ {
        COLUMNS
            .into_iter()
            .zip(self.widths)
            .filter(|(field, _)| self.ratings || *field != Field::Rating)
    }
    pub fn width(&self) -> f32 {
        LIST_LEAD + self.columns().map(|(_, width)| width).sum::<f32>() + STATUS_GUTTER
    }
    fn name_gutter(&self) -> f32 {
        if self.import {
            super::review::MARK_GUTTER
        } else {
            0.
        }
    }
}
fn column_label(field: Field) -> &'static str {
    if field == Field::Shutter {
        "Shutter"
    } else {
        field.label()
    }
}
fn column_text(path: &Path, properties: &[(&str, String)], field: Field) -> String {
    if field == Field::Name {
        path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        let label = match field {
            Field::Date => "Captured",
            _ => column_label(field),
        };
        properties
            .iter()
            .find(|(key, _)| *key == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| "—".into())
    }
}
pub(super) fn dense_height(width: f32) -> f32 {
    width * 1.08 + 2. * IMAGE_MARGIN
}

impl Browser {
    pub(super) fn list_columns(&mut self, ui: &egui::Ui) -> ListColumns {
        let pixels_per_point = ui.ctx().pixels_per_point();
        if let Some(columns) = &self.list_columns {
            if columns.import == self.import_mode
                && columns.ratings == self.show_ratings
                && columns.pixels_per_point == pixels_per_point
            {
                return columns.clone();
            }
        }
        let measure = |text: String| {
            ui.painter()
                .layout_no_wrap(
                    text,
                    egui::FontId::proportional(CAPTION_FONT),
                    egui::Color32::LIGHT_GRAY,
                )
                .size()
                .x
        };
        let mut widths = COLUMNS.map(|field| measure(column_label(field).into()));
        for path in &self.files {
            let properties = self
                .records
                .get(path)
                .map(|entry| crate::metadata::property_rows(&entry.properties))
                .unwrap_or_default();
            for (index, field) in COLUMNS.into_iter().enumerate() {
                if field != Field::Rating {
                    widths[index] =
                        widths[index].max(measure(column_text(path, &properties, field)));
                }
            }
        }
        widths[1] = widths[1].max(super::chrome::STAR_WIDTH);
        let mut columns = ListColumns {
            widths,
            import: self.import_mode,
            ratings: self.show_ratings,
            pixels_per_point,
        };
        columns.widths[0] += columns.name_gutter();
        for width in &mut columns.widths {
            *width += 2. * CELL_PADDING + SORT_GUTTER;
        }
        self.list_columns = Some(columns.clone());
        columns
    }
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
            self.file_menu(&response, path);
            return response.on_hover_text(self.file_help(path));
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
        self.file_menu(&response, path);
        response.on_hover_text(self.file_help(path))
    }
    pub(super) fn list_header(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        columns: &ListColumns,
    ) {
        ui.painter()
            .rect_filled(rect, 0., ui.visuals().faint_bg_color);
        let mut left = rect.left() + LIST_LEAD;
        for (field, width) in columns.columns() {
            let cell = egui::Rect::from_min_size(
                egui::pos2(left, rect.top()),
                egui::vec2(width, rect.height()),
            );
            let response = ui.interact(
                cell,
                ui.id().with(("list column", field.label())),
                egui::Sense::click(),
            );
            if response.hovered() {
                ui.painter()
                    .rect_filled(cell, 0., ui.visuals().widgets.hovered.bg_fill);
            }
            let gutter = if field == Field::Name {
                columns.name_gutter()
            } else {
                0.
            };
            ui.painter().text(
                egui::pos2(cell.left() + gutter + CELL_PADDING, cell.center().y),
                egui::Align2::LEFT_CENTER,
                column_label(field),
                egui::FontId::proportional(CAPTION_FONT),
                egui::Color32::LIGHT_GRAY,
            );
            if response.clicked() {
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
        columns: &ListColumns,
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
        for (field, width) in columns.columns() {
            let mut cell = egui::Rect::from_min_size(
                egui::pos2(left, rect.top()),
                egui::vec2(width, rect.height()),
            );
            if field == Field::Rating {
                self.stars(
                    ui,
                    path,
                    egui::Rect::from_min_size(
                        cell.min + egui::vec2(CELL_PADDING, 0.),
                        egui::vec2(super::chrome::STAR_WIDTH, cell.height()),
                    ),
                );
            } else {
                if field == Field::Name {
                    cell = self.review_control(ui, path, cell);
                }
                let text = column_text(path, &properties, field);
                ui.painter()
                    .with_clip_rect(cell.shrink2(egui::vec2(CELL_PADDING, 0.)))
                    .text(
                        egui::pos2(cell.left() + CELL_PADDING, cell.center().y),
                        egui::Align2::LEFT_CENTER,
                        text,
                        egui::FontId::proportional(CAPTION_FONT),
                        egui::Color32::LIGHT_GRAY,
                    );
            }
            left += width;
        }
        self.cache_badge(ui, path, rect.right_center() - egui::vec2(STATUS_INSET, 0.));
        self.file_menu(&response, path);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_fit_offscreen_values_and_refresh_when_metadata_or_review_controls_change() {
        let ctx = egui::Context::default();
        let mut browser = Browser::new(ctx.clone());
        browser.files = (0..60)
            .map(|index| format!("image-{index}.x3f").into())
            .collect();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let initial = browser.list_columns(ui);
                let path =
                    std::path::PathBuf::from("a-much-longer-filename-outside-the-visible-rows.x3f");
                browser.files[59] = path.clone();
                browser.records.insert(
                    path,
                    super::super::query::Entry::new(
                        std::collections::BTreeMap::from([(
                            "ISO".into(),
                            "12345678901234567890".into(),
                        )]),
                        None,
                        Default::default(),
                    ),
                );
                browser.query_dirty = true;
                browser.apply_query();
                let updated = browser.list_columns(ui);
                assert!(updated.widths[0] > initial.widths[0]);
                assert!(updated.widths[2] > initial.widths[2]);
                assert_eq!(updated.widths[3..], initial.widths[3..]);

                browser.import_mode = true;
                let import = browser.list_columns(ui);
                assert_eq!(
                    import.widths[0],
                    updated.widths[0] + super::super::review::MARK_GUTTER
                );
                browser.show_ratings = true;
                let ratings = browser.list_columns(ui);
                assert!(ratings.width() > import.width());
                assert_eq!(ratings.widths, import.widths);
            });
        });
    }
}
