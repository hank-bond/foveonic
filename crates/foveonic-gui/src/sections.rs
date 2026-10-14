//! Draws shared disclosure triangles and tool-section headers, accepting section drops only inside the tools pane.
use crate::preferences::Section;
use eframe::egui;

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
    for x in [0., 4.] {
        for y in [-4., 0., 4.] {
            ui.painter().circle_filled(
                rect.right_center() + egui::vec2(x - 14., y),
                1.,
                egui::Color32::GRAY,
            );
        }
    }
    response.dnd_set_drag_payload(section);
    response
}
pub fn drop(ui: &mut egui::Ui, header: egui::Rect) -> Option<(Section, bool)> {
    let zone = egui::Rect::from_min_max(header.min, egui::pos2(header.right(), ui.cursor().top()));
    let response = ui.interact(zone, ui.id().with("section drop"), egui::Sense::hover());
    let after = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| p.y > zone.center().y);
    if response.dnd_hover_payload::<Section>().is_some() {
        let y = if after { zone.bottom() } else { zone.top() };
        ui.painter().line_segment(
            [egui::pos2(zone.left(), y), egui::pos2(zone.right(), y)],
            egui::Stroke::new(2_f32, egui::Color32::from_rgb(85, 145, 235)),
        );
    }
    response
        .dnd_release_payload::<Section>()
        .map(|s| (*s, after))
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
                            ui.add_space(7.);
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
