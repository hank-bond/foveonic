//! Builds aligned sensor/tone editing rows with reset, focused value nudges, and sample-based rerender signals.

use eframe::egui;
use foveonic::{render::Settings, source::Metadata};
use std::ops::RangeInclusive;

pub fn adjustment<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: RangeInclusive<T>,
    default: T,
) -> bool {
    adjustment_with_nudge(
        ui,
        label,
        value,
        range,
        default,
        if T::INTEGRAL { 1. } else { 0.1 },
    )
}

/// Set the focused value box's arrow-key step independently of mouse-drag sensitivity.
pub fn adjustment_with_nudge<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: RangeInclusive<T>,
    default: T,
    nudge: f64,
) -> bool {
    let before = value.to_f64();
    let decimals = if T::INTEGRAL {
        0
    } else {
        (-nudge.abs().log10()).ceil().clamp(2., 15.) as usize
    };
    ui.push_id(label, |ui| {
        ui.horizontal(|ui| {
            ui.add_sized([92., 20.], egui::Label::new(label));
            ui.spacing_mut().slider_width = (ui.available_width() - 62.).max(35.);
            let response = ui
                .add(egui::Slider::new(value, range.clone()).show_value(false))
                .interact(egui::Sense::click_and_drag());
            if response.double_clicked() {
                *value = default;
            }
            // Match add_sized's layout, but inspect DragValue's stable child ID before it handles keys.
            let response = ui
                .allocate_ui_with_layout(
                    egui::vec2(54., 20.),
                    egui::Layout::centered_and_justified(ui.layout().main_dir()),
                    |ui| {
                        let focused = ui.memory(|memory| memory.has_focus(ui.next_auto_id()));
                        ui.add(
                            egui::DragValue::new(value)
                                .range(range.clone())
                                .speed(if focused {
                                    nudge
                                } else if T::INTEGRAL {
                                    1.
                                } else {
                                    0.01
                                })
                                .fixed_decimals(decimals),
                        )
                    },
                )
                .inner;
            // Clamp keyboard overshoot and refresh the value field's cached text.
            let accepted = value
                .to_f64()
                .clamp(range.start().to_f64(), range.end().to_f64());
            if value.to_f64() != accepted {
                *value = T::from_f64(accepted);
                ui.data_mut(|data| data.remove::<String>(response.id));
                ui.ctx().request_repaint();
            }
        });
    });
    // Double-click sensing replaces the slider response, so compare actual values for rerendering.
    before != value.to_f64()
}

pub fn processing(ui: &mut egui::Ui, settings: &mut Settings, metadata: Option<&Metadata>) -> bool {
    let mut changed = false;
    if let Some(metadata) = metadata {
        for (title, key, value) in [
            (
                "White balance",
                "WhiteBalanceGains",
                &mut settings.white_balance,
            ),
            (
                "Color mode",
                "ColorModeCompensations",
                &mut settings.color_mode,
            ),
        ] {
            ui.horizontal(|ui| {
                ui.add_sized([92., 20.], egui::Label::new(title));
                egui::ComboBox::from_id_salt(key)
                    .width(ui.available_width() - 8.)
                    .selected_text(if value.is_empty() {
                        "Choose…"
                    } else {
                        value.as_str()
                    })
                    .show_ui(ui, |ui| {
                        if let Some(list) = metadata.lists.get(key) {
                            for name in list.names() {
                                changed |=
                                    ui.selectable_value(value, name.to_owned(), name).changed();
                            }
                        }
                    });
            });
        }
    }
    ui.separator();
    ui.weak("Sensor");
    changed |= adjustment(ui, "Sensor gain", &mut settings.sensor_gain, 0. ..=2., 1.);
    ui.separator();
    ui.weak("Tone and color");
    for (label, value, range, default) in [
        ("Exposure", &mut settings.exposure, -4. ..=4., 0.),
        ("Saturation", &mut settings.saturation, 0. ..=2., 1.),
    ] {
        changed |= adjustment(ui, label, value, range, default);
    }
    for (label, value) in [
        ("Contrast", &mut settings.tone.contrast),
        ("Shadows", &mut settings.tone.shadows),
        ("Highlights", &mut settings.tone.highlights),
    ] {
        changed |= adjustment(ui, label, value, -2. ..=2., 0.);
    }
    ui.separator();
    ui.weak("Corrections and detail");
    for (label, value, range, default) in [
        ("ColorDQ gain", &mut settings.color_dq_gain, 0. ..=2., 0.),
        ("Sharpen", &mut settings.sharpen_amount, 0. ..=2., 0.),
        ("Radius", &mut settings.sharpen_radius, 0.1..=16., 1.),
    ] {
        changed |= adjustment(ui, label, value, range, default);
    }
    let mut repair = settings.repair_badness.is_some();
    if ui
        .checkbox(&mut repair, "Isolated bad-pixel repair")
        .changed()
    {
        settings.repair_badness = repair.then_some(8);
        changed = true;
    }
    if let Some(value) = &mut settings.repair_badness {
        changed |= adjustment(ui, "Min. badness", value, 0..=15, 8);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    fn nudge_frame<T: egui::emath::Numeric>(
        ctx: &egui::Context,
        value: &mut T,
        range: RangeInclusive<T>,
        step: Option<f64>,
        time: f64,
        events: Vec<egui::Event>,
    ) -> bool {
        let mut changed = false;
        let _ = ctx.run(
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
                    changed = if let Some(step) = step {
                        adjustment_with_nudge(
                            ui,
                            "Value",
                            value,
                            range.clone(),
                            T::from_f64(1.),
                            step,
                        )
                    } else {
                        adjustment(ui, "Value", value, range.clone(), T::from_f64(1.))
                    };
                });
            },
        );
        changed
    }
    fn arrow(key: egui::Key, repeat: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers: Default::default(),
        }
    }
    fn focus_nudge<T: egui::emath::Numeric>(
        ctx: &egui::Context,
        value: &mut T,
        range: RangeInclusive<T>,
        step: Option<f64>,
    ) {
        nudge_frame(ctx, value, range.clone(), step, 0., vec![]);
        let pos = egui::pos2(362., 18.);
        for (time, pressed) in [(0.1, true), (0.2, false)] {
            nudge_frame(
                ctx,
                value,
                range.clone(),
                step,
                time,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }
    #[test]
    fn focused_float_values_nudge_by_a_tenth_preserve_precision_and_stop_at_bounds() {
        let ctx = egui::Context::default();
        let mut value = 1.05_f32;
        assert!(!nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.,
            vec![arrow(egui::Key::ArrowUp, false)]
        ));
        assert_eq!(value, 1.05); // unfocused value stays unchanged
        focus_nudge(&ctx, &mut value, 0. ..=2., None);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.3,
            vec![arrow(egui::Key::ArrowUp, false)]
        ));
        assert!((value - 1.15).abs() < 0.00001);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.4,
            vec![arrow(egui::Key::ArrowDown, false)]
        ));
        assert!((value - 1.05).abs() < 0.00001);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.5,
            vec![arrow(egui::Key::ArrowDown, true)]
        ));
        assert!((value - 0.95).abs() < 0.00001);
        value = 2.;
        assert!(!nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.6,
            vec![arrow(egui::Key::ArrowUp, true)]
        ));
        assert_eq!(value, 2.);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.7,
            vec![arrow(egui::Key::ArrowDown, false)]
        ));
        assert_eq!(value, 1.9);
        value = 0.;
        assert!(!nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            None,
            0.8,
            vec![arrow(egui::Key::ArrowDown, true)]
        ));
        assert_eq!(value, 0.);
    }
    #[test]
    fn parameter_nudge_overrides_and_integer_steps_are_respected() {
        let ctx = egui::Context::default();
        let mut value = 1_f32;
        focus_nudge(&ctx, &mut value, 0. ..=2., Some(0.25));
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            Some(0.25),
            0.3,
            vec![arrow(egui::Key::ArrowUp, false)]
        ));
        assert_eq!(value, 1.25);
        let ctx = egui::Context::default();
        let mut value = 1_f32;
        focus_nudge(&ctx, &mut value, 0. ..=2., Some(0.001));
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0. ..=2.,
            Some(0.001),
            0.3,
            vec![arrow(egui::Key::ArrowUp, false)]
        ));
        assert!((value - 1.001).abs() < 0.000001);
        let ctx = egui::Context::default();
        let mut value = 8_u32;
        focus_nudge(&ctx, &mut value, 0..=15, None);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0..=15,
            None,
            0.3,
            vec![arrow(egui::Key::ArrowUp, false)]
        ));
        assert_eq!(value, 9);
        assert!(nudge_frame(
            &ctx,
            &mut value,
            0..=15,
            None,
            0.4,
            vec![arrow(egui::Key::ArrowDown, false)]
        ));
        assert_eq!(value, 8);
    }

    #[test]
    fn slider_drag_reports_value_changes_for_rerender() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 100.),
            )),
            ..Default::default()
        };
        let mut value = 0_f32;
        let _ = ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                adjustment(ui, "Exposure", &mut value, -4. ..=4., 0.);
            });
        });
        for (time, events) in [
            (
                1.,
                vec![
                    egui::Event::PointerMoved(egui::pos2(290., 18.)),
                    egui::Event::PointerButton {
                        pos: egui::pos2(290., 18.),
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            ),
            (1.1, vec![egui::Event::PointerMoved(egui::pos2(260., 18.))]),
        ] {
            let before = value;
            let mut input = input.clone();
            input.time = Some(time);
            input.events = events;
            let mut changed = false;
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    changed |= adjustment(ui, "Exposure", &mut value, -4. ..=4., 0.);
                });
            });
            assert_ne!(value, before, "pointer input must actually move the slider");
            assert!(changed, "a changed value must request a rerender");
        }
    }

    #[test]
    fn double_click_resets_the_slider_and_value_remains_editable() {
        let ctx = egui::Context::default();
        let mut value = 0.7_f32;
        let mut slider = egui::Rect::NOTHING;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 100.),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                adjustment(ui, "Exposure", &mut value, -4. ..=4., 0.);
            });
        });
        // The slider follows the 92-point label in this row, before the value field.
        slider.min = egui::pos2(108., 8.);
        slider.max = egui::pos2(320., 28.);
        let point = slider.center();
        for (time, pressed) in [(1., true), (1.05, false), (1.1, true), (1.15, false)] {
            let mut event = input.clone();
            event.time = Some(time);
            event.events = vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ];
            let _ = ctx.run(event, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    adjustment(ui, "Exposure", &mut value, -4. ..=4., 0.);
                });
            });
        }
        assert_eq!(value, 0.);
        // Click the separate value field, then type a numeric value and accept it.
        let field = egui::pos2(362., 18.);
        for (time, events) in [
            (
                2.,
                vec![
                    egui::Event::PointerMoved(field),
                    egui::Event::PointerButton {
                        pos: field,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            ),
            (
                2.1,
                vec![egui::Event::PointerButton {
                    pos: field,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            ),
            (2.2, vec![egui::Event::Text("1.25".into())]),
            (
                2.3,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            ),
        ] {
            let mut event = input.clone();
            event.time = Some(time);
            event.events = events;
            let _ = ctx.run(event, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    adjustment(ui, "Exposure", &mut value, -4. ..=4., 0.);
                });
            });
        }
        assert_eq!(value, 1.25);
    }
}
