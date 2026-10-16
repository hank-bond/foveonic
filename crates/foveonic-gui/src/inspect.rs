//! Groups pipeline views and controls display-only Input/Output and Difference comparisons.

use crate::{help, sections};
use eframe::egui;
use foveonic::{
    inspection::{self, Channels},
    render::Settings,
};

pub fn components(channels: Channels) -> &'static [(i32, &'static str)] {
    match channels {
        Channels::Layers => &[(0, "Layer 1 (R)"), (1, "Layer 2 (G)"), (2, "Layer 3 (B)")],
        Channels::Rgb => &[(-1, "RGB"), (0, "Red"), (1, "Green"), (2, "Blue")],
        Channels::Scalar => &[(-1, "Scalar")],
    }
}
fn choose(output: &mut String, channel: &mut i32, name: &str, component: i32) {
    *output = name.into();
    *channel = component;
}

/// A full-width disclosure row. Opening it never selects a processing output.
fn group_row(ui: &mut egui::Ui, name: &str, selected: bool) -> bool {
    let id = ui.make_persistent_id(("inspect group", name));
    let mut open = ui.data_mut(|d| d.get_persisted::<bool>(id).unwrap_or(false));
    let padding = crate::widgets::button_padding(ui);
    let galley = egui::WidgetText::from(name).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        ui.available_width(),
        egui::TextStyle::Button,
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(
            ui.available_width(),
            ui.spacing()
                .interact_size
                .y
                .max(galley.size().y + 2. * padding.y),
        ),
        egui::Sense::click(),
    );
    if response.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    let color = crate::widgets::menu_cell(ui, &response, selected);
    ui.painter().galley(
        egui::pos2(
            rect.left() + padding.x,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
    sections::disclosure_icon(
        ui,
        rect.right_center() - egui::vec2(padding.x + 4., 0.),
        !open,
        color,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, ui.is_enabled(), name)
    });
    response.on_hover_text(help::inspection(name).unwrap());
    open
}

/// Keep a restored recipe's selection valid before its first worker request.
pub fn normalize(settings: &Settings, output: &mut String, channel: &mut i32) {
    let outputs = inspection::image_outputs(settings);
    if !outputs.iter().any(|o| o.name == output) {
        choose(output, channel, "final", -1);
    }
    let current = outputs.iter().find(|o| o.name == output).unwrap();
    let choices = components(current.channels);
    if !choices.iter().any(|(c, _)| c == channel) {
        *channel = choices[0].0;
    }
}

fn menu_width(ui: &egui::Ui, outputs: &[inspection::ImageOutput], selected: &str) -> f32 {
    let stages = crate::widgets::dropdown_width(
        ui,
        std::iter::once(selected).chain(outputs.iter().map(|image| image.name)),
    );
    let components = crate::widgets::dropdown_width(
        ui,
        outputs
            .iter()
            .filter(|image| image.channels != Channels::Scalar)
            .flat_map(|image| components(image.channels).iter().map(|(_, label)| *label)),
    ) + ui.spacing().indent;
    // The popup frame and scrollbar consume width inside the header's span.
    // Keep the disclosure column clear even when a floating scrollbar is visible.
    let spacing = ui.spacing();
    let gutter = spacing.scroll.allocated_width().max(
        spacing.scroll.bar_width
            + spacing.scroll.bar_inner_margin
            + spacing.scroll.bar_outer_margin,
    );
    stages.max(components)
        + f32::from(spacing.menu_margin.left)
        + f32::from(spacing.menu_margin.right)
        + gutter
}

/// Returns true only when processing must produce a different buffer.
pub fn show(
    ui: &mut egui::Ui,
    settings: &Settings,
    output: &mut String,
    channel: &mut i32,
) -> bool {
    let before = output.clone();
    normalize(settings, output, channel);
    let outputs = inspection::image_outputs(settings);
    let current = outputs.iter().find(|o| o.name == output).unwrap();
    let choices = components(current.channels);
    let label = choices.iter().find(|(c, _)| c == channel).unwrap().1;
    let selected = if current.channels == Channels::Scalar {
        output.clone()
    } else {
        format!("{output} · {label}")
    };
    let current_help = help::inspection(current.name).unwrap();
    let width = menu_width(ui, &outputs, &selected);
    let combo = egui::ComboBox::from_id_salt("artifact")
        .selected_text(selected)
        .width(width)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    crate::widgets::dropdown(ui, combo, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            for image in outputs {
                if image.channels == Channels::Scalar {
                    if crate::widgets::menu_row(ui, image.name, image.name == output)
                        .on_hover_text(help::inspection(image.name).unwrap())
                        .clicked()
                    {
                        choose(output, channel, image.name, -1);
                        ui.memory_mut(|memory| memory.close_popup());
                    }
                } else {
                    if group_row(ui, image.name, image.name == output) {
                        ui.indent(image.name, |ui| {
                        for &(component, label) in components(image.channels) {
                            if crate::widgets::menu_row(
                                    ui, label,
                                    image.name == output && component == *channel,
                                )
                                .on_hover_text(if image.channels == Channels::Layers {
                                    "Shows one of the camera's three color-sensitive sensor layers in gray. Each responds to overlapping colors; it is not a finished red, green, or blue channel."
                                } else if component == -1 {
                                    "Shows the normal color view of this processing stage."
                                } else {
                                    "Shows this color channel in gray: brighter means more of that channel. This changes the display only; it does not remove colors from the photo or change the scopes."
                                })
                                .clicked()
                            {
                                choose(output, channel, image.name, component);
                                ui.memory_mut(|memory| memory.close_popup());
                            }
                        }
                        });
                    }
                }
            }
        })
        .response
        .on_hover_text(format!(
            "{}\n\nInspect shows steps used to make the photo. Earlier steps do not include later edits. Changing this view does not change your export: export always uses the finished photo.",
            current_help,
        ));
    before != *output
}

/// Session-only presentation controls, independent of photographic settings.
pub struct Comparison {
    pub input: bool,
    pub difference: bool,
    pub gain: f32,
}
impl Default for Comparison {
    fn default() -> Self {
        Self {
            input: false,
            difference: false,
            gain: 32.,
        }
    }
}
impl Comparison {
    pub fn requested(&self) -> bool {
        self.input || self.difference
    }

    pub fn mapping(&self, channel: i32, has_pair: bool) -> crate::viewport::Mapping {
        let mut mapping = crate::viewport::Mapping::normal(channel);
        if has_pair {
            mapping.input = self.input && !self.difference;
            if self.difference && channel >= 0 {
                mapping.difference_gain = self.gain;
            }
        }
        mapping
    }

    /// Returns true only when the user newly requests a comparison pair.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        settings: &Settings,
        output: &str,
        channel: i32,
    ) -> bool {
        let before = self.requested();
        let input = inspection::comparison_input(settings, output);
        let available = input.is_some() && channel >= 0;
        if input.is_none() {
            self.input = false;
        }
        if !available {
            self.difference = false;
        }
        let side_help = if input.is_none() {
            "This view has no matching input comparison."
        } else if self.difference {
            "Turn Difference off to switch between Input and Output."
        } else {
            "Input shows the image before this step. Output shows it after. Scopes always measure the output. This does not change edits or export."
        };
        ui.add_enabled_ui(input.is_some() && !self.difference, |ui| {
            for (side, label) in [(true, "Input"), (false, "Output")] {
                crate::widgets::selectable_value(ui, &mut self.input, side, label)
                    .on_hover_text(side_help)
                    .on_disabled_hover_text(side_help);
            }
        });
        let explanation = if input.is_some() && channel < 0 {
            "Choose an individual RGB channel for Difference. Input and Output can still show combined RGB."
        } else if input.is_none() {
            "This view has no matching input comparison. Original readings and diagnostic maps stand alone. Color and tone conversion change the meaning or units of the values."
        } else {
            "Shows output minus input. Middle gray means no change. Lighter means increased and darker means decreased. Scopes still measure the output. This does not change edits or export."
        };
        ui.add_enabled_ui(available, |ui| {
            crate::widgets::checkbox(ui, &mut self.difference, "Δ Difference")
        })
        .inner
        .on_disabled_hover_text(explanation)
        .on_hover_text(explanation);
        if self.difference {
            ui.label("Amplify");
            ui.add(egui::DragValue::new(&mut self.gain).range(1. ..=1024.).speed(1.).suffix("×"))
                .on_hover_text("Magnifies small differences for display only. Amplification can clip this diagnostic view without clipping the photograph. At 1×, a full nominal signal-range change reaches black or white.");
        }
        !before && self.requested()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspect_width_follows_available_labels_and_preserves_long_selected_components() {
        for em in [13., 20.] {
            for rendering in [
                foveonic::render::Rendering::Color,
                foveonic::render::Rendering::Monochrome,
            ] {
                for (name, component, selected) in [
                    ("final", -1, "final · RGB"),
                    ("calibration.layers", 0, "calibration.layers · Layer 1 (R)"),
                ] {
                    let ctx = egui::Context::default();
                    crate::widgets::install_style(&ctx);
                    ctx.style_mut(|style| {
                        style.override_font_id = Some(egui::FontId::proportional(em))
                    });
                    let mut settings = Settings::new("Sunlight", "Standard");
                    settings.rendering = rendering;
                    let mut output = name.to_owned();
                    let mut channel = component;
                    let mut expected = 0.;
                    let rendered = ctx.run(egui::RawInput::default(), |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            expected =
                                menu_width(ui, &inspection::image_outputs(&settings), selected);
                            assert!(!show(ui, &settings, &mut output, &mut channel));
                        });
                    });
                    let text = rendered
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.job.text == selected => {
                                Some(text)
                            }
                            _ => None,
                        })
                        .unwrap();
                    let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                    let header = rendered
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Rect(rect)
                                if rect.stroke.width > 0. && rect.rect.contains_rect(bounds) =>
                            {
                                Some(rect.rect)
                            }
                            _ => None,
                        })
                        .min_by(|a, b| a.area().total_cmp(&b.area()))
                        .unwrap();
                    assert!((header.width() - expected).abs() < 1.);
                    if em == 13. && name == "final" {
                        assert!(
                            header.width() < 205.,
                            "default Inspect still reserves its old fixed width"
                        );
                    }
                    assert_eq!(output, name);
                    assert_eq!(channel, component);
                }
            }
        }
    }

    #[test]
    fn scrolling_inspect_popup_keeps_disclosure_arrows_clear_of_long_labels() {
        for em in [13., 20.] {
            let ctx = egui::Context::default();
            crate::widgets::install_style(&ctx);
            ctx.style_mut(|style| style.override_font_id = Some(egui::FontId::proportional(em)));
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.reconstruction = Some(Default::default());
            settings.sharpen_amount = Some(1.);
            let mut output = "final".to_owned();
            let mut channel = -1;
            let mut draw = |events| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 800.),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.spacing_mut().combo_height = 180.;
                            assert!(!show(ui, &settings, &mut output, &mut channel));
                        });
                    },
                )
            };
            let closed = draw(vec![]);
            let point = closed
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "final · RGB" => {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            for pressed in [true, false] {
                draw(vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ]);
            }
            let opened = draw(vec![]);
            let text = opened
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "calibration.layers" => {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
                .unwrap();
            let arrow = opened
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Path(path) if path.closed && path.points.len() == 3 => {
                        let bounds = egui::Rect::from_points(&path.points);
                        ((bounds.center().y - text.center().y).abs() < 1.).then_some(bounds)
                    }
                    _ => None,
                })
                .unwrap();
            assert!(
                arrow.left() - text.right() >= 4.,
                "{em}pt label overlaps disclosure: text={text:?}, arrow={arrow:?}"
            );
        }
    }

    #[test]
    fn popup_disclosure_stays_open_but_component_scalar_and_outside_clicks_close() {
        fn text_rect(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Rect {
            shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing visible text: {label}"))
        }
        let ctx = egui::Context::default();
        let settings = Settings::new("Sunlight", "Standard");
        let mut output = "final".to_owned();
        let mut channel = -1;
        let draw = |output: &mut String, channel: &mut i32, events: Vec<egui::Event>| {
            let mut id = egui::Id::NULL;
            let mut changed = false;
            let result = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 1200.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.spacing_mut().combo_height = 1000.;
                        id = ui.make_persistent_id(egui::Id::new("artifact"));
                        changed = show(ui, &settings, output, channel);
                    });
                },
            );
            (changed, egui::ComboBox::is_open(&ctx, id), result.shapes)
        };
        let click = |output: &mut String, channel: &mut i32, point| {
            draw(
                output,
                channel,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            );
            draw(
                output,
                channel,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            )
        };
        let (_, _, shapes) = draw(&mut output, &mut channel, vec![]);
        let combo = text_rect(&shapes, "final · RGB").center();
        assert!(click(&mut output, &mut channel, combo).1);
        let (_, _, shapes) = draw(&mut output, &mut channel, vec![]);
        let group = text_rect(&shapes, "sensor.layers").center();
        let (changed, open, _) = click(&mut output, &mut channel, group);
        assert!(!changed);
        assert!(open, "expanding a group must not dismiss Inspect");
        assert_eq!((&*output, channel), ("final", -1));
        let (_, open, shapes) = draw(&mut output, &mut channel, vec![]);
        assert!(open);
        assert!(shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Path(path) if path.closed && path.points.len() == 3)));
        let component = text_rect(&shapes, "Layer 2 (G)").center();
        let (_, open, _) = click(&mut output, &mut channel, group);
        assert!(open, "collapsing a group must not dismiss Inspect");
        click(&mut output, &mut channel, group);
        let (changed, open, _) = click(&mut output, &mut channel, component);
        assert!(changed);
        assert!(!open, "choosing a component must dismiss Inspect");
        assert_eq!((&*output, channel), ("sensor.layers", 1));
        let (_, _, shapes) = draw(&mut output, &mut channel, vec![]);
        let combo = text_rect(&shapes, "sensor.layers · Layer 2 (G)").center();
        click(&mut output, &mut channel, combo);
        let (_, _, shapes) = draw(&mut output, &mut channel, vec![]);
        let scalar = text_rect(&shapes, "highlight.density").center();
        let (changed, open, _) = click(&mut output, &mut channel, scalar);
        assert!(changed);
        assert!(!open);
        assert_eq!((&*output, channel), ("highlight.density", -1));
        let (_, _, shapes) = draw(&mut output, &mut channel, vec![]);
        let combo = text_rect(&shapes, "highlight.density").center();
        click(&mut output, &mut channel, combo);
        assert!(draw(&mut output, &mut channel, vec![]).1);
        assert!(!click(&mut output, &mut channel, egui::pos2(550., 1100.)).1);
    }

    #[test]
    fn disclosure_rows_share_option_padding_at_each_font_size() {
        for em in [14., 20.] {
            let ctx = egui::Context::default();
            crate::widgets::install_style(&ctx);
            let mut rect = egui::Rect::NOTHING;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.style_mut().override_font_id = Some(egui::FontId::proportional(em));
                    rect = ui
                        .scope(|ui| group_row(ui, "noise.layers", false))
                        .response
                        .rect;
                });
            });
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "noise.layers" => Some(text),
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

    #[test]
    fn disclosure_opens_and_closes_from_both_edges_of_the_row() {
        let ctx = egui::Context::default();
        let draw = |events: Vec<egui::Event>| {
            let mut open = false;
            let mut rect = egui::Rect::NOTHING;
            let rendered = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(300., 300.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let origin = ui.cursor().min;
                        let width = ui.available_width();
                        let height = ui.spacing().interact_size.y;
                        open = group_row(ui, "repair.layers", false);
                        rect = egui::Rect::from_min_size(origin, egui::vec2(width, height));
                    });
                },
            );
            assert_eq!(rendered.shapes.iter().filter(|shape| {
                matches!(&shape.shape, egui::Shape::Path(path) if path.closed && path.points.len() == 3)
            }).count(), 1, "the disclosure must be a drawn triangle, not a font glyph");
            (open, rect)
        };
        let (open, rect) = draw(vec![]);
        assert!(!open);
        let click = |point| {
            draw(vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ]);
            draw(vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }])
            .0
        };
        assert!(click(rect.left_center() + egui::vec2(5., 0.)));
        assert!(draw(vec![]).0); // state survives subsequent frames
        assert!(!click(rect.right_center() - egui::vec2(5., 0.)));
    }

    #[test]
    fn comparison_buttons_reuse_pairs_restore_the_side_and_support_rgb() {
        let ctx = egui::Context::default();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.saturation = 1.2;
        let mut comparison = Comparison::default();
        let draw = |comparison: &mut Comparison, output: &str, channel: i32, events| {
            let mut requested = false;
            let result = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 400.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            requested = comparison.show(ui, &settings, output, channel);
                        });
                    });
                },
            );
            (requested, result)
        };
        let click = |comparison: &mut Comparison, output, channel, label| {
            let (_, result) = draw(comparison, output, channel, vec![]);
            let point = result
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            let mut requested = false;
            for pressed in [true, false] {
                requested |= draw(
                    comparison,
                    output,
                    channel,
                    vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                )
                .0;
            }
            requested
        };
        assert!(!comparison.requested());
        assert!(!comparison.input);
        assert!(click(&mut comparison, "repair.layers", 0, "Input"));
        assert!(comparison.input);
        assert!(!draw(&mut comparison, "repair.layers", 1, vec![]).0);
        assert!(!click(&mut comparison, "repair.layers", 1, "Δ Difference"));
        assert!(comparison.difference && comparison.input);
        assert!(!click(&mut comparison, "repair.layers", 1, "Output"));
        assert!(comparison.input, "Difference must disable the side buttons");
        assert!(!click(&mut comparison, "repair.layers", 1, "Δ Difference"));
        assert!(!comparison.difference && comparison.input);
        assert!(comparison.mapping(1, true).input);
        assert!(!click(&mut comparison, "repair.layers", 1, "Output"));
        assert!(!comparison.input);
        assert!(click(&mut comparison, "repair.layers", 1, "Δ Difference"));
        assert!(!draw(&mut comparison, "repair.layers", 1, vec![]).0);
        draw(&mut comparison, "color.rgb", 0, vec![]);
        assert!(!comparison.requested());
        assert!(click(&mut comparison, "saturation.rgb", -1, "Input"));
        assert!(comparison.input);
        assert!(!click(
            &mut comparison,
            "saturation.rgb",
            -1,
            "Δ Difference"
        ));
        assert!(!comparison.difference);
        assert!(comparison.mapping(-1, true).input);
        draw(&mut comparison, "sensor.layers", 0, vec![]);
        assert!(!comparison.requested());
        assert!(!click(&mut comparison, "sensor.layers", 0, "Input"));
        assert!(!comparison.requested());
    }

    #[test]
    fn comparison_mapping_keeps_pending_inputs_on_output_and_difference_overrides_the_side() {
        let mut comparison = Comparison::default();
        assert_eq!(
            comparison.mapping(2, true),
            crate::viewport::Mapping::normal(2)
        );
        comparison.input = true;
        assert_eq!(
            comparison.mapping(2, false),
            crate::viewport::Mapping::normal(2)
        );
        let input = comparison.mapping(2, true);
        assert!(input.input);
        assert_eq!(input.channel, 2);
        assert_eq!(input.difference_gain, 0.);
        comparison.difference = true;
        let difference = comparison.mapping(2, true);
        assert!(!difference.input);
        assert_eq!(difference.difference_gain, 32.);
        assert_eq!(
            comparison.mapping(2, false),
            crate::viewport::Mapping::normal(2)
        );
        comparison.difference = false;
        assert_eq!(comparison.mapping(2, true), input);
    }

    #[test]
    fn component_selection_reuses_the_root_and_scalar_views_have_no_layers() {
        let mut output = "final".to_owned();
        let mut channel = -1;
        choose(&mut output, &mut channel, "final", 2);
        assert_eq!((&*output, channel), ("final", 2));
        choose(&mut output, &mut channel, "sensor.layers", 1);
        assert_eq!((&*output, channel), ("sensor.layers", 1));
        assert_eq!(components(Channels::Scalar), &[(-1, "Scalar")]);
        assert_eq!(components(Channels::Layers).len(), 3);
    }
}
