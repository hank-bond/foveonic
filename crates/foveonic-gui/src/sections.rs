//! Draws shared disclosure triangles and tool-section headers, accepting section drops only inside the tools pane.
use crate::preferences::{EditingSection, Section};
use eframe::egui;

pub const SECTION_GAP: f32 = 7.;

pub fn disclosure_icon(ui: &egui::Ui, center: egui::Pos2, collapsed: bool, color: egui::Color32) {
    let points = if collapsed {
        vec![
            center + egui::vec2(-2., -4.),
            center + egui::vec2(3., 0.),
            center + egui::vec2(-2., 4.),
        ]
    } else {
        vec![
            center + egui::vec2(-4., -2.),
            center + egui::vec2(4., -2.),
            center + egui::vec2(0., 3.),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
}

pub fn header(ui: &mut egui::Ui, section: Section, collapsed: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 29.),
        egui::Sense::click_and_drag(),
    );
    crate::widgets::paint(ui, &response, false);
    disclosure_icon(
        ui,
        rect.left_center() + egui::vec2(11., 0.),
        collapsed,
        egui::Color32::GRAY,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(24., 0.),
        egui::Align2::LEFT_CENTER,
        section.title(),
        egui::FontId::proportional(13.),
        egui::Color32::LIGHT_GRAY,
    );
    grip_icon(ui, rect.right_center() + egui::vec2(-12., 0.), 1.);
    response.dnd_set_drag_payload(section);
    response
}
/// Shared six-dot grip, scaled down for editing-tool headers.
pub fn grip_icon(ui: &egui::Ui, center: egui::Pos2, scale: f32) {
    for x in [-2., 2.] {
        for y in [-4., 0., 4.] {
            ui.painter().circle_filled(
                center + egui::vec2(x, y) * scale,
                scale,
                egui::Color32::GRAY,
            );
        }
    }
}

pub fn editing_handle(ui: &mut egui::Ui, section: EditingSection) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(20., 20.), egui::Sense::drag());
    grip_icon(ui, rect.center(), 0.65);
    response.dnd_set_drag_payload(section);
    response
        .on_hover_cursor(egui::CursorIcon::Grab)
        .on_hover_text("Drag to reorder editing sections");
}

pub fn drop(ui: &mut egui::Ui, header: egui::Rect) -> Option<(Section, bool)> {
    let zone = egui::Rect::from_min_max(header.min, egui::pos2(header.right(), ui.cursor().top()));
    let after = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| p.y > zone.center().y);
    let y = if after { zone.bottom() } else { zone.top() };
    drop_target(ui, zone, y).map(|source| (source, after))
}

pub fn drop_editing(
    ui: &mut egui::Ui,
    zone: egui::Rect,
    indicator_y: f32,
) -> Option<EditingSection> {
    drop_target(ui, zone, indicator_y)
}

fn drop_target<T: Clone + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    zone: egui::Rect,
    indicator_y: f32,
) -> Option<T> {
    let response = ui.interact(
        zone,
        ui.id().with(("section drop", std::any::TypeId::of::<T>())),
        egui::Sense::hover(),
    );
    // Taking a payload consumes it even when its type cannot be downcast.
    // Leave other workspace/tool drags available to their owning drop target.
    response.dnd_hover_payload::<T>()?;
    ui.painter().line_segment(
        [
            egui::pos2(zone.left(), indicator_y),
            egui::pos2(zone.right(), indicator_y),
        ],
        egui::Stroke::new(2_f32, egui::Color32::from_rgb(85, 145, 235)),
    );
    response.dnd_release_payload::<T>().map(|s| (*s).clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;
    fn frame(
        ctx: &egui::Context,
        prefs: &mut Preferences,
        time: f64,
        events: Vec<egui::Event>,
    ) -> Vec<(Section, egui::Rect)> {
        let mut rects = Vec::new();
        let mut movement = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::SidePanel::right("tools")
                    .exact_width(300.)
                    .show(ctx, |ui| {
                        for section in prefs.sections.clone() {
                            ui.add_space(SECTION_GAP);
                            ui.push_id(section.title(), |ui| {
                                let response =
                                    header(ui, section, prefs.collapsed.contains(&section));
                                rects.push((section, response.rect));
                                if response.clicked() {
                                    prefs.toggle(section);
                                }
                                if let Some((source, after)) = drop(ui, response.rect) {
                                    movement = Some((source, section, after));
                                }
                            });
                        }
                    });
            },
        );
        if let Some((a, b, c)) = movement {
            prefs.move_section(a, b, c);
        }
        rects
    }
    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    #[test]
    fn pointer_collapse_and_drag_reorder_stay_in_the_right_pane() {
        let ctx = egui::Context::default();
        let mut prefs = Preferences::default();
        frame(&ctx, &mut prefs, 0., vec![]);
        let rects = frame(&ctx, &mut prefs, 0.1, vec![]);
        let source = rects[0].1.center();
        let target = rects[3].1.center() + egui::vec2(0., 8.);
        frame(
            &ctx,
            &mut prefs,
            0.2,
            vec![egui::Event::PointerMoved(source), button(source, true)],
        );
        frame(
            &ctx,
            &mut prefs,
            0.3,
            vec![egui::Event::PointerMoved(target)],
        );
        frame(&ctx, &mut prefs, 0.4, vec![button(target, false)]);
        assert_eq!(
            prefs.sections,
            [
                Section::Scopes,
                Section::Editing,
                Section::Metadata,
                Section::Navigator
            ]
        );
        let rects = frame(&ctx, &mut prefs, 0.5, vec![]);
        let click = rects[0].1.center();
        frame(
            &ctx,
            &mut prefs,
            0.6,
            vec![egui::Event::PointerMoved(click), button(click, true)],
        );
        frame(&ctx, &mut prefs, 0.7, vec![button(click, false)]);
        assert!(prefs.collapsed.contains(&Section::Scopes));
        let saved = prefs.sections.clone();
        frame(&ctx, &mut prefs, 0.8, vec![button(click, true)]);
        let outside = egui::pos2(50., 400.);
        frame(
            &ctx,
            &mut prefs,
            0.9,
            vec![egui::Event::PointerMoved(outside)],
        );
        frame(&ctx, &mut prefs, 1., vec![button(outside, false)]);
        assert_eq!(prefs.sections, saved);
    }
}

#[cfg(test)]
mod payload_tests {
    use super::*;
    #[test]
    fn editing_drop_keeps_an_outer_workspace_payload_for_its_owner() {
        let ctx = egui::Context::default();
        let point = egui::pos2(60., 50.);
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(240., 160.),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let header = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(ui.available_width(), 20.),
                        );
                        ui.add_space(90.);
                        egui::DragAndDrop::set_payload(ctx, Section::Scopes);
                        let zone = egui::Rect::from_min_max(
                            header.min,
                            egui::pos2(header.right(), ui.cursor().top()),
                        );
                        ui.push_id("editing", |ui| {
                            assert!(drop_editing(ui, zone, header.center().y).is_none())
                        });
                        assert_eq!(
                            egui::DragAndDrop::payload::<Section>(ctx).as_deref(),
                            Some(&Section::Scopes)
                        );
                        let result = drop(ui, header);
                        if !pressed {
                            assert_eq!(result.map(|(s, _)| s), Some(Section::Scopes));
                        }
                    });
                },
            );
        }
    }
}
