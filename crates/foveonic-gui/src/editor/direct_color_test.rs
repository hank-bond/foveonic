//! Checks weighted gestures, delayed picks and stale-reply isolation through egui pointer events.

use super::*;
#[test]
fn weighted_drag_preserves_other_colors_and_clamps_existing_slider_values() {
    let mut original = ColorEditorControls::default();
    original.bands[1].hue = 0.9;
    original.master.saturation = 0.4;
    let weights = [0.5, 0.5, 0., 0., 0., 0., 0., 0.];
    let result = adjusted(original, weights, Channel::Hue, 0.4);
    assert_eq!(result.bands[0].hue, 0.2);
    assert_eq!(result.bands[1].hue, 1.);
    assert_eq!(result.bands[2..], original.bands[2..]);
    assert_eq!(result.master, original.master);
    assert_eq!(
        adjusted(original, [0.; 8], Channel::Lightness, 1.),
        original
    );
    assert_eq!(
        adjusted(original, weights, Channel::Saturation, -1.).bands[0].saturation,
        -0.5
    );
}
#[test]
fn outdated_picks_and_disabled_tools_cannot_replace_the_current_selection() {
    let ctx = egui::Context::default();
    toggle(&ctx);
    let settings = Settings::default();
    let mut s = state(&ctx);
    s.drag = Some(Drag {
        target: Target::Basic,
        range: None,
        token: 2,
        path: "current.x3f".into(),
        anchor: egui::Pos2::ZERO,
        position: egui::Pos2::ZERO,
        configuration: Default::default(),
        moved: false,
        released: false,
        amounts: [0.; 3],
        original: Default::default(),
        expected: settings,
        weights: None,
        feedback: None,
    });
    store(&ctx, s);
    for (token, path) in [(1, "current.x3f"), (2, "old.x3f")] {
        receive(
            &ctx,
            ColorPickReply {
                tool: ColorPickTool::Basic,
                token,
                path: path.into(),
                sample: Ok([1., 0., 0., 0., 0., 0., 0., 0.].into()),
            },
        );
        assert!(state(&ctx).drag.unwrap().weights.is_none());
    }
    select(&ctx, 3);
    receive(
        &ctx,
        ColorPickReply {
            tool: ColorPickTool::Basic,
            token: 2,
            path: "current.x3f".into(),
            sample: Ok([1.; 8].into()),
        },
    );
    assert_eq!(selection(&ctx), (3, [0.; 8]));
    exit(&ctx);
    receive(
        &ctx,
        ColorPickReply {
            tool: ColorPickTool::Basic,
            token: 2,
            path: "current.x3f".into(),
            sample: Ok([1.; 8].into()),
        },
    );
    assert!(!active(&ctx));
    assert_eq!(selection(&ctx).1, [0.; 8]);
}

mod gesture_tests {
    use super::*;
    pub(super) fn frame(
        ctx: &egui::Context,
        settings: &mut Settings,
        events: Vec<egui::Event>,
        time: f64,
    ) -> (bool, Option<ColorPickRequest>) {
        let mut result = (false, None);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 400.),
                )),
                modifiers: events
                    .iter()
                    .find_map(|e| match e {
                        egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                        _ => None,
                    })
                    .unwrap_or_else(|| ctx.input(|i| i.modifiers)),
                events,
                time: Some(time),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (_, response) =
                        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                    result = photo(
                        ui,
                        &response,
                        Some([0.4, 0.6]),
                        Path::new("a.x3f"),
                        settings,
                        true,
                    );
                });
            },
        );
        result
    }
    pub(super) fn button(pos: egui::Pos2, pressed: bool, alt: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers {
                alt,
                ..Default::default()
            },
        }
    }
    #[test]
    fn delayed_pick_after_release_applies_original_drag_once_and_updates_existing_sliders() {
        let ctx = egui::Context::default();
        toggle(&ctx);
        let mut settings = Settings::default();
        let anchor = egui::pos2(200., 200.);
        let end = egui::pos2(203., 110.);
        frame(&ctx, &mut settings, vec![], 0.);
        let (changed, request) = frame(
            &ctx,
            &mut settings,
            vec![
                egui::Event::PointerMoved(anchor),
                button(anchor, true, false),
            ],
            0.1,
        );
        assert!(!changed);
        let request = request.unwrap();
        assert_eq!(request.uv, [0.4, 0.6]);
        frame(
            &ctx,
            &mut settings,
            vec![egui::Event::PointerMoved(end)],
            0.2,
        );
        let (changed, _) = frame(&ctx, &mut settings, vec![button(end, false, false)], 0.3);
        assert!(!changed);
        assert!(
            !active(&ctx),
            "release must restore ordinary image navigation before a delayed reply"
        );
        frame(
            &ctx,
            &mut settings,
            vec![egui::Event::PointerMoved(egui::pos2(450., 350.))],
            0.35,
        );
        receive(
            &ctx,
            ColorPickReply {
                tool: ColorPickTool::Basic,
                token: request.token,
                path: request.path,
                sample: Ok([0.75, 0.25, 0., 0., 0., 0., 0., 0.].into()),
            },
        );
        assert!(frame(&ctx, &mut settings, vec![], 0.4).0);
        assert!((settings.photographic.color_editor.bands[0].lightness - 0.225).abs() < 1e-6);
        assert!((settings.photographic.color_editor.bands[1].lightness - 0.075).abs() < 1e-6);
        assert!((settings.photographic.color_editor.bands[0].hue - 0.0075).abs() < 1e-6);
        assert!(!frame(&ctx, &mut settings, vec![], 0.5).0);
    }
    #[test]
    fn horizontal_and_option_vertical_gestures_preserve_tone_and_unselected_ranges() {
        for alt in [false, true] {
            let ctx = egui::Context::default();
            toggle(&ctx);
            let mut settings = Settings {
                exposure: -1.3,
                ..Default::default()
            };
            let anchor = egui::pos2(200., 200.);
            frame(&ctx, &mut settings, vec![], 0.);
            let (_, request) = frame(
                &ctx,
                &mut settings,
                vec![egui::Event::PointerMoved(anchor), button(anchor, true, alt)],
                0.1,
            );
            let request = request.unwrap();
            receive(
                &ctx,
                ColorPickReply {
                    tool: ColorPickTool::Basic,
                    token: request.token,
                    path: request.path,
                    sample: Ok([0., 0., 0., 0., 1., 0., 0., 0.].into()),
                },
            );
            let end = if alt {
                egui::pos2(201., 110.)
            } else {
                egui::pos2(290., 200.)
            };
            assert!(
                frame(
                    &ctx,
                    &mut settings,
                    vec![egui::Event::PointerMoved(end)],
                    0.2
                )
                .0
            );
            let c = settings.photographic.color_editor.bands[4];
            assert_eq!(settings.exposure, -1.3);
            assert_eq!(c.lightness, 0.);
            assert!(
                (if alt { c.saturation } else { c.hue } - if alt { 0.6 } else { 0.3 }).abs() < 1e-6
            );
            assert_eq!(
                settings.photographic.color_editor.bands[0],
                Default::default()
            );
        }
    }
}

#[test]
fn gesture_settings_swap_collisions_and_persist_without_image_edits() {
    let mut config = Configuration::default();
    config.assign(1, Gesture::Vertical);
    assert_eq!(config.channel(Gesture::Vertical).unwrap().index(), 1);
    assert_eq!(config.gestures[2], Gesture::AltVertical);
    config.assign(2, Gesture::AltHorizontal);
    config.sensitivity = 75.;
    let prefs = crate::preferences::Preferences {
        direct_color: config,
        ..Default::default()
    };
    let restored: crate::preferences::Preferences =
        serde_json::from_slice(&serde_json::to_vec(&prefs).unwrap()).unwrap();
    assert_eq!(restored.direct_color, config);
    let old: crate::preferences::Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.direct_color, Configuration::default());
    let ctx = egui::Context::default();
    configure(&ctx, config);
    assert_eq!(configuration(&ctx), config);
}

#[test]
fn changed_assignments_and_sensitivity_control_the_same_pointer_gesture() {
    let ctx = egui::Context::default();
    let mut config = Configuration::default();
    config.assign(1, Gesture::Vertical);
    config.sensitivity = 100.;
    configure(&ctx, config);
    toggle(&ctx);
    let mut settings = Settings::default();
    let anchor = egui::pos2(200., 200.);
    let end = egui::pos2(200., 110.);
    gesture_tests::frame(&ctx, &mut settings, vec![], 0.);
    let (_, request) = gesture_tests::frame(
        &ctx,
        &mut settings,
        vec![
            egui::Event::PointerMoved(anchor),
            gesture_tests::button(anchor, true, false),
        ],
        0.1,
    );
    let request = request.unwrap();
    receive(
        &ctx,
        ColorPickReply {
            tool: ColorPickTool::Basic,
            token: request.token,
            path: request.path,
            sample: Ok([1., 0., 0., 0., 0., 0., 0., 0.].into()),
        },
    );
    gesture_tests::frame(
        &ctx,
        &mut settings,
        vec![egui::Event::PointerMoved(end)],
        0.2,
    );
    assert_eq!(settings.photographic.color_editor.bands[0].saturation, 1.2);
    assert_eq!(settings.photographic.color_editor.bands[0].lightness, 0.);
}

#[test]
fn one_held_drag_changes_both_axes_and_can_change_direction_without_releasing() {
    let ctx = egui::Context::default();
    toggle(&ctx);
    let mut settings = Settings::default();
    let anchor = egui::pos2(200., 200.);
    gesture_tests::frame(&ctx, &mut settings, vec![], 0.);
    let (_, request) = gesture_tests::frame(
        &ctx,
        &mut settings,
        vec![
            egui::Event::PointerMoved(anchor),
            gesture_tests::button(anchor, true, false),
        ],
        0.1,
    );
    let request = request.unwrap();
    receive(
        &ctx,
        ColorPickReply {
            tool: ColorPickTool::Basic,
            token: request.token,
            path: request.path,
            sample: Ok([1., 0., 0., 0., 0., 0., 0., 0.].into()),
        },
    );
    gesture_tests::frame(
        &ctx,
        &mut settings,
        vec![egui::Event::PointerMoved(egui::pos2(260., 110.))],
        0.2,
    );
    let c = settings.photographic.color_editor.bands[0];
    assert!((c.hue - 0.2).abs() < 1e-6);
    assert!((c.lightness - 0.3).abs() < 1e-6);
    gesture_tests::frame(
        &ctx,
        &mut settings,
        vec![egui::Event::PointerMoved(egui::pos2(230., 170.))],
        0.3,
    );
    let c = settings.photographic.color_editor.bands[0];
    assert!((c.hue - 0.1).abs() < 1e-6);
    assert!((c.lightness - 0.1).abs() < 1e-6);
    // Reassign the live modifier during the same hold. Preserve prior lightness
    // and hue while the remaining vertical movement changes Saturation.
    gesture_tests::frame(
        &ctx,
        &mut settings,
        vec![egui::Event::PointerButton {
            pos: egui::pos2(230., 170.),
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::ALT,
        }],
        0.4,
    );
    gesture_tests::frame(
        &ctx,
        &mut settings,
        vec![egui::Event::PointerMoved(egui::pos2(230., 110.))],
        0.5,
    );
    let c = settings.photographic.color_editor.bands[0];
    assert!((c.hue - 0.1).abs() < 1e-6);
    assert!((c.lightness - 0.1).abs() < 1e-6);
    assert!((c.saturation - 0.4).abs() < 1e-6);
}

#[test]
fn right_click_settings_open_and_escape_closes_the_menu_before_the_tool() {
    let ctx = egui::Context::default();
    toggle(&ctx);
    let mut settings = Settings::default();
    let position = egui::pos2(200., 200.);
    gesture_tests::frame(&ctx, &mut settings, vec![], 0.);
    let button = |pressed| egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    gesture_tests::frame(
        &ctx,
        &mut settings,
        vec![egui::Event::PointerMoved(position), button(true)],
        0.1,
    );
    gesture_tests::frame(&ctx, &mut settings, vec![button(false)], 0.2);
    assert!(menu_open(&ctx));
    gesture_tests::frame(
        &ctx,
        &mut settings,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        0.3,
    );
    assert!(!menu_open(&ctx));
    assert!(active(&ctx));
    assert_eq!(settings, Settings::default());
}

#[test]
fn direct_saturation_reaches_the_shared_positive_limit_without_extending_hue_or_lightness() {
    let mut original = ColorEditorControls::default();
    original.bands[0].saturation = 1.8;
    let mut weights = [0.; 8];
    weights[0] = 1.;
    assert_eq!(
        adjusted(original, weights, Channel::Saturation, 0.5).bands[0].saturation,
        2.
    );
    assert_eq!(
        adjusted(original, weights, Channel::Hue, 5.).bands[0].hue,
        1.
    );
    assert_eq!(
        adjusted(original, weights, Channel::Lightness, 5.).bands[0].lightness,
        1.
    );
}

#[test]
fn advanced_drag_uses_both_axes_and_finishes_delayed_picks_without_touching_existing_ranges() {
    for delayed in [false, true] {
        let ctx = egui::Context::default();
        toggle_advanced(&ctx);
        let mut settings = Settings::default();
        let existing = foveonic::render::AdvancedColorRange {
            hue: 40.,
            ..Default::default()
        };
        settings.photographic.color_editor.advanced.push(existing);
        let before = settings.photographic.color_editor;
        let anchor = egui::pos2(200., 200.);
        let end = egui::pos2(260., 110.);
        gesture_tests::frame(&ctx, &mut settings, vec![], 0.);
        let (_, request) = gesture_tests::frame(
            &ctx,
            &mut settings,
            vec![
                egui::Event::PointerMoved(anchor),
                gesture_tests::button(anchor, true, false),
            ],
            0.1,
        );
        assert!(
            advanced_active(&ctx),
            "press must not disarm the eyedropper"
        );
        let request = request.unwrap();
        let reply = || ColorPickReply {
            tool: ColorPickTool::Advanced,
            token: request.token,
            path: request.path.clone(),
            sample: Ok(foveonic::render::ColorRangeSample {
                color: [240., 0.7, 1., 0.],
                weights: [1.; 8],
            }),
        };
        if !delayed {
            receive(&ctx, reply());
        }
        gesture_tests::frame(
            &ctx,
            &mut settings,
            vec![egui::Event::PointerMoved(end)],
            0.2,
        );
        gesture_tests::frame(
            &ctx,
            &mut settings,
            vec![gesture_tests::button(end, false, false)],
            0.3,
        );
        assert!(!active(&ctx));
        if delayed {
            gesture_tests::frame(
                &ctx,
                &mut settings,
                vec![egui::Event::PointerMoved(egui::pos2(450., 350.))],
                0.4,
            );
            receive(&ctx, reply());
            assert!(gesture_tests::frame(&ctx, &mut settings, vec![], 0.5).0);
        }
        let result = settings.photographic.color_editor;
        assert_eq!(result.bands, before.bands);
        assert_eq!(result.master, before.master);
        assert_eq!(result.advanced.count, 2);
        assert_eq!(result.advanced.ranges[0], existing);
        let range = result.advanced.ranges[1];
        assert_eq!(range.hue, 240.);
        assert!((range.adjustment.hue - 0.2).abs() < 1e-6);
        assert!((range.adjustment.lightness - 0.3).abs() < 1e-6);
        assert_eq!(super::super::advanced_color::state(&ctx).selected, 1);
        assert!(!gesture_tests::frame(&ctx, &mut settings, vec![], 0.6).0);
        assert!(!dragging(&ctx));
    }
}

#[test]
fn toolbar_release_arms_each_picker_until_a_photo_gesture_releases() {
    for advanced in [false, true] {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        let mut button_center = egui::Pos2::ZERO;
        let mut frame = |time, events| {
            let _ = ctx.run(
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
                        let button = crate::widgets::icon_button(
                            ui,
                            "Pick",
                            active(ctx),
                            crate::widgets::ControlSize::Large,
                        );
                        button_center = button.rect.center();
                        if button.clicked() {
                            if advanced {
                                toggle_advanced(ctx)
                            } else {
                                toggle(ctx)
                            }
                        }
                        let (_, response) = ui.allocate_exact_size(
                            ui.available_size(),
                            egui::Sense::click_and_drag(),
                        );
                        photo(
                            ui,
                            &response,
                            Some([0.4, 0.6]),
                            Path::new("a.x3f"),
                            &mut settings,
                            true,
                        );
                    });
                },
            );
            button_center
        };
        let pos = frame(0., vec![]);
        frame(
            0.1,
            vec![
                egui::Event::PointerMoved(pos),
                gesture_tests::button(pos, true, false),
            ],
        );
        frame(0.2, vec![gesture_tests::button(pos, false, false)]);
        assert!(
            active(&ctx),
            "the toolbar's release must leave the picker armed"
        );
        let photo_pos = egui::pos2(200., 200.);
        frame(
            0.3,
            vec![
                egui::Event::PointerMoved(photo_pos),
                gesture_tests::button(photo_pos, true, false),
            ],
        );
        assert!(active(&ctx));
        frame(0.4, vec![gesture_tests::button(photo_pos, false, false)]);
        assert!(!active(&ctx));
    }
}

#[test]
fn asynchronous_pick_errors_reach_the_log_once_and_ignore_stale_settings() {
    for stale in [false, true] {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        let mut log = crate::activity::Log::default();
        toggle(&ctx);
        gesture_tests::frame(&ctx, &mut settings, vec![], 0.);
        let pos = egui::pos2(100., 100.);
        let request = gesture_tests::frame(
            &ctx,
            &mut settings,
            vec![
                egui::Event::PointerMoved(pos),
                gesture_tests::button(pos, true, false),
            ],
            0.1,
        )
        .1
        .unwrap();
        gesture_tests::frame(
            &ctx,
            &mut settings,
            vec![gesture_tests::button(pos, false, false)],
            0.2,
        );
        log.receive(&ctx);
        log.open();
        receive(
            &ctx,
            ColorPickReply {
                tool: request.tool,
                token: request.token,
                path: request.path,
                sample: Err("No reliable color at this point".into()),
            },
        );
        if stale {
            settings.exposure = 1.;
        }
        gesture_tests::frame(&ctx, &mut settings, vec![], 0.3);
        log.receive(&ctx);
        assert_eq!(log.unread_errors(), !stale);
        log.open();
        gesture_tests::frame(&ctx, &mut settings, vec![], 0.4);
        log.receive(&ctx);
        assert!(!log.unread_errors());
    }
}
