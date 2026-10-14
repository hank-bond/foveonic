//! Shares button, checkbox, and dropdown states across the workspace.

use eframe::egui;

const RADIUS: u8 = 4;

pub fn install_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::from_gray(22);
    visuals.window_fill = egui::Color32::from_gray(22);
    visuals.extreme_bg_color = egui::Color32::from_gray(12);
    visuals.selection.bg_fill = egui::Color32::from_rgb(35, 65, 100);
    visuals.selection.stroke = egui::Stroke::new(1_f32, egui::Color32::from_rgb(100, 160, 245));
    for (widget, fill, border) in [
        (&mut visuals.widgets.inactive, 29, 70),
        (&mut visuals.widgets.hovered, 34, 130),
        (&mut visuals.widgets.active, 24, 170),
        (&mut visuals.widgets.open, 34, 130),
        (&mut visuals.widgets.noninteractive, 22, 50),
    ] {
        widget.bg_fill = egui::Color32::from_gray(fill);
        widget.weak_bg_fill = widget.bg_fill;
        widget.bg_stroke = egui::Stroke::new(1_f32, egui::Color32::from_gray(border));
        widget.corner_radius = egui::CornerRadius::same(RADIUS);
        widget.expansion = 0.;
    }
    visuals.collapsing_header_frame = true;
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(7., 6.);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.));
    ctx.set_style(style);
}

pub fn visuals(ui: &egui::Ui, response: &egui::Response, on: bool) -> egui::style::WidgetVisuals {
    let widgets = &ui.visuals().widgets;
    let pressed = response.is_pointer_button_down_on() || response.clicked();
    let hovered = response.hovered() || response.has_focus() || response.highlighted();
    let mut visuals = if !response.enabled() {
        widgets.noninteractive
    } else if pressed {
        widgets.active
    } else if hovered {
        widgets.hovered
    } else {
        widgets.inactive
    };
    visuals.corner_radius = egui::CornerRadius::same(RADIUS);
    if on {
        visuals.bg_fill = if pressed {
            egui::Color32::from_rgb(24, 45, 72)
        } else {
            ui.visuals().selection.bg_fill
        };
        visuals.weak_bg_fill = visuals.bg_fill;
        visuals.bg_stroke = egui::Stroke::new(
            1_f32,
            if hovered && response.enabled() {
                egui::Color32::from_rgb(140, 190, 255)
            } else {
                ui.visuals().selection.stroke.color
            },
        );
    }
    visuals
}

fn frame(ui: &egui::Ui, response: &egui::Response, on: bool, rect: egui::Rect) -> egui::Shape {
    let visuals = visuals(ui, response, on);
    egui::Shape::Rect(egui::epaint::RectShape::new(
        rect,
        visuals.corner_radius,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    ))
}

pub fn paint(ui: &egui::Ui, response: &egui::Response, on: bool) {
    ui.painter().add(frame(ui, response, on, response.rect));
}

pub fn button(ui: &mut egui::Ui, label: &str, on: bool, minimum: egui::Vec2) -> egui::Response {
    // egui's selected frames have square corners and fixed colors. Paint our
    // frame behind its text while keeping native layout and input handling.
    let background = ui.painter().add(egui::Shape::Noop);
    let response = ui.add(egui::Button::new(label).frame(false).min_size(minimum));
    ui.painter()
        .set(background, frame(ui, &response, on, response.rect));
    response
}

pub fn checkbox(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    let background = ui.painter().add(egui::Shape::Noop);
    let response = ui
        .scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            for visual in [
                &mut widgets.inactive,
                &mut widgets.hovered,
                &mut widgets.active,
                &mut widgets.noninteractive,
            ] {
                visual.bg_fill = egui::Color32::TRANSPARENT;
                visual.bg_stroke = egui::Stroke::NONE;
            }
            ui.checkbox(value, label)
        })
        .inner;
    let (_, icon) = ui.spacing().icon_rectangles(response.rect);
    ui.painter()
        .set(background, frame(ui, &response, false, icon));
    response
}

pub fn selectable_value<T: PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    choice: T,
    label: &str,
) -> egui::Response {
    let mut response = button(ui, label, *value == choice, egui::Vec2::ZERO);
    if response.clicked() && *value != choice {
        *value = choice;
        response.mark_changed();
    }
    response
}

pub fn menu_cell(ui: &egui::Ui, response: &egui::Response, selected: bool) -> egui::Color32 {
    let hovered = response.enabled()
        && (response.hovered() || response.has_focus() || response.highlighted());
    let background = if selected {
        ui.visuals().selection.bg_fill
    } else {
        egui::Color32::from_gray(if hovered { 60 } else { 29 })
    };
    ui.painter()
        .rect_filled(response.rect, egui::CornerRadius::ZERO, background);
    egui::Color32::from_gray(if hovered { 240 } else { 180 })
}

pub fn menu_row(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let padding = ui.spacing().button_padding;
    let galley = egui::WidgetText::from(label).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        ui.available_width(),
        egui::TextStyle::Button,
    );
    let size = egui::vec2(
        ui.available_width().max(galley.size().x + 2. * padding.x),
        ui.spacing()
            .interact_size
            .y
            .max(galley.size().y + 2. * padding.y),
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let color = menu_cell(ui, &response, selected);
    ui.painter().galley(
        egui::pos2(
            rect.left() + padding.x,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    response
}

pub fn menu_value<T: PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    choice: T,
    label: &str,
) -> egui::Response {
    let mut response = menu_row(ui, label, *value == choice);
    if response.clicked() && *value != choice {
        *value = choice;
        response.mark_changed();
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(
        ctx: &egui::Context,
        on: bool,
        enabled: bool,
        time: f64,
        events: Vec<egui::Event>,
    ) -> (egui::Response, egui::style::WidgetVisuals, egui::Color32) {
        let mut result = None;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 100.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        let response = button(ui, "A", on, egui::vec2(22., 20.));
                        let visual = visuals(ui, &response, on);
                        result = Some((response, visual));
                    });
                });
            },
        );
        let (response, visual) = result.unwrap();
        let fill = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.rect.contains(response.rect.center())
                        && rect.rect.width() < 30.
                        && rect.stroke.width > 0. =>
                {
                    assert_eq!(rect.corner_radius, egui::CornerRadius::same(RADIUS));
                    Some(rect.fill)
                }
                _ => None,
            })
            .unwrap();
        (response, visual, fill)
    }

    #[test]
    fn dropdown_cells_are_full_width_square_borderless_and_use_selection_hover_colors() {
        for selected in [false, true] {
            let ctx = egui::Context::default();
            install_style(&ctx);
            let render = |time, events| {
                let mut rect = egui::Rect::NOTHING;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400., 200.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.;
                            let width = ui.available_width();
                            rect = menu_row(ui, "Option", selected).rect;
                            assert_eq!(rect.width(), width);
                            let next = menu_row(ui, "Next", false).rect;
                            assert_eq!(next.top(), rect.bottom());
                            assert_eq!(next.width(), rect.width());
                        });
                    },
                );
                let background = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(shape) if shape.rect == rect => {
                            assert_eq!(shape.corner_radius, egui::CornerRadius::ZERO);
                            assert_eq!(shape.stroke, egui::Stroke::NONE);
                            Some(shape.fill)
                        }
                        _ => None,
                    })
                    .unwrap();
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == "Option" => {
                            Some(text.fallback_color)
                        }
                        _ => None,
                    })
                    .unwrap();
                (rect, background, text)
            };
            render(0., vec![]);
            let (rect, background, text) = render(0.02, vec![]);
            assert_eq!(
                background,
                if selected {
                    egui::Color32::from_rgb(35, 65, 100)
                } else {
                    egui::Color32::from_gray(29)
                }
            );
            assert_eq!(text, egui::Color32::from_gray(180));
            let (_, background, text) =
                render(0.04, vec![egui::Event::PointerMoved(rect.center())]);
            assert_eq!(
                background,
                if selected {
                    egui::Color32::from_rgb(35, 65, 100)
                } else {
                    egui::Color32::from_gray(60)
                }
            );
            assert_eq!(text, egui::Color32::from_gray(240));
        }
    }

    #[test]
    fn dropdown_cell_edges_select_values_and_keep_changed_signals() {
        let ctx = egui::Context::default();
        install_style(&ctx);
        let mut value = 0;
        let mut render = |time, events| {
            let mut result = None;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 200.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.set_width(260.);
                        result = Some(menu_value(ui, &mut value, 1, "Choice"));
                    });
                },
            );
            (result.unwrap(), value)
        };
        let (response, _) = render(0., vec![]);
        let point = response.rect.right_center() - egui::vec2(2., 0.);
        render(0.02, vec![egui::Event::PointerMoved(point)]);
        let event = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        assert!(!render(0.04, vec![event(true)]).0.changed());
        let (response, value) = render(0.06, vec![event(false)]);
        assert!(response.changed());
        assert_eq!(value, 1);
    }

    #[test]
    fn neutral_and_accent_states_keep_rounding_hover_press_and_disabled_feedback() {
        for on in [false, true] {
            let ctx = egui::Context::default();
            install_style(&ctx);
            let (response, normal, normal_fill) = render(&ctx, on, true, 0., vec![]);
            assert_eq!(response.rect.size(), egui::vec2(22., 20.));
            assert_eq!(
                normal.bg_fill,
                if on {
                    egui::Color32::from_rgb(35, 65, 100)
                } else {
                    egui::Color32::from_gray(29)
                }
            );
            let point = response.rect.center();
            let (_, hovered, _) =
                render(&ctx, on, true, 1., vec![egui::Event::PointerMoved(point)]);
            assert_ne!(normal.bg_stroke, hovered.bg_stroke);
            let event = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            let (_, pressed, _) = render(&ctx, on, true, 1.1, vec![event(true)]);
            assert_eq!(
                pressed.bg_fill,
                if on {
                    egui::Color32::from_rgb(24, 45, 72)
                } else {
                    egui::Color32::from_gray(24)
                }
            );
            assert!(render(&ctx, on, true, 1.2, vec![event(false)]).0.clicked());
            let (disabled, _, disabled_fill) = render(&ctx, on, false, 2., vec![event(true)]);
            assert!(!disabled.enabled());
            assert!(!disabled.clicked());
            assert_ne!(normal_fill, disabled_fill);
            assert!(!render(&ctx, on, false, 2.1, vec![event(false)]).0.clicked());
        }
    }

    #[test]
    fn keyboard_focus_uses_hover_and_enter_still_activates_buttons() {
        let ctx = egui::Context::default();
        install_style(&ctx);
        let (response, _, _) = render(&ctx, true, true, 0., vec![]);
        ctx.memory_mut(|memory| memory.request_focus(response.id));
        let (response, focused, _) = render(&ctx, true, true, 1., vec![]);
        assert!(response.has_focus());
        assert_eq!(focused.bg_fill, egui::Color32::from_rgb(35, 65, 100));
        assert_eq!(
            focused.bg_stroke.color,
            egui::Color32::from_rgb(140, 190, 255)
        );
        let (response, pressed, _) = render(
            &ctx,
            true,
            true,
            1.1,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(response.clicked());
        assert_eq!(pressed.bg_fill, egui::Color32::from_rgb(24, 45, 72));
    }

    #[test]
    fn checkboxes_stay_neutral_and_keep_checkmarks_and_native_changed_signals() {
        let ctx = egui::Context::default();
        install_style(&ctx);
        let mut checked = false;
        let mut render = |time, events| {
            let mut result = None;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 100.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = checkbox(ui, &mut checked, "Enabled");
                        result = Some((response, checked));
                    });
                },
            );
            let (response, checked) = result.unwrap();
            let checkmark = output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                    egui::Shape::Path(path) if path.points.len() == 3 && path.stroke.width > 0.
                )
            });
            assert_eq!(checkmark, checked);
            let fill = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.rect.width() < 30. && rect.stroke.width > 0. =>
                    {
                        assert_eq!(rect.corner_radius, egui::CornerRadius::same(RADIUS));
                        assert_eq!(rect.stroke.color.r(), rect.stroke.color.g());
                        assert_eq!(rect.stroke.color.g(), rect.stroke.color.b());
                        assert_eq!(rect.fill.r(), rect.fill.g());
                        assert_eq!(rect.fill.g(), rect.fill.b());
                        Some(rect.fill)
                    }
                    _ => None,
                })
                .unwrap();
            (response, checked, fill)
        };
        let (response, value, fill) = render(0., vec![]);
        assert!(!value);
        assert_eq!(fill, egui::Color32::from_gray(29));
        let point = response.rect.center();
        assert_eq!(
            render(1., vec![egui::Event::PointerMoved(point)]).2,
            egui::Color32::from_gray(34)
        );
        let event = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let (response, value, fill) = render(1.1, vec![event(true)]);
        assert!(!response.changed());
        assert!(!value);
        assert_eq!(fill, egui::Color32::from_gray(24));
        let (response, value, fill) = render(1.2, vec![event(false)]);
        assert!(response.changed());
        assert!(value);
        assert_eq!(fill, egui::Color32::from_gray(24));
        let (response, value, fill) = render(1.3, vec![]);
        assert!(!response.changed());
        assert!(value);
        assert_eq!(fill, egui::Color32::from_gray(34));
        ctx.memory_mut(|memory| memory.surrender_focus(response.id));
        let (_, value, fill) = render(1.4, vec![egui::Event::PointerMoved(egui::pos2(350., 80.))]);
        assert!(value);
        assert_eq!(fill, egui::Color32::from_gray(29));
    }
}
