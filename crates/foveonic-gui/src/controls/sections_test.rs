//! Checks editing-tool drops, unchanged image settings and meaningful insertion slots.

use super::*;
fn frame(
    ctx: &egui::Context,
    order: &mut Vec<EditingSection>,
    time: f64,
    events: Vec<egui::Event>,
) -> Vec<(EditingSection, egui::Rect)> {
    let mut headers = Vec::new();
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
                    assert!(!show(
                        ui,
                        order,
                        &[
                            EditingSection::Tone,
                            EditingSection::ColorEditor,
                            EditingSection::Noise
                        ],
                        |ui, section| {
                            let start = ui.cursor().min;
                            let rect = egui::Rect::from_min_size(
                                start,
                                egui::vec2(ui.available_width(), 20.),
                            );
                            let changed =
                                super::super::common::section_header(ui, section, "Tool", None);
                            headers.push((section, rect));
                            ui.add_space(40.);
                            changed
                        }
                    ));
                });
        },
    );
    headers
}
#[test]
fn editing_handles_reorder_without_reporting_an_image_edit() {
    let ctx = egui::Context::default();
    crate::widgets::install_style(&ctx);
    let mut order = EditingSection::ALL.to_vec();
    frame(&ctx, &mut order, 0., vec![]);
    let headers = frame(&ctx, &mut order, 0.1, vec![]);
    let source = headers
        .iter()
        .find(|(s, _)| *s == EditingSection::ColorEditor)
        .unwrap()
        .1
        .right_center()
        - egui::vec2(10., 0.);
    let target = headers
        .iter()
        .find(|(s, _)| *s == EditingSection::Tone)
        .unwrap()
        .1
        .center_top()
        + egui::vec2(0., 2.);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(
        &ctx,
        &mut order,
        1.,
        vec![egui::Event::PointerMoved(source), button(source, true)],
    );
    frame(
        &ctx,
        &mut order,
        1.1,
        vec![egui::Event::PointerMoved(target)],
    );
    frame(&ctx, &mut order, 1.2, vec![button(target, false)]);
    let editor = order
        .iter()
        .position(|s| *s == EditingSection::ColorEditor)
        .unwrap();
    let tone = order
        .iter()
        .position(|s| *s == EditingSection::Tone)
        .unwrap();
    assert!(editor < tone);
}

#[test]
fn existing_positions_and_hidden_tool_only_moves_have_no_drop_target() {
    use EditingSection::*;
    let order = EditingSection::ALL.to_vec();
    let visible = [Tone, ColorEditor, Noise, Sharpening, Corrections];
    assert!(reordered(&order, &visible, ColorEditor, Tone, true).is_none());
    assert!(reordered(&order, &visible, ColorEditor, Noise, false).is_none());
    assert!(reordered(&order, &visible, ColorEditor, ColorEditor, true).is_none());
    assert!(reordered(&order, &visible, Noise, Sharpening, false).is_none());
    let next = reordered(&order, &visible, ColorEditor, Tone, false).unwrap();
    let next: Vec<_> = next.into_iter().filter(|s| visible.contains(s)).collect();
    assert_eq!(next, [ColorEditor, Tone, Noise, Sharpening, Corrections]);
}
