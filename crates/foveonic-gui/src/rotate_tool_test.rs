//! Checks rotation gestures, two-click alignment, cancellation and relative tool selection.

use super::*;
fn pointer_frame(
    ctx: &egui::Context,
    rotation: &mut Rotation,
    time: &mut f64,
    events: Vec<egui::Event>,
) -> bool {
    *time += 0.05;
    let mut changed = false;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 400.),
            )),
            time: Some(*time),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (image, response) =
                    ui.allocate_exact_size(egui::vec2(300., 300.), egui::Sense::click_and_drag());
                changed |= photo(ui, &response, image, rotation);
            });
        },
    );
    changed
}
fn pointer_button(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        },
    ]
}
#[test]
fn straighten_accepts_drag_and_two_click_lines_and_commits_only_the_final_release() {
    let start = egui::pos2(80., 80.);
    for end in [
        egui::pos2(240., 120.),
        egui::pos2(120., 240.),
        egui::pos2(84., 81.),
        egui::pos2(81., 84.),
    ] {
        let mut results = Vec::new();
        for two_click in [false, true] {
            let ctx = egui::Context::default();
            activate(&ctx, Mode::Straighten);
            let mut rotation = Rotation::default();
            let mut time = 0.;
            pointer_frame(&ctx, &mut rotation, &mut time, vec![]);
            assert!(!pointer_frame(
                &ctx,
                &mut rotation,
                &mut time,
                pointer_button(start, true)
            ));
            if two_click {
                assert!(!pointer_frame(
                    &ctx,
                    &mut rotation,
                    &mut time,
                    pointer_button(start, false)
                ));
                assert_eq!(state(&ctx).gesture.unwrap().phase, Phase::AwaitingEnd);
            }
            assert!(!pointer_frame(
                &ctx,
                &mut rotation,
                &mut time,
                vec![egui::Event::PointerMoved(end)]
            ));
            assert!(rotation.is_neutral());
            assert_eq!(displayed(&ctx, rotation), rotation);
            assert_eq!(state(&ctx).gesture.as_ref().unwrap().start, start);
            if two_click {
                assert!(!pointer_frame(
                    &ctx,
                    &mut rotation,
                    &mut time,
                    pointer_button(end, true)
                ));
                assert!(rotation.is_neutral());
            }
            assert!(pointer_frame(
                &ctx,
                &mut rotation,
                &mut time,
                pointer_button(end, false)
            ));
            assert!(state(&ctx).gesture.is_none());
            results.push(rotation.degrees);
        }
        assert_eq!(results[0], results[1]);
        let expected =
            if end.x > end.y { -1. } else { 1. } * f64::from(0.25_f32.atan().to_degrees());
        assert!((results[0] - expected).abs() < 1e-4);
    }
}
#[test]
fn unfinished_click_line_cancels_on_escape_or_an_outside_click() {
    for outside in [false, true] {
        let ctx = egui::Context::default();
        activate(&ctx, Mode::Straighten);
        let mut rotation = Rotation {
            degrees: 12.,
            ..Default::default()
        };
        let original = rotation;
        let mut time = 0.;
        pointer_frame(&ctx, &mut rotation, &mut time, vec![]);
        let start = egui::pos2(80., 80.);
        pointer_frame(&ctx, &mut rotation, &mut time, pointer_button(start, true));
        pointer_frame(&ctx, &mut rotation, &mut time, pointer_button(start, false));
        assert_eq!(state(&ctx).gesture.unwrap().phase, Phase::AwaitingEnd);
        if outside {
            assert!(!pointer_frame(
                &ctx,
                &mut rotation,
                &mut time,
                pointer_button(egui::pos2(350., 350.), true)
            ));
        } else {
            escape(&ctx);
        }
        assert!(active(&ctx));
        assert!(state(&ctx).gesture.is_none());
        assert_eq!(rotation, original);
    }
}
#[test]
fn cycling_selects_each_choice_and_invoking_quarter_turns_retains_the_mode() {
    let ctx = egui::Context::default();
    let mut rotation = Rotation {
        degrees: 12.,
        ..Default::default()
    };
    let mut crop = Crop::default();
    activate(&ctx, Mode::Straighten);
    crate::photo_tools::take_view_change(&ctx);
    for mode in [Mode::Freehand, Mode::Left, Mode::Right, Mode::Straighten] {
        cycle(&ctx);
        assert_eq!(state(&ctx).mode, mode);
        assert!(active(&ctx));
        assert_eq!(editing(&ctx), mode.gesture());
        assert_eq!(rotation.turns, 0, "selection does not edit the photograph");
        crate::photo_tools::take_view_change(&ctx);
    }
    activate(&ctx, Mode::Left);
    invoke(&ctx, &mut rotation, &mut crop);
    assert_eq!(rotation.turns, 3);
    invoke(&ctx, &mut rotation, &mut crop);
    assert_eq!(rotation.turns, 2);
    assert_eq!(state(&ctx).mode, Mode::Left);
    cycle(&ctx);
    invoke(&ctx, &mut rotation, &mut crop);
    assert_eq!(rotation.turns, 3);
    assert_eq!(rotation.degrees, 12.);
    assert_eq!(state(&ctx).mode, Mode::Right);
    assert!(active(&ctx));
    assert!(!editing(&ctx));
}
#[test]
fn straighten_leaves_the_photograph_still_while_freehand_previews_the_draft() {
    let ctx = egui::Context::default();
    for mode in [Mode::Straighten, Mode::Freehand] {
        activate(&ctx, mode);
        let original = Rotation::default();
        let draft = Rotation {
            degrees: 20.,
            ..original
        };
        let mut s = state(&ctx);
        s.gesture = Some(Gesture {
            phase: Phase::FirstPress,
            original,
            draft,
            start: egui::pos2(10., 10.),
            end: egui::pos2(50., 30.),
        });
        store(&ctx, s);
        assert_eq!(
            displayed(&ctx, original),
            if mode == Mode::Straighten {
                original
            } else {
                draft
            }
        );
    }
}
#[test]
fn escape_discards_only_the_draft_before_leaving_the_tool() {
    let ctx = egui::Context::default();
    activate(&ctx, Mode::Freehand);
    assert!(crate::photo_tools::take_view_change(&ctx));
    let mut s = state(&ctx);
    s.gesture = Some(Gesture {
        phase: Phase::FirstPress,
        original: Rotation::default(),
        draft: Rotation {
            degrees: 20.,
            ..Default::default()
        },
        start: egui::Pos2::ZERO,
        end: egui::pos2(20., 20.),
    });
    store(&ctx, s);
    escape(&ctx);
    assert!(active(&ctx));
    assert_eq!(displayed(&ctx, Rotation::default()), Rotation::default());
    assert!(!crate::photo_tools::take_view_change(&ctx));
    escape(&ctx);
    assert!(!active(&ctx));
    assert!(crate::photo_tools::take_view_change(&ctx));
}
#[test]
fn straightening_chooses_nearest_axis_and_freehand_preserves_existing_angle() {
    let center = egui::pos2(100., 100.);
    assert!(
        (angle(
            Mode::Straighten,
            3.,
            egui::pos2(0., 0.),
            egui::pos2(100., 10.),
            center
        ) - (3. - f64::from(0.1_f32.atan().to_degrees())))
        .abs()
            < 1e-4
    );
    assert!(
        (angle(
            Mode::Straighten,
            0.,
            egui::pos2(0., 0.),
            egui::pos2(10., 100.),
            center
        ) - f64::from(0.1_f32.atan().to_degrees()))
        .abs()
            < 1e-4
    );
    assert_eq!(
        angle(
            Mode::Freehand,
            5.,
            egui::pos2(200., 100.),
            egui::pos2(100., 200.),
            center
        ),
        45.
    );
}
