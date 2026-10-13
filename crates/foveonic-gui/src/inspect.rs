//! Groups pipeline buffers and component views in one inspection selector without scheduling component-only changes.

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
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
        egui::Sense::click(),
    );
    if response.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    let visuals = ui.style().interact_selectable(&response, selected);
    ui.painter()
        .rect_filled(rect, visuals.corner_radius, visuals.bg_fill);
    let padding = ui.spacing().button_padding.x;
    ui.painter().text(
        rect.left_center() + egui::vec2(padding, 0.),
        egui::Align2::LEFT_CENTER,
        name,
        egui::TextStyle::Button.resolve(ui.style()),
        visuals.text_color(),
    );
    sections::disclosure_icon(
        ui,
        rect.right_center() - egui::vec2(padding + 4., 0.),
        !open,
        visuals.text_color(),
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
    egui::ComboBox::from_id_salt("artifact")
        .selected_text(selected)
        .width(205.)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            for image in outputs {
                if image.channels == Channels::Scalar {
                    if ui
                        .selectable_label(image.name == output, image.name)
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
                            if ui
                                .selectable_label(
                                    image.name == output && component == *channel,
                                    label,
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
pub struct Difference {
    pub enabled: bool,
    pub gain: f32,
}
impl Default for Difference {
    fn default() -> Self {
        Self {
            enabled: false,
            gain: 32.,
        }
    }
}
impl Difference {
    /// Returns true only when the user newly requests a comparison pair.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        settings: &Settings,
        output: &str,
        channel: i32,
    ) -> bool {
        let before = self.enabled;
        let input = inspection::comparison_input(settings, output);
        let available = input.is_some() && channel >= 0;
        if !available {
            self.enabled = false;
        }
        let explanation = if input.is_some() && channel < 0 {
            "Choose an individual RGB channel for a signed difference. Combined RGB change heatmaps are not available yet."
        } else if input.is_none() {
            "This view has no same-unit operation-input comparison yet. Original readings and diagnostic maps stand alone; color and tone conversion change the signal's meaning or units."
        } else {
            "Shows output minus this operation's input. Middle gray is unchanged; lighter means increased and darker means decreased. This changes display only, not editing or export. Scopes still measure the output."
        };
        ui.add_enabled(
            available,
            egui::Checkbox::new(&mut self.enabled, "Δ Difference"),
        )
        .on_disabled_hover_text(explanation)
        .on_hover_text(explanation);
        if self.enabled {
            ui.label("Amplify");
            ui.add(egui::DragValue::new(&mut self.gain).range(1. ..=1024.).speed(1.).suffix("×"))
                .on_hover_text("Magnifies small differences for display only. Amplification can clip this diagnostic view without clipping the photograph. At 1×, a full nominal signal-range change reaches black or white.");
        }
        !before && self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn difference_toggle_requests_once_and_unsupported_views_disable_it() {
        let ctx = egui::Context::default();
        let settings = Settings::new("Sunlight", "Standard");
        let mut difference = Difference::default();
        let draw =
            |difference: &mut Difference, output: &str, channel: i32, events: Vec<egui::Event>| {
                let mut requested = false;
                let mut rect = egui::Rect::NOTHING;
                let _ = ctx.run(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            rect = ui
                                .horizontal(|ui| {
                                    requested = difference.show(ui, &settings, output, channel);
                                })
                                .response
                                .rect;
                        });
                    },
                );
                (requested, rect)
            };
        let (_, rect) = draw(&mut difference, "repair.layers", 0, vec![]);
        let point = rect.left_center() + egui::vec2(10., 0.);
        let click = |difference: &mut Difference, output, channel| {
            draw(
                difference,
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
                difference,
                output,
                channel,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            )
            .0
        };
        assert!(click(&mut difference, "repair.layers", 0));
        assert!(difference.enabled);
        assert!(!draw(&mut difference, "repair.layers", 1, vec![]).0);
        assert!(!click(&mut difference, "repair.layers", 1));
        assert!(!difference.enabled);
        difference.enabled = true;
        draw(&mut difference, "color.rgb", 0, vec![]);
        assert!(!difference.enabled);
        difference.enabled = true;
        draw(&mut difference, "saturation.rgb", -1, vec![]);
        assert!(!difference.enabled);
        assert!(!click(&mut difference, "sensor.layers", 0));
        assert!(!difference.enabled);
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
