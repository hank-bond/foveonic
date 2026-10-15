//! Displays advanced reconstruction controls with automatic ColorDQ and preset-linked parameter overrides.

use eframe::egui;
use foveonic::{
    noise::{self, Controls, Parameter},
    source::Metadata,
};

pub fn link_button(ui: &mut egui::Ui, linked: bool, parent: &str) -> egui::Response {
    let response = crate::widgets::button(ui, "", !linked, egui::vec2(22., 20.));
    let rect = response.rect;
    let stroke = egui::Stroke::new(1.3_f32, ui.style().interact(&response).text_color());
    let offset = if linked { 2.5 } else { 5. };
    for sign in [-1., 1.] {
        ui.painter().circle_stroke(
            rect.center() + egui::vec2(sign * offset, sign * 1.5),
            3.5,
            stroke,
        );
    }
    if linked {
        ui.painter().line_segment(
            [
                rect.center() - egui::vec2(3., 2.),
                rect.center() + egui::vec2(3., 2.),
            ],
            stroke,
        );
    }
    response.on_hover_text(if linked {
        format!("Follows {parent}. Click to keep the current value and adjust it separately. Changes to {parent} will then leave it alone.")
    } else {
        format!("Set separately from {parent}. Click to replace this override with the current automatic value and follow {parent} again.")
    })
}

pub fn show(
    ui: &mut egui::Ui,
    controls: &mut Controls,
    color_dq_gain: &mut Option<f32>,
    metadata: Option<&Metadata>,
    wb: &str,
) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("Advanced")
        .id_salt("advanced-reconstruction")
        .default_open(crate::smoke::editor_review_active())
        .show(ui, |ui| {
            changed |= crate::controls::automatic_adjustment(
                ui, "ColorDQ gain", color_dq_gain, 0. ..=2., 1., None, None,
            );
            let calculated = metadata.ok_or_else(|| "Waiting for camera metadata".to_owned())
                .and_then(|m| noise::calculated_values(m, wb, *controls).map_err(|e| e.to_string()));
            let values = match calculated {
                Ok(values) => Some(values),
                Err(error) => { ui.weak(error); None }
            };
            for parameter in Parameter::ALL {
                changed |= row(ui, &mut controls.overrides, parameter, values.map(|v| v[parameter as usize]));
            }
        })
        .header_response
        .on_hover_text("Fine-tunes color cleanup, noise smoothing, and texture restoration. ColorDQ has its own automatic setting. The other values follow the basic noise controls until you unlink them. Separate adjustments can preserve texture but leave grain or introduce artifacts. Detail controls how much texture is restored.");
    changed
}

fn row(
    ui: &mut egui::Ui,
    overrides: &mut noise::Overrides,
    parameter: Parameter,
    automatic: Option<f32>,
) -> bool {
    let before = overrides.get(parameter);
    let mut value = before.or(automatic).unwrap_or(0.);
    ui.push_id(parameter.key(), |ui| {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(before.is_some() || automatic.is_some(), |ui| {
                if link_button(ui, before.is_none(), parameter.parent()).clicked() {
                    overrides.set(parameter, if before.is_none() { Some(value) } else { None });
                }
            });
            let manual = overrides.get(parameter);
            value = manual.or(automatic).unwrap_or(0.);
            let bounds = parameter.bounds();
            let range = bounds.start().min(value)..=bounds.end().max(value);
            let default = automatic.unwrap_or(value);
            let editing = ui.add_enabled_ui(manual.is_some(), |ui| {
                crate::controls::adjustment_with_nudge(
                    ui,
                    parameter.label(),
                    &mut value,
                    range,
                    default,
                    if parameter == Parameter::LumaNoiseScale {
                        0.1
                    } else {
                        0.01
                    },
                )
            });
            crate::help::attach(
                editing.response,
                &format!(
                    "{}\n\n{} {}. {}",
                    parameter.help(),
                    if manual.is_some() {
                        "Set separately from"
                    } else {
                        "Follows"
                    },
                    parameter.parent(),
                    if manual.is_some() {
                        "Click the link to follow that control again."
                    } else {
                        "Click the link to keep this value and adjust it separately."
                    },
                ),
            );
            if manual.is_some() && editing.inner {
                overrides.set(parameter, Some(value));
            }
        });
    });
    before != overrides.get(parameter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_icons_have_rounded_button_outlines_and_interaction_feedback() {
        for linked in [false, true] {
            let ctx = egui::Context::default();
            crate::widgets::install_style(&ctx);
            let frame = |time, events| {
                let mut rect = egui::Rect::NOTHING;
                let mut clicked = false;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 400.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let response = link_button(ui, linked, "Luma noise");
                            rect = response.rect;
                            clicked = response.clicked();
                        });
                    },
                );
                let button = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(shape)
                            if shape.rect.contains(rect.center()) && shape.rect.width() < 30. =>
                        {
                            Some((shape.stroke, shape.fill, shape.corner_radius))
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!(rect.width() >= 22.);
                assert!(rect.height() >= 20.);
                assert!(button.0.width > 0.);
                assert_ne!(button.2, egui::CornerRadius::ZERO);
                (rect, clicked, button)
            };
            frame(0., vec![egui::Event::PointerMoved(egui::pos2(500., 300.))]);
            let (rect, clicked, normal) = frame(0.02, vec![]);
            assert!(!clicked);
            assert_eq!(
                normal.0.color,
                if linked {
                    egui::Color32::from_gray(70)
                } else {
                    egui::Color32::from_rgb(100, 160, 245)
                }
            );
            assert_eq!(
                normal.1,
                if linked {
                    egui::Color32::from_gray(29)
                } else {
                    egui::Color32::from_rgb(35, 65, 100)
                }
            );
            let point = rect.center();
            let (_, clicked, hovered) = frame(0.04, vec![egui::Event::PointerMoved(point)]);
            assert!(!clicked);
            assert_ne!(normal, hovered);
            let event = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            let (_, clicked, pressed) = frame(0.06, vec![event(true)]);
            assert!(!clicked);
            assert_ne!(hovered, pressed);
            assert!(frame(0.08, vec![event(false)]).1);
        }
    }

    #[test]
    fn colordq_auto_toggle_reports_changes_without_changing_linked_parameters() {
        let ctx = egui::Context::default();
        ctx.memory_mut(|memory| memory.set_everything_is_visible(true));
        let mut controls = Controls::default();
        let mut color_dq_gain = None;
        let mut frame = |time, events| {
            let mut changed = false;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 1000.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        changed = show(ui, &mut controls, &mut color_dq_gain, None, "Sunlight");
                    });
                },
            );
            assert_eq!(controls, Controls::default());
            (changed, output, color_dq_gain)
        };
        let (_, output, gain) = frame(0., vec![]);
        assert_eq!(gain, None);
        let point = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "A" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        let click = |pressed| {
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ]
        };
        assert!(!frame(0.02, click(true)).0);
        let (changed, _, gain) = frame(0.04, click(false));
        assert!(changed);
        assert_eq!(gain, Some(1.));
        assert!(!frame(0.06, click(true)).0);
        let (changed, _, gain) = frame(0.08, click(false));
        assert!(changed);
        assert_eq!(gain, None);
    }

    #[test]
    fn expanded_noise_controls_have_no_inline_link_note() {
        let ctx = egui::Context::default();
        ctx.memory_mut(|memory| memory.set_everything_is_visible(true));
        let mut controls = Controls::default();
        let mut color_dq_gain = None;
        let before = controls;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 800.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert!(!show(
                        ui,
                        &mut controls,
                        &mut color_dq_gain,
                        None,
                        "Sunlight"
                    ));
                });
            },
        );
        assert_eq!(controls, before);
        assert_eq!(color_dq_gain, None);
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&Parameter::MeanDecay.label()));
        assert!(!texts
            .iter()
            .any(|text| text.contains("Linked values follow the basic noise controls.")));
    }

    #[test]
    fn linked_read_only_parameters_still_explain_their_effect() {
        for parameter in Parameter::ALL {
            let ctx = egui::Context::default();
            let mut overrides = noise::Overrides::default();
            let mut output = None;
            for time in [0., 1., 2.] {
                output = Some(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 400.),
                        )),
                        time: Some(time),
                        events: vec![egui::Event::PointerMoved(egui::pos2(90., 18.))],
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            assert!(!row(ui, &mut overrides, parameter, Some(0.5)));
                        });
                    },
                ));
            }
            assert!(overrides.get(parameter).is_none());
            assert!(output.unwrap().shapes.iter().any(|shape| matches!(
                &shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains(parameter.help())
            )), "missing read-only help for {parameter:?}");
        }
    }

    #[test]
    fn pointer_link_captures_value_keeps_override_and_relinks_to_latest_parent() {
        let ctx = egui::Context::default();
        let mut overrides = noise::Overrides::default();
        let parameter = Parameter::MeanDecay;
        let mut frame = |time, automatic, events| {
            let mut changed = false;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(450., 100.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        changed = row(ui, &mut overrides, parameter, Some(automatic));
                    });
                },
            );
            (changed, output, overrides.get(parameter))
        };
        let (_, output, value) = frame(0., 0.95, vec![]);
        assert!(value.is_none());
        let point = output
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Circle(c) = &s.shape {
                    Some(c.center)
                } else {
                    None
                }
            })
            .unwrap();
        let click = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(
            1.,
            0.95,
            vec![egui::Event::PointerMoved(point), click(true)],
        );
        let (changed, _, value) = frame(1.1, 0.95, vec![click(false)]);
        assert!(changed);
        assert_eq!(value, Some(0.95));
        assert_eq!(frame(2., 0.4, vec![]).2, Some(0.95));
        frame(3., 0.4, vec![click(true)]);
        let (changed, _, value) = frame(3.1, 0.4, vec![click(false)]);
        assert!(changed);
        assert!(value.is_none());
    }
}
