//! Builds aligned sensor/tone editing rows with reset, focused value nudges, and sample-based rerender signals.

use crate::help;
use eframe::egui;
use foveonic::{
    render::{RepairThreshold, Settings},
    source::Metadata,
};
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
            help::control(ui.add_sized([92., 20.], egui::Label::new(label)), label);
            ui.spacing_mut().slider_width = (ui.available_width() - 62.).max(35.);
            let response = ui
                .add(egui::Slider::new(value, range.clone()).show_value(false))
                .interact(egui::Sense::click_and_drag());
            if response.double_clicked() {
                *value = default;
            }
            help::control(response, label);
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
            help::control(response, label);
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
                help::control(ui.add_sized([92., 20.], egui::Label::new(title)), title);
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
                    })
                    .response
                    .on_hover_text(help::editing(title).unwrap());
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
    ui.weak("Reference noise and detail");
    let mut reconstruction = settings.reconstruction.is_some();
    if help::control(
        ui.checkbox(&mut reconstruction, "Reference reconstruction"),
        "Reference reconstruction",
    )
    .changed()
    {
        settings.reconstruction = reconstruction.then(Default::default);
        changed = true;
    }
    if let Some(controls) = &mut settings.reconstruction {
        changed |= adjustment(ui, "Chroma noise", &mut controls.chroma, 0..=4, 2);
        changed |= adjustment(ui, "Luma noise", &mut controls.luminance, 0..=4, 2);
        changed |= adjustment(ui, "Detail", &mut controls.detail, 0. ..=2., 1.);
        ui.small("Noise preset 0 still reduces noise. Detail 0 shows the smoothed image; 1× restores the normal amount of detail.");
        changed |= crate::advanced::show(
            ui,
            controls,
            metadata,
            &settings.white_balance,
            settings.sensor_gain,
        );
    }
    ui.separator();
    ui.weak("Corrections and sharpening");
    let mut automatic = settings.color_dq_gain.is_none();
    if help::control(ui.checkbox(&mut automatic, "Auto ColorDQ"), "Auto ColorDQ").changed() {
        settings.color_dq_gain = if automatic { None } else { Some(1.) };
        changed = true;
    }
    if let Some(gain) = &mut settings.color_dq_gain {
        changed |= adjustment(ui, "ColorDQ gain", gain, 0. ..=2., 1.);
    } else if settings.reconstruction.is_none() {
        ui.weak("ColorDQ inactive without reconstruction");
    }
    changed |= adjustment(ui, "Sharpen", &mut settings.sharpen_amount, 0. ..=2., 0.);
    let mut repair = settings.repair.is_some();
    if help::control(
        ui.checkbox(&mut repair, "Camera-record repair"),
        "Camera-record repair",
    )
    .changed()
    {
        settings.repair = repair.then_some(RepairThreshold::Camera);
        changed = true;
    }
    if let Some(threshold) = &mut settings.repair {
        let camera = metadata.map(|m| RepairThreshold::Camera.resolve(m));
        let automatic = camera.as_ref().and_then(|r| r.as_ref().ok()).copied();
        let mut manual = matches!(threshold, RepairThreshold::Manual(_));
        if help::control(ui.checkbox(&mut manual, "Manual cutoff"), "Manual cutoff").changed() {
            *threshold = if manual {
                RepairThreshold::Manual(automatic.unwrap_or(0))
            } else {
                RepairThreshold::Camera
            };
            changed = true;
        }
        if let RepairThreshold::Manual(value) = threshold {
            let default = automatic.unwrap_or(*value);
            changed |= adjustment(ui, "Min. badness", value, 0..=15, default);
        } else if let Some(value) = automatic {
            help::control(
                ui.weak(format!("Camera defect threshold: {value}")),
                "Min. badness",
            );
        } else {
            ui.weak(camera.map_or_else(
                || "Waiting for camera cutoff".into(),
                |r| r.unwrap_err().to_string(),
            ));
        }
    }
    changed |= help::control(
        ui.checkbox(&mut settings.highlight_restoration, "Highlight restoration"),
        "Highlight restoration",
    )
    .changed();
    changed |= help::control(
        ui.checkbox(&mut settings.despeckle, "Despeckle"),
        "Despeckle",
    )
    .changed();
    changed |= help::control(
        ui.checkbox(&mut settings.row_correction, "Row banding correction"),
        "Row banding correction",
    )
    .changed();
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_visible_editing_controls_have_explanations() {
        let ctx = egui::Context::default();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        settings.color_dq_gain = Some(1.);
        settings.repair = Some(RepairThreshold::Manual(0));
        let metadata = Metadata {
            width: 1,
            height: 1,
            rotation: 0,
            properties: Default::default(),
            lists: Default::default(),
            matrices: Default::default(),
        };
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 2000.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    processing(ui, &mut settings, Some(&metadata));
                });
            },
        );
        for label in [
            "White balance",
            "Color mode",
            "Sensor gain",
            "Exposure",
            "Saturation",
            "Contrast",
            "Shadows",
            "Highlights",
            "Reference reconstruction",
            "Chroma noise",
            "Luma noise",
            "Detail",
            "Auto ColorDQ",
            "ColorDQ gain",
            "Sharpen",
            "Camera-record repair",
            "Manual cutoff",
            "Min. badness",
            "Highlight restoration",
            "Despeckle",
        ] {
            assert!(
                output.shapes.iter().any(|shape| matches!(
                    &shape.shape, egui::Shape::Text(text) if text.galley.job.text == label
                )),
                "control missing: {label}"
            );
            assert!(help::editing(label).is_some(), "help missing: {label}");
        }
    }

    #[test]
    fn sensor_gain_help_opens_on_label_slider_and_value_without_changing_settings() {
        for point in [
            egui::pos2(45., 18.),
            egui::pos2(220., 18.),
            egui::pos2(362., 18.),
        ] {
            let ctx = egui::Context::default();
            let mut value = 1_f32;
            let mut output = None;
            for time in [0., 1., 2.] {
                output = Some(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 400.),
                        )),
                        time: Some(time),
                        events: vec![egui::Event::PointerMoved(point)],
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.set_width(384.);
                            assert!(!adjustment(ui, "Sensor gain", &mut value, 0. ..=2., 1.));
                        });
                    },
                ));
            }
            assert_eq!(value, 1.);
            assert!(output.unwrap().shapes.iter().any(|shape| matches!(
                &shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains("An ISO-like adjustment")
            )), "help did not open at {point:?}");
        }
    }

    #[test]
    fn correction_checkboxes_start_enabled_and_toggle_independently() {
        for label in [
            "Camera-record repair",
            "Highlight restoration",
            "Despeckle",
            "Row banding correction",
        ] {
            let ctx = egui::Context::default();
            let mut settings = Settings::new("Sunlight", "Standard");
            let mut render = |events| {
                let mut changed = false;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 2000.),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            changed = processing(ui, &mut settings, None);
                        });
                    },
                );
                (
                    changed,
                    output,
                    settings.repair.is_some(),
                    settings.highlight_restoration,
                    settings.despeckle,
                    settings.row_correction,
                )
            };
            let (_, output, repair, highlights, despeckle, rows) = render(vec![]);
            assert!(repair && highlights && despeckle && rows);
            let point = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => {
                        Some(t.pos + t.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            let event = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            render(vec![egui::Event::PointerMoved(point), event(true)]);
            let (changed, _, repair, highlights, despeckle, rows) = render(vec![event(false)]);
            assert!(changed);
            assert_eq!(
                (repair, highlights, despeckle, rows),
                match label {
                    "Despeckle" => (true, true, false, true),
                    "Highlight restoration" => (true, false, true, true),
                    "Row banding correction" => (true, true, true, false),
                    _ => (false, true, true, true),
                }
            );
        }
    }

    #[test]
    fn reference_checkbox_enables_shared_defaults_and_requests_rerender() {
        let ctx = egui::Context::default();
        let mut settings = Settings::new("Sunlight", "Standard");
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
                        changed = processing(ui, &mut settings, None);
                    });
                },
            );
            (changed, output, settings.reconstruction)
        };
        let (changed, output, reconstruction) = frame(0., vec![]);
        assert!(reconstruction.is_none());
        assert!(!changed);
        let point = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Reference reconstruction" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        for (time, enabled) in [(1., true), (2., false)] {
            let event = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            assert!(!frame(time, vec![egui::Event::PointerMoved(point), event(true)]).0);
            let (changed, _, controls) = frame(time + 0.1, vec![event(false)]);
            assert!(changed);
            assert_eq!(controls, enabled.then(Default::default));
        }
        assert!(settings.reconstruction.is_none());
        assert_eq!(settings.sensor_gain, 1.);
        assert_eq!(settings.exposure, 0.);
    }

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
