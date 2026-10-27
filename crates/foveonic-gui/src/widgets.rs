//! Shares button, checkbox, and dropdown states across the workspace.

use eframe::egui;

const VALUE_SIGNIFICANT_DIGITS: usize = 3;
/// Compact photographic readouts retain up to three significant digits.
/// Small or large magnitudes use scientific notation instead of long zero runs.
pub(crate) fn format_parameter_value(value: f64) -> String {
    if value == 0. {
        return "0.00".into();
    }
    if !value.is_finite() {
        return value.to_string();
    }
    // Read the exponent after rounding so carries such as 9.999 -> 10.0
    // choose the correct number of decimal places too.
    let scientific = format!(
        "{value:.precision$e}",
        precision = VALUE_SIGNIFICANT_DIGITS - 1
    );
    let (mantissa, exponent) = scientific.split_once('e').expect("formatted exponent");
    let exponent: i32 = exponent.parse().expect("numeric exponent");
    if !(-3..VALUE_SIGNIFICANT_DIGITS as i32).contains(&exponent) {
        return format!(
            "{}e{exponent}",
            mantissa.trim_end_matches('0').trim_end_matches('.')
        );
    }
    let mut decimals = (VALUE_SIGNIFICANT_DIGITS as i32 - 1 - exponent) as usize;
    let mut text = format!("{value:.decimals$}");
    while decimals > 2 && text.ends_with('0') {
        text.pop();
        decimals -= 1;
    }
    text
}

const RADIUS: u8 = 4;
pub const BUTTON_GROUP_MARGIN: i8 = 4;
pub const PARAMETER_GROUP_HORIZONTAL_PADDING: i8 = 2 * BUTTON_GROUP_MARGIN;
pub const MENU_BACKGROUND: egui::Color32 = egui::Color32::from_gray(29);
/// Keep slider tracks distinct from both linked groups and the main panel.
pub const SLIDER_TRACK_FILL: egui::Color32 = egui::Color32::from_gray(60);

/// Give related controls a shared padded background without changing their spacing.
pub fn button_group<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    linked_group(ui, egui::Margin::same(BUTTON_GROUP_MARGIN), contents)
}

/// Keep paired parameter rows roomy at the sides without increasing row gaps.
pub fn parameter_group<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    linked_group(
        ui,
        egui::Margin::symmetric(PARAMETER_GROUP_HORIZONTAL_PADDING, BUTTON_GROUP_MARGIN),
        contents,
    )
}

fn linked_group<R>(
    ui: &mut egui::Ui,
    padding: egui::Margin,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::new()
        .inner_margin(padding)
        .corner_radius(6)
        .fill(MENU_BACKGROUND)
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), contents)
                .inner
        })
}

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
    // Built-in DragValue hints print the full float independently of its
    // formatter. Domain help stays available through our explicit tooltips.
    style.explanation_tooltips = false;
    style.number_formatter = egui::style::NumberFormatter::new(|value, decimals| {
        if *decimals.end() == 0 && value.abs() < 1000. {
            format!("{value:.0}")
        } else {
            format_parameter_value(value)
        }
    });
    style.spacing.item_spacing = egui::vec2(7., 6.);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.));
    // About two lowercase characters in this proportional font, split evenly
    // around the indentation pipe.
    style.spacing.indent = egui::TextStyle::Body.resolve(&style).size;
    style.visuals.indent_has_left_vline = true;
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

pub fn button_padding(ui: &egui::Ui) -> egui::Vec2 {
    let style = ui.style();
    let em = style.override_font_id.as_ref().map_or_else(
        || {
            style
                .override_text_style
                .as_ref()
                .unwrap_or(&egui::TextStyle::Button)
                .resolve(style)
                .size
        },
        |font| font.size,
    );
    egui::vec2(0.5 * em, 0.25 * em)
}

/// Balance the leading gap with the layout's automatic trailing gap, then
/// reserve the full control height before laying out the first label.
pub fn control_row<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.add_space(ui.spacing().item_spacing.y);
    centered_control_row(ui, contents)
}

/// Center labels with padded controls, leaving outer spacing to the container.
pub fn centered_control_row<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let height = control_height(ui);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), height),
        egui::Layout::left_to_right(egui::Align::Center),
        contents,
    )
}

/// Reserve padded control height before placing centered labels or groups.
pub fn control_height(ui: &egui::Ui) -> f32 {
    let text_height = |style| {
        egui::WidgetText::from("M")
            .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, style)
            .size()
            .y
    };
    (text_height(egui::TextStyle::Button) + 2. * button_padding(ui).y)
        .max(text_height(egui::TextStyle::Body))
        .max(ui.spacing().interact_size.y)
}

pub fn button_group_height(ui: &egui::Ui) -> f32 {
    control_height(ui) + 2. * f32::from(BUTTON_GROUP_MARGIN)
}

/// Fit the longest label plus the dropdown arrow and normal control padding.
pub fn dropdown_width<'a>(ui: &egui::Ui, labels: impl IntoIterator<Item = &'a str>) -> f32 {
    let longest = labels
        .into_iter()
        .map(|label| {
            egui::WidgetText::from(label)
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::TextStyle::Button,
                )
                .size()
                .x
        })
        .fold(0_f32, f32::max);
    longest + 2. * button_padding(ui).x + ui.spacing().icon_width + ui.spacing().icon_spacing
}

pub fn dropdown<R>(
    ui: &mut egui::Ui,
    combo: egui::ComboBox,
    menu_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    // ComboBox creates an inner horizontal row using interact_size.y before
    // measuring its text. Reserve the padded height so that row cannot start
    // lower than adjacent buttons. Keep the parent UI and popup IDs stable.
    let previous = ui.spacing().clone();
    ui.spacing_mut().button_padding = button_padding(ui);
    ui.spacing_mut().interact_size.y = control_height(ui);
    let response = combo.show_ui(ui, menu_contents);
    ui.spacing_mut().button_padding = previous.button_padding;
    ui.spacing_mut().interact_size = previous.interact_size;
    response
}

pub fn button(ui: &mut egui::Ui, label: &str, on: bool, minimum: egui::Vec2) -> egui::Response {
    // egui removes padding from frameless buttons. Keep its frame invisible
    // so we can use padded layout and paint our own border and colors.
    let background = ui.painter().add(egui::Shape::Noop);
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = button_padding(ui);
            ui.add(
                egui::Button::new(label)
                    .frame(true)
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE)
                    .min_size(minimum),
            )
        })
        .inner;
    ui.painter()
        .set(background, frame(ui, &response, on, response.rect));
    response
}

pub fn checkbox(
    ui: &mut egui::Ui,
    value: &mut bool,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
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
    } else if hovered {
        egui::Color32::from_gray(60)
    } else {
        MENU_BACKGROUND
    };
    ui.painter()
        .rect_filled(response.rect, egui::CornerRadius::ZERO, background);
    egui::Color32::from_gray(if hovered { 240 } else { 180 })
}

pub fn menu_row(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let padding = button_padding(ui);
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

    #[test]
    fn indentation_reserves_one_body_em_total_with_a_centered_pipe() {
        let ctx = egui::Context::default();
        install_style(&ctx);
        let mut parent_left = 0.;
        let mut child_left = 0.;
        let mut em = 0.;
        let output = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                em = egui::TextStyle::Body.resolve(ui.style()).size;
                parent_left = ui.available_rect_before_wrap().left();
                ui.indent("indentation-spacing", |ui| {
                    child_left = ui.available_rect_before_wrap().left();
                    ui.label("Child");
                });
            });
        });
        assert_eq!(child_left - parent_left, em);
        assert!(output.shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::LineSegment { points, .. } => {
                points[0].x == parent_left + 0.5 * em && points[1].x == parent_left + 0.5 * em
            }
            _ => false,
        }));
    }

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
    fn file_count_and_zoom_rows_center_labels_and_buttons_from_the_first_frame() {
        for em in [13., 20.] {
            for (label, buttons) in [
                ("16 files", &["Icons", "List"][..]),
                ("Zoom 41%", &["100%", "200%", "400%"][..]),
            ] {
                let ctx = egui::Context::default();
                install_style(&ctx);
                ctx.style_mut(|style| {
                    style.override_font_id = Some(egui::FontId::proportional(em))
                });
                let mut label_rect = egui::Rect::NOTHING;
                let mut button_rects = Vec::new();
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        control_row(ui, |ui| {
                            label_rect = ui.label(label).rect;
                            for name in buttons {
                                button_rects.push(button(ui, name, false, egui::Vec2::ZERO).rect);
                            }
                        });
                    });
                });
                for rect in button_rects {
                    assert!((rect.center().y - label_rect.center().y).abs() < 0.1);
                }
                let text_center = |label: &str| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.job.text == label => {
                                Some(text.pos.y + text.galley.size().y * 0.5)
                            }
                            _ => None,
                        })
                        .unwrap()
                };
                for name in buttons {
                    assert!((text_center(name) - text_center(label)).abs() < 1.);
                }
            }
        }
    }

    #[test]
    fn file_count_and_zoom_bands_balance_their_surrounding_vertical_spacing() {
        for pixels_per_point in [1., 2.] {
            for em in [13., 20.] {
                let ctx = egui::Context::default();
                install_style(&ctx);
                ctx.set_pixels_per_point(pixels_per_point);
                ctx.style_mut(|style| {
                    style.override_font_id = Some(egui::FontId::proportional(em))
                });
                let _ = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        for label in ["16 files", "Zoom 50%"] {
                            let before = ui.next_widget_position().y;
                            let gap = ui.spacing().item_spacing.y;
                            let row = control_row(ui, |ui| {
                                ui.label(label);
                                button(ui, "Mode", false, egui::Vec2::ZERO);
                            })
                            .response
                            .rect;
                            let after = ui.next_widget_position().y;
                            assert!((row.top() - before - gap).abs() < 0.01);
                            assert!((after - row.bottom() - gap).abs() < 0.01);
                        }
                    });
                });
            }
        }
    }

    #[test]
    fn buttons_pad_text_in_font_units_without_changing_item_spacing() {
        for em in [14., 20.] {
            let ctx = egui::Context::default();
            install_style(&ctx);
            let mut rect = egui::Rect::NOTHING;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.style_mut().override_font_id = Some(egui::FontId::proportional(em));
                    ui.horizontal(|ui| {
                        let spacing = ui.spacing().clone();
                        rect = button(ui, "Padding", false, egui::Vec2::ZERO).rect;
                        let next = button(ui, "Next", true, egui::Vec2::ZERO).rect;
                        assert!((next.left() - rect.right() - spacing.item_spacing.x).abs() < 0.01);
                        assert_eq!(ui.spacing().button_padding, spacing.button_padding);
                        assert_eq!(ui.spacing().item_spacing, spacing.item_spacing);
                    });
                });
            });
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "Padding" => Some(text),
                    _ => None,
                })
                .unwrap();
            let size = text.galley.size();
            assert!((rect.width() - size.x - em).abs() < 1.);
            assert!((rect.height() - size.y - 0.5 * em).abs() < 1.);
            let bounds = egui::Rect::from_min_size(text.pos, size);
            for gap in [bounds.left() - rect.left(), rect.right() - bounds.right()] {
                assert!((gap - 0.5 * em).abs() < 1.);
            }
            for gap in [bounds.top() - rect.top(), rect.bottom() - bounds.bottom()] {
                assert!((gap - 0.25 * em).abs() < 1.);
            }
        }
    }

    #[test]
    fn dropdowns_share_button_padding_without_changing_ids_or_parent_spacing() {
        for em in [14., 20.] {
            for enabled in [false, true] {
                let ctx = egui::Context::default();
                install_style(&ctx);
                let mut rect = egui::Rect::NOTHING;
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.style_mut().override_font_id = Some(egui::FontId::proportional(em));
                        ui.add_enabled_ui(enabled, |ui| {
                            centered_control_row(ui, |ui| {
                                let spacing = ui.spacing().clone();
                                let before = button(ui, "Before", false, egui::Vec2::ZERO).rect;
                                let id = ui.make_persistent_id(egui::Id::new("padded dropdown"));
                                let response = dropdown(
                                    ui,
                                    egui::ComboBox::from_id_salt("padded dropdown")
                                        .selected_text("Padding"),
                                    |_| {},
                                )
                                .response;
                                assert_eq!(response.id, id);
                                assert_eq!(response.enabled(), enabled);
                                rect = response.rect;
                                let next = button(ui, "Next", false, egui::Vec2::ZERO).rect;
                                assert!((next.height() - rect.height()).abs() < 1.);
                                assert!(
                                    (before.center().y - rect.center().y).abs() < 0.5,
                                    "before={before:?} dropdown={rect:?} next={next:?}"
                                );
                                assert!((next.center().y - rect.center().y).abs() < 0.5);
                                assert!(
                                    (next.left() - rect.right() - spacing.item_spacing.x).abs()
                                        < 0.01
                                );
                                assert_eq!(ui.spacing().button_padding, spacing.button_padding);
                                assert_eq!(ui.spacing().item_spacing, spacing.item_spacing);
                                assert_eq!(ui.spacing().interact_size, spacing.interact_size);
                            });
                        });
                    });
                });
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == "Padding" => Some(text),
                        _ => None,
                    })
                    .unwrap();
                let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                assert!((bounds.left() - rect.left() - 0.5 * em).abs() < 1.);
                for gap in [bounds.top() - rect.top(), rect.bottom() - bounds.bottom()] {
                    assert!((gap - 0.25 * em).abs() < 1.);
                }
            }
        }
    }

    #[test]
    fn open_dropdown_options_keep_font_based_padding_and_gap_free_rows() {
        for em in [14., 20.] {
            let ctx = egui::Context::default();
            install_style(&ctx);
            // Popup areas use the context's font style rather than local overrides.
            ctx.style_mut(|style| {
                style
                    .text_styles
                    .insert(egui::TextStyle::Button, egui::FontId::proportional(em));
            });
            let draw = |time| {
                let mut response = None;
                let output = ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let spacing = ui.spacing().clone();
                            response = Some(dropdown(
                                ui,
                                egui::ComboBox::from_id_salt("padded options")
                                    .selected_text("Closed"),
                                |ui| {
                                    ui.spacing_mut().item_spacing.y = 0.;
                                    let first = menu_row(ui, "Option", false).rect;
                                    let next = menu_row(ui, "Next", true).rect;
                                    assert_eq!(first.width(), next.width());
                                    assert_eq!(first.bottom(), next.top());
                                    first
                                },
                            ));
                            assert_eq!(ui.spacing().button_padding, spacing.button_padding);
                            assert_eq!(ui.spacing().item_spacing, spacing.item_spacing);
                        });
                    },
                );
                (response.unwrap(), output.shapes)
            };
            let (closed, _) = draw(0.);
            ctx.memory_mut(|memory| memory.open_popup(closed.response.id.with("popup")));
            draw(0.02);
            let (open, shapes) = draw(0.04);
            let rect = open.inner.unwrap();
            let text = shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "Option" => Some(text),
                    _ => None,
                })
                .unwrap();
            assert_eq!(text.galley.job.sections[0].format.font_id.size, em);
            let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
            assert!(
                (bounds.left() - rect.left() - 0.5 * em).abs() < 1.,
                "{em} pt dropdown: option bounds {bounds:?}, row {rect:?}, font {:?}",
                text.galley.job.sections[0].format.font_id,
            );
            for gap in [bounds.top() - rect.top(), rect.bottom() - bounds.bottom()] {
                assert!((gap - 0.25 * em).abs() < 1.);
            }
        }
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
            assert!(response.rect.width() >= 22.);
            assert!(response.rect.height() >= 20.);
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
