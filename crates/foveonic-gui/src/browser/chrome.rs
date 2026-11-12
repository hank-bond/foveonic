//! Draws compact browser view/sort/search menus and independent star-rating controls.

use super::{
    query::{Field, RatingFilter},
    Browser,
};
use crate::{preferences::Organizer, widgets};
use eframe::egui;
use std::path::Path;

const STAR_SIDE: f32 = 14.;
pub(super) const STAR_WIDTH: f32 = STAR_SIDE * 5.;
const VIEW_ICON_SIDE: f32 = 12.;
const MIN_SEARCH_WIDTH: f32 = 148.;

impl Browser {
    pub fn header(
        &mut self,
        ui: &mut egui::Ui,
        organizer: &mut Organizer,
        enabled: bool,
        focus: egui::Id,
    ) {
        self.apply_query();
        let visible = self.files.len() - self.excluded.len();
        let count = if visible == self.files.len() {
            format!("{visible} images")
        } else {
            format!("{visible} / {} images", self.files.len())
        };
        let combined = if self.import_mode {
            let (picked, hidden) = self.review_counts();
            format!("{count} | {picked} picked, {hidden} hidden")
        } else {
            count.clone()
        };
        let label_width = ui
            .painter()
            .layout_no_wrap(
                combined.clone(),
                egui::TextStyle::Body.resolve(ui.style()),
                egui::Color32::GRAY,
            )
            .size()
            .x;
        let side = widgets::ControlSize::Medium.side();
        let gap = widgets::MEDIUM_CONTROL_GAP;
        let search_width = MIN_SEARCH_WIDTH
            + side
            + gap
            + if self.filter.text.is_empty() {
                0.
            } else {
                side + gap
            };
        let inline = ui.available_width()
            >= label_width
                + search_width
                + 3. * (side + gap)
                + if self.import_mode { side + gap } else { 0. };
        widgets::control_row(ui, |ui| {
            ui.spacing_mut().item_spacing.x = widgets::MEDIUM_CONTROL_GAP;
            let view = icon_menu(ui, |ui| {
                ui.horizontal(|ui| {
                    for mode in [Organizer::Icons, Organizer::List, Organizer::Large] {
                        if view_button(ui, mode, *organizer == mode).clicked() {
                            *organizer = mode;
                            self.reveal_selection();
                            ui.close_menu();
                        }
                    }
                });
            })
            .response;
            paint_view(ui, view.rect.center(), *organizer, egui::Color32::GRAY);
            view.on_hover_text("Browser view");
            let sort = icon_menu(ui, |ui| {
                if crate::widgets::checkbox(ui, &mut self.sort.reverse, "Reverse").changed() {
                    self.query_dirty = true;
                }
                ui.separator();
                for field in Field::ALL {
                    if ui
                        .selectable_value(&mut self.sort.field, field, field.label())
                        .clicked()
                    {
                        self.query_dirty = true;
                        ui.close_menu();
                    }
                }
            })
            .response;
            let center = sort.rect.center();
            let stroke = egui::Stroke::new(1.3_f32, egui::Color32::GRAY);
            for (x, direction) in [(-3., -1.), (3., 1.)] {
                let tip = center + egui::vec2(x, direction * 6.);
                ui.painter()
                    .line_segment([center + egui::vec2(x, -direction * 6.), tip], stroke);
                ui.painter()
                    .line_segment([tip + egui::vec2(-2., -direction * 3.), tip], stroke);
                ui.painter()
                    .line_segment([tip + egui::vec2(2., -direction * 3.), tip], stroke);
            }
            sort.on_hover_text(format!("Sort by {}", self.sort.field.label()));
            let label = if inline { &combined } else { &count };
            let reserved = side
                + gap
                + if self.import_mode {
                    side + ui.spacing().item_spacing.x
                } else {
                    0.
                }
                + if inline { search_width + gap } else { 0. };
            ui.add_sized(
                egui::vec2(
                    (ui.available_width() - reserved).max(1.),
                    widgets::control_height(ui),
                ),
                egui::Label::new(label).truncate(),
            );
            ui.add_enabled_ui(enabled, |ui| {
                self.ratings_button(ui);
            });
            if self.import_mode {
                ui.add_enabled_ui(enabled, |ui| {
                    self.visibility_button(ui, focus);
                });
            }
            if inline {
                self.search_controls(ui);
            }
        });
        if !inline {
            widgets::centered_control_row(ui, |ui| self.search_controls(ui));
        }
        if self.import_mode && !inline {
            let (picked, hidden) = self.review_counts();
            ui.weak(format!("{picked} picked, {hidden} hidden"));
        }
        if let Some((path, error)) = self.rating_errors.first_key_value() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Rating could not be saved or restored",
            )
            .on_hover_text(format!("{}: {error}", path.display()));
        }
    }

    pub(super) fn ratings_button(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            egui::Vec2::splat(widgets::ControlSize::Medium.side()),
            egui::Sense::click(),
        );
        if response.clicked() {
            self.show_ratings = !self.show_ratings;
            self.reveal_selection();
        }
        widgets::paint(ui, &response, self.show_ratings);
        let color = widgets::visuals(ui, &response, self.show_ratings)
            .fg_stroke
            .color;
        outline_star(ui, rect.center(), 6., color);
        response.on_hover_text(if self.show_ratings {
            "Hide ratings in list and large view · keys 0–5 rate highlighted photos"
        } else {
            "Show ratings in list and large view · keys 0–5 rate highlighted photos"
        })
    }

    fn search_controls(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.x = widgets::MEDIUM_CONTROL_GAP;
        let filtered = self.filter.rating != RatingFilter::All;
        let search = icon_menu(ui, |ui| {
            ui.weak("Rating filter");
            let mut changed = ui
                .selectable_value(&mut self.filter.rating, RatingFilter::All, "All ratings")
                .clicked();
            changed |= ui
                .selectable_value(&mut self.filter.rating, RatingFilter::Unrated, "Unrated")
                .clicked();
            ui.separator();
            for stars in 1..=5 {
                let label = format!(
                    "{}{}",
                    "★".repeat(stars as usize),
                    if stars < 5 { " or higher" } else { "" }
                );
                changed |= ui
                    .selectable_value(&mut self.filter.rating, RatingFilter::AtLeast(stars), label)
                    .clicked();
            }
            if changed {
                self.query_dirty = true;
                ui.close_menu();
            }
        })
        .response;
        let center = search.rect.center() - egui::vec2(2., 2.);
        let stroke = egui::Stroke::new(1.3_f32, egui::Color32::GRAY);
        ui.painter().circle_stroke(center, 4., stroke);
        ui.painter().line_segment(
            [center + egui::vec2(3., 3.), center + egui::vec2(7., 7.)],
            stroke,
        );
        if filtered {
            ui.painter().circle_filled(
                search.rect.right_top() + egui::vec2(-3., 3.),
                2.,
                ui.visuals().selection.stroke.color,
            );
        }
        search.on_hover_text("Filter by star rating");
        let clear_width = if self.filter.text.is_empty() {
            0.
        } else {
            widgets::ControlSize::Medium.side() + ui.spacing().item_spacing.x
        };
        if ui
            .add_sized(
                egui::vec2(
                    (ui.available_width() - clear_width).max(30.),
                    widgets::control_height(ui),
                ),
                egui::TextEdit::singleline(&mut self.filter.text)
                    .id(egui::Id::new("browser search"))
                    .hint_text("Search")
                    .desired_width(f32::INFINITY),
            )
            .changed()
        {
            self.query_dirty = true;
        }
        if !self.filter.text.is_empty() {
            let side = widgets::ControlSize::Medium.side();
            let clear = ui.add_sized(egui::Vec2::splat(side), egui::Button::new(""));
            let radius = side * 0.24;
            let stroke =
                egui::Stroke::new(1.4_f32, widgets::visuals(ui, &clear, false).fg_stroke.color);
            for direction in [-1., 1.] {
                let delta = egui::vec2(radius, direction * radius);
                ui.painter().line_segment(
                    [clear.rect.center() - delta, clear.rect.center() + delta],
                    stroke,
                );
            }
            if clear.on_hover_text("Clear search").clicked() {
                self.filter.text.clear();
                self.query_dirty = true;
            }
        }
    }

    pub(super) fn stars(&mut self, ui: &mut egui::Ui, path: &Path, rect: egui::Rect) {
        if !self.show_ratings {
            return;
        }
        let width = STAR_WIDTH.min(rect.width());
        let row = egui::Rect::from_center_size(rect.center(), egui::vec2(width, STAR_SIDE));
        let current = self.rating(path).stars();
        let hovered = if width > 0. && ui.is_enabled() && ui.rect_contains_pointer(row) {
            ui.input(|input| input.pointer.hover_pos())
                .map(|position| (((position.x - row.left()) / (width / 5.)) as u8 + 1).min(5))
        } else {
            None
        };
        let preview = hovered.filter(|stars| *stars != current);
        for index in 0..5_u8 {
            let cell = egui::Rect::from_min_size(
                row.min + egui::vec2(f32::from(index) * width / 5., 0.),
                egui::vec2(width / 5., STAR_SIDE),
            );
            let response = ui.interact(
                cell,
                ui.make_persistent_id(("star rating", path, index)),
                egui::Sense::click(),
            );
            if response.clicked() {
                let value = if current == index + 1 { 0 } else { index + 1 };
                let targets = if self.highlighted.contains(path) {
                    self.processing_selection(Some(path))
                } else {
                    vec![path.to_owned()]
                };
                self.rate(&targets, foveonic::review::Rating::try_from(value).unwrap());
                self.review_clicked = true;
                ui.ctx().request_repaint();
            }
            let color = if preview.is_some_and(|stars| index < stars)
                || (preview.is_none() && index < current)
            {
                egui::Color32::LIGHT_GRAY
            } else {
                egui::Color32::from_gray(75)
            };
            if preview.is_some_and(|stars| index < stars) {
                outline_star(ui, cell.center(), 5.5, color);
            } else {
                ui.painter().text(
                    cell.center(),
                    egui::Align2::CENTER_CENTER,
                    if preview.is_none() && index < current {
                        "★"
                    } else {
                        "·"
                    },
                    egui::FontId::proportional(STAR_SIDE),
                    color,
                );
            }
            response.on_hover_text(format!(
                "{} star{} · click the current rating to clear · keys 0–5 rate highlighted photos",
                index + 1,
                if index == 0 { "" } else { "s" }
            ));
        }
    }
}

fn outline_star(ui: &egui::Ui, center: egui::Pos2, radius: f32, color: egui::Color32) {
    let points = (0..10)
        .map(|index| {
            let angle = -std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::PI / 5.;
            center
                + egui::vec2(angle.cos(), angle.sin())
                    * if index % 2 == 0 {
                        radius
                    } else {
                        radius * 0.45
                    }
        })
        .collect();
    ui.painter().add(egui::Shape::closed_line(
        points,
        egui::Stroke::new(1.2_f32, color),
    ));
}

fn view_button(ui: &mut egui::Ui, mode: Organizer, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::splat(widgets::ControlSize::Medium.side()),
        egui::Sense::click(),
    );
    widgets::paint(ui, &response, selected);
    let color = if selected {
        ui.visuals().selection.stroke.color
    } else {
        egui::Color32::GRAY
    };
    paint_view(ui, rect.center(), mode, color);
    response.on_hover_text(match mode {
        Organizer::Icons => "Grid",
        Organizer::List => "List",
        Organizer::Large => "Large previews",
    })
}

fn paint_view(ui: &egui::Ui, center: egui::Pos2, mode: Organizer, color: egui::Color32) {
    let icon = egui::Rect::from_center_size(center, egui::Vec2::splat(VIEW_ICON_SIDE));
    // Three four-unit cells and two two-unit gaps fill the centered icon canvas.
    let scale = VIEW_ICON_SIDE / 16.;
    let block = |x, y, width, height| {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                icon.min + egui::vec2(x, y) * scale,
                egui::vec2(width, height) * scale,
            ),
            0.,
            color,
        );
    };
    for row in 0..3 {
        let top = row as f32 * 6.;
        match mode {
            Organizer::Icons => {
                for col in 0..3 {
                    block(col as f32 * 6., top, 4., 4.);
                }
            }
            Organizer::List => {
                block(0., top, 4., 4.);
                block(6., top, 10., 4.);
            }
            Organizer::Large => {
                block(0., top, 16., 4.);
            }
        }
    }
}

fn icon_menu<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    widgets::icon_menu_button(
        ui,
        "Browser options",
        widgets::ControlSize::Medium,
        contents,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_previews_outlines_without_changing_the_saved_rating() {
        let ctx = egui::Context::default();
        let mut browser = Browser::new(ctx.clone());
        browser.show_ratings = true;
        browser.import_mode = true;
        let path = std::path::PathBuf::from("photo.x3f");
        browser.files.push(path.clone());
        let mut time = 0.;
        let mut frame = |events| {
            time += 0.01;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(200., 100.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        browser.stars(
                            ui,
                            &path,
                            egui::Rect::from_min_size(egui::pos2(40., 40.), egui::vec2(70., 14.)),
                        );
                    });
                },
            );
            let outlines = output.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Path(path) if path.closed && path.points.len() == 10)).count();
            let filled = output.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "★")).count();
            (outlines, filled, browser.rating(&path).stars())
        };
        frame(vec![]);
        let third = egui::pos2(75., 47.);
        assert_eq!(frame(vec![egui::Event::PointerMoved(third)]), (3, 0, 0));
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(vec![pointer(third, true)]);
        frame(vec![pointer(third, false)]);
        assert_eq!(frame(vec![]), (0, 3, 3));
        assert_eq!(
            frame(vec![egui::Event::PointerMoved(egui::pos2(103., 47.))]),
            (5, 0, 3)
        );
        assert_eq!(
            frame(vec![egui::Event::PointerMoved(egui::pos2(150., 80.))]),
            (0, 3, 3)
        );
    }
}
