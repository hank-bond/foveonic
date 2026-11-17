//! Exercises group viewing through real pointer input and per-photo selection ownership.

use super::{
    tests::{draw, draw_targets},
    *,
};
use eframe::egui;

fn near(actual: egui::Vec2, expected: egui::Vec2) {
    assert!(
        (actual - expected).length() < 2e-5,
        "expected {expected:?}, got {actual:?}"
    );
}

fn center(view: &Viewport) -> egui::Vec2 {
    let scale = view.zoom_percent(1.).unwrap() / 100.;
    egui::vec2(0.5, 0.5) - view.pan / (egui::vec2(1600., 1200.) * scale)
}

fn button(pos: egui::Pos2, pressed: bool, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        },
    ]
}

fn wheel(pos: egui::Pos2, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0., 80.),
            modifiers,
        },
    ]
}

#[test]
fn shift_vertical_wheel_changes_each_blue_once_in_either_draw_order_including_hidden() {
    for source_index in [0, 1] {
        let mut views = PhotoViews::default();
        let ctx = egui::Context::default();
        let paths = [
            Path::new("a.x3f"),
            Path::new("b.x3f"),
            Path::new("white.x3f"),
        ];
        let hidden = Path::new("hidden.x3f");
        let targets = [paths[0].to_owned(), paths[1].to_owned(), hidden.to_owned()];
        draw(&mut views, &ctx, &paths, 0., vec![]);
        let centers = [
            egui::vec2(0.6, 0.4),
            egui::vec2(0.35, 0.65),
            egui::vec2(0.55, 0.45),
        ];
        for (index, path) in paths.iter().enumerate() {
            let view = views.view_mut(Some(path));
            view.zoom_by(4. + index as f32 * 2.);
            view.pan_to(centers[index]);
        }
        draw_targets(
            &mut views,
            &ctx,
            &paths,
            &targets,
            0.1,
            vec![],
            egui::Modifiers::SHIFT,
        );
        let before = paths.map(|path| views.view(Some(path)).zoom_percent(1.).unwrap());
        let before_centers = paths.map(|path| center(views.view(Some(path))));
        let point = views
            .view(Some(paths[source_index]))
            .response()
            .unwrap()
            .rect
            .center()
            + egui::vec2(17., -11.);
        let (_, image) = views
            .view(Some(paths[source_index]))
            .image_rectangles()
            .unwrap();
        let anchor = (point - image.min) / image.size();
        assert!(draw_targets(
            &mut views,
            &ctx,
            &paths,
            &targets,
            0.2,
            wheel(point, egui::Modifiers::SHIFT),
            egui::Modifiers::SHIFT,
        )
        .is_empty());
        let scroll = ctx.input(|input| input.smooth_scroll_delta);
        assert!(
            scroll.x > 0. && scroll.y == 0.,
            "egui must exercise Shift's horizontal scroll translation"
        );
        let factor = (scroll.x * 0.005).exp();
        for index in [0, 1] {
            let ratio = views.view(Some(paths[index])).zoom_percent(1.).unwrap() / before[index];
            assert!(
                (ratio - factor).abs() < 1e-5,
                "source {source_index}, peer {index}: {ratio} vs {factor}"
            );
            near(
                center(views.view(Some(paths[index]))),
                anchor + (before_centers[index] - anchor) / factor,
            );
        }
        assert_eq!(views.view(Some(paths[2])).zoom_percent(1.), Some(before[2]));
        near(center(views.view(Some(paths[2]))), before_centers[2]);
        assert!(
            !views.apply_group_input(&targets, &paths.map(Path::to_path_buf), 1.),
            "delivery is consumed once"
        );
        // A fresh egui context avoids another frame of wheel smoothing on the source.
        let reveal = egui::Context::default();
        draw(&mut views, &reveal, &[hidden], 1., vec![]);
        let canvas = views.view(Some(hidden)).response().unwrap().rect.size();
        let fit = (canvas.x / 1600.).min(canvas.y / 1200.);
        let actual = views.view(Some(hidden)).zoom_percent(1.).unwrap();
        assert!((actual - fit * factor * 100.).abs() < 1e-4);
        near(
            center(views.view(Some(hidden))),
            anchor + (egui::vec2(0.5, 0.5) - anchor) / factor,
        );
    }
}

#[test]
fn blue_double_click_from_zoom_fits_both_zoomed_and_fit_peers_without_touching_white() {
    let mut views = PhotoViews::default();
    let ctx = egui::Context::default();
    let paths = [
        Path::new("a.x3f"),
        Path::new("b.x3f"),
        Path::new("white.x3f"),
    ];
    let targets = paths[..2]
        .iter()
        .map(|path| path.to_path_buf())
        .collect::<Vec<_>>();
    draw(&mut views, &ctx, &paths, 0., vec![]);
    views.view_mut(Some(paths[0])).zoom_by(4.);
    views.view_mut(Some(paths[0])).pan_to(egui::vec2(0.6, 0.4));
    views.view_mut(Some(paths[2])).zoom_by(8.);
    views.view_mut(Some(paths[2])).pan_to(egui::vec2(0.4, 0.65));
    let white_zoom = views.view(Some(paths[2])).zoom_percent(1.);
    let white_center = center(views.view(Some(paths[2])));
    assert!(!views.view(Some(paths[1])).explicit_zoom());
    let point = views.view(Some(paths[0])).response().unwrap().rect.center();
    for (time, pressed) in [(0.1, true), (0.15, false), (0.2, true), (0.25, false)] {
        assert!(
            draw_targets(
                &mut views,
                &ctx,
                &paths,
                &targets,
                time,
                button(point, pressed, egui::Modifiers::SHIFT),
                egui::Modifiers::SHIFT,
            )
            .is_empty(),
            "every click on an existing blue group preserves selection"
        );
    }
    for path in &paths[..2] {
        assert!(!views.view(Some(path)).explicit_zoom());
        near(center(views.view(Some(path))), egui::vec2(0.5, 0.5));
    }
    assert_eq!(views.view(Some(paths[2])).zoom_percent(1.), white_zoom);
    near(center(views.view(Some(paths[2]))), white_center);
}

#[test]
fn shift_drag_keeps_relative_displacement_with_mixed_zoom_and_press_ownership() {
    for (press_shift, move_shift) in [(true, true), (true, false), (false, true)] {
        let mut views = PhotoViews::default();
        let ctx = egui::Context::default();
        let paths = [
            Path::new("a.x3f"),
            Path::new("b.x3f"),
            Path::new("white.x3f"),
        ];
        let targets = paths[..2]
            .iter()
            .map(|path| path.to_path_buf())
            .collect::<Vec<_>>();
        draw(&mut views, &ctx, &paths, 0., vec![]);
        for (index, path) in paths.iter().enumerate() {
            let view = views.view_mut(Some(path));
            view.zoom_by(4. + index as f32 * 2.);
            view.pan_to(egui::vec2(0.6, 0.4));
        }
        let before = paths.map(|path| center(views.view(Some(path))));
        let initial_pan = views.view(Some(paths[0])).pan;
        let point = views.view(Some(paths[0])).response().unwrap().rect.center();
        let press_mods = if press_shift {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        };
        let move_mods = if move_shift {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        };
        assert!(draw_targets(
            &mut views,
            &ctx,
            &paths,
            &targets,
            0.1,
            button(point, true, press_mods),
            press_mods
        )
        .is_empty());
        let end = point + egui::vec2(41., 11.);
        assert!(draw_targets(
            &mut views,
            &ctx,
            &paths,
            &targets,
            0.2,
            vec![egui::Event::PointerMoved(end)],
            move_mods
        )
        .is_empty());
        assert!(draw_targets(
            &mut views,
            &ctx,
            &paths,
            &targets,
            0.3,
            button(end, false, move_mods),
            move_mods
        )
        .is_empty());
        near(
            views.view(Some(paths[0])).pan - initial_pan,
            egui::vec2(41., 11.),
        );
        let source_delta = center(views.view(Some(paths[0]))) - before[0];
        assert!(source_delta.length() > 0.001);
        near(
            center(views.view(Some(paths[1]))) - before[1],
            if press_shift {
                source_delta
            } else {
                egui::Vec2::ZERO
            },
        );
        near(center(views.view(Some(paths[2]))), before[2]);
    }
}

#[test]
fn plain_white_and_tool_wheel_input_stays_local() {
    use crate::photo_tools::{self, Tool};
    for (tool, source_index, modifiers, zooms) in [
        (Tool::Pointer, 0, egui::Modifiers::NONE, true),
        (Tool::Pointer, 2, egui::Modifiers::SHIFT, true),
        (Tool::WhiteBalance, 0, egui::Modifiers::SHIFT, true),
        (Tool::Loupe, 0, egui::Modifiers::SHIFT, false),
    ] {
        let mut views = PhotoViews::default();
        let ctx = egui::Context::default();
        photo_tools::select(&ctx, tool);
        let paths = [
            Path::new("a.x3f"),
            Path::new("b.x3f"),
            Path::new("white.x3f"),
        ];
        let targets = paths[..2]
            .iter()
            .map(|path| path.to_path_buf())
            .collect::<Vec<_>>();
        draw(&mut views, &ctx, &paths, 0., vec![]);
        let before = paths.map(|path| views.view(Some(path)).zoom_percent(1.).unwrap());
        let point = views
            .view(Some(paths[source_index]))
            .response()
            .unwrap()
            .rect
            .center();
        assert!(draw_targets(
            &mut views,
            &ctx,
            &paths,
            &targets,
            0.1,
            wheel(point, modifiers),
            modifiers
        )
        .is_empty());
        for (index, path) in paths.iter().enumerate() {
            let after = views.view(Some(path)).zoom_percent(1.).unwrap();
            if zooms && index == source_index {
                assert!(
                    after > before[index],
                    "{tool:?} source must receive its local zoom"
                );
            } else {
                assert_eq!(after, before[index], "{tool:?} must not zoom peer {index}");
            }
        }
        assert_eq!(photo_tools::selected(&ctx), tool);
    }
}

#[test]
fn white_range_click_then_blue_second_click_resets_the_current_group_once() {
    use crate::viewport_selection::{Gesture, Request, Selection};
    let mut views = PhotoViews::default();
    let ctx = egui::Context::default();
    let paths = [
        Path::new("a.x3f"),
        Path::new("b.x3f"),
        Path::new("white.x3f"),
    ];
    let displayed = paths.map(Path::to_path_buf);
    let mut selection = Selection::default();
    selection.sync_browser(&displayed, 2);
    draw(&mut views, &ctx, &paths, 0., vec![]);
    views.view_mut(Some(paths[0])).zoom_by(4.);
    views.view_mut(Some(paths[1])).zoom_by(8.);
    let point =
        views.view(Some(paths[2])).response().unwrap().rect.center() + egui::vec2(30., -20.);
    for (time, pressed) in [(0.1, true), (0.15, false)] {
        let requests = draw_targets(
            &mut views,
            &ctx,
            &paths,
            selection.paths(),
            time,
            button(point, pressed, egui::Modifiers::SHIFT),
            egui::Modifiers::SHIFT,
        );
        if pressed {
            assert!(requests.is_empty());
        } else {
            assert_eq!(
                requests,
                vec![Request {
                    path: paths[2].to_owned(),
                    gesture: Gesture::Range
                }]
            );
        }
        for request in requests {
            selection.apply_displayed(request, 8, &displayed);
        }
    }
    assert_eq!(selection.paths(), &displayed);
    assert!(!views.view(Some(paths[2])).explicit_zoom());
    for (time, pressed) in [(0.2, true), (0.25, false)] {
        assert!(draw_targets(
            &mut views,
            &ctx,
            &paths,
            selection.paths(),
            time,
            button(point, pressed, egui::Modifiers::SHIFT),
            egui::Modifiers::SHIFT
        )
        .is_empty());
    }
    let destination = center(views.view(Some(paths[2])));
    assert!((destination - egui::vec2(0.5, 0.5)).length() > 0.01);
    for path in paths {
        assert_eq!(views.view(Some(path)).zoom_percent(1.), Some(200.));
        near(center(views.view(Some(path))), destination);
    }
    assert_eq!(selection.paths(), &displayed);
}
