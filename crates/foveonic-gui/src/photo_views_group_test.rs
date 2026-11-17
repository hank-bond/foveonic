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
        let geometry = paths.map(|path| views.view(Some(path)).image_rectangles().unwrap());
        let (canvas, image) = geometry[source_index];
        let visible = canvas.intersect(image);
        let fraction = (point - visible.min) / visible.size();
        let anchors = geometry.map(|(canvas, image)| {
            let visible = canvas.intersect(image);
            (visible.min + fraction * visible.size() - image.min) / image.size()
        });
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
                anchors[index] + (before_centers[index] - anchors[index]) / factor,
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
            fraction + (egui::vec2(0.5, 0.5) - fraction) / factor,
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
fn shift_drag_keeps_screen_displacement_with_mixed_zoom_and_press_ownership() {
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
        let before = paths.map(|path| views.view(Some(path)).pan);
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
        let source_delta = views.view(Some(paths[0])).pan - before[0];
        assert!(source_delta.length() > 0.001);
        let expected = if press_shift {
            source_delta
        } else {
            egui::Vec2::ZERO
        };
        let actual = views.view(Some(paths[1])).pan - before[1];
        assert!((actual - expected).length() < 1e-3);
        assert_eq!(views.view(Some(paths[2])).pan, before[2]);
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
fn white_range_click_does_not_seed_group_double_click() {
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
    let before = paths.map(|path| {
        let view = views.view(Some(path));
        (view.zoom_percent(1.), center(view))
    });
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
    for (path, (zoom, position)) in paths.into_iter().zip(before) {
        assert_eq!(views.view(Some(path)).zoom_percent(1.), zoom);
        near(center(views.view(Some(path))), position);
    }
    assert_eq!(selection.paths(), &displayed);
}

/// Different tile and photograph aspects expose anchors copied from a peer's RAW coordinates.
fn draw_mixed_aspects(
    views: &mut PhotoViews,
    ctx: &egui::Context,
    time: f64,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> Vec<crate::viewport_selection::Request> {
    let paths = [
        Path::new("landscape.x3f"),
        Path::new("portrait.x3f"),
        Path::new("white.x3f"),
    ];
    let targets = paths[..2]
        .iter()
        .map(|path| path.to_path_buf())
        .collect::<Vec<_>>();
    let sizes = [
        egui::vec2(300., 480.),
        egui::vec2(420., 280.),
        egui::vec2(240., 480.),
    ];
    let extents = [[1800, 1200], [1200, 1800], [1800, 1200]];
    let mut requests = Vec::new();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 540.),
            )),
            time: Some(time),
            modifiers,
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut x = ui.min_rect().left();
                for (index, path) in paths.iter().enumerate() {
                    let rect =
                        egui::Rect::from_min_size(egui::pos2(x, ui.min_rect().top()), sizes[index]);
                    x += sizes[index].x;
                    let mut tile =
                        ui.new_child(egui::UiBuilder::new().id_salt(index).max_rect(rect));
                    let view = views.view_mut(Some(path));
                    view.editing = crate::photo_tools::takes_pointer(ctx);
                    view.group_target = index < 2;
                    view.show_preview(
                        &mut tile,
                        (egui::TextureId::User(index as u64), sizes[index]),
                        extents[index],
                        Default::default(),
                    );
                    let selecting = crate::photo_tools::selects_photos(ctx)
                        && !ctx.input(|input| input.key_down(egui::Key::Space));
                    if let Some(gesture) = view.response().and_then(|response| {
                        crate::photo_tools::selection_gesture(
                            &response,
                            view.selection_eligible(selecting),
                        )
                    }) {
                        requests.push(crate::viewport_selection::Request {
                            path: path.to_path_buf(),
                            gesture,
                        });
                    }
                }
            });
        },
    );
    views.apply_group_input(
        &targets,
        &paths.map(Path::to_path_buf),
        ctx.pixels_per_point(),
    );
    requests
}

#[test]
fn shift_zoom_preserves_each_photos_own_point_in_its_displayed_region() {
    use crate::photo_tools::{self, Tool};
    let paths = [
        Path::new("landscape.x3f"),
        Path::new("portrait.x3f"),
        Path::new("white.x3f"),
    ];
    for source in [0, 1] {
        for fraction in [egui::vec2(0.5, 0.5), egui::vec2(0.25, 0.7)] {
            for initial in [0, 1, 2] {
                for pinch in [false, true] {
                    for tool in [Tool::Pointer, Tool::Rotate] {
                        let mut views = PhotoViews::default();
                        let ctx = egui::Context::default();
                        photo_tools::select(&ctx, tool);
                        draw_mixed_aspects(&mut views, &ctx, 0., vec![], egui::Modifiers::NONE);
                        for index in 0..3 {
                            if initial == 1 || (initial == 2 && index == 1) || index == 2 {
                                let view = views.view_mut(Some(paths[index]));
                                view.zoom_by(4. + index as f32 * 2.);
                                view.pan_to(
                                    [
                                        egui::vec2(0.65, 0.35),
                                        egui::vec2(0.3, 0.7),
                                        egui::vec2(0.55, 0.45),
                                    ][index],
                                );
                            }
                        }
                        let space = if tool == Tool::Rotate {
                            vec![egui::Event::Key {
                                key: egui::Key::Space,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: egui::Modifiers::SHIFT,
                            }]
                        } else {
                            vec![]
                        };
                        draw_mixed_aspects(&mut views, &ctx, 0.1, space, egui::Modifiers::SHIFT);
                        let geometry =
                            paths.map(|path| views.view(Some(path)).image_rectangles().unwrap());
                        let zoom =
                            paths.map(|path| views.view(Some(path)).zoom_percent(1.).unwrap());
                        let pan = paths.map(|path| views.view(Some(path)).pan);
                        let visible = geometry.map(|(canvas, image)| canvas.intersect(image));
                        let points = visible.map(|rect| rect.min + fraction * rect.size());
                        let expected = std::array::from_fn::<_, 3, _>(|index| {
                            (points[index] - geometry[index].1.min) / geometry[index].1.size()
                        });
                        let events = if pinch {
                            vec![
                                egui::Event::PointerMoved(points[source]),
                                egui::Event::Zoom(1.5),
                            ]
                        } else {
                            wheel(points[source], egui::Modifiers::SHIFT)
                        };
                        assert!(draw_mixed_aspects(
                            &mut views,
                            &ctx,
                            0.2,
                            events,
                            egui::Modifiers::SHIFT
                        )
                        .is_empty());
                        let factor = if pinch {
                            1.5
                        } else {
                            (ctx.input(|input| input.smooth_scroll_delta.x) * 0.005).exp()
                        };
                        assert!(factor > 1.);
                        for index in 0..2 {
                            let view = views.view(Some(paths[index]));
                            let actual_zoom = view.zoom_percent(1.).unwrap();
                            assert!((actual_zoom / zoom[index] - factor).abs() < 1e-5);
                            let size = geometry[index].1.size() * factor;
                            let image = egui::Rect::from_center_size(
                                geometry[index].0.center() + view.pan,
                                size,
                            );
                            let actual = (points[index] - image.min) / image.size();
                            assert!((actual - expected[index]).length() < 2e-5,
                                "source={source} peer={index} fraction={fraction:?} initial={initial} pinch={pinch} tool={tool:?}: expected own point ({:.6}, {:.6}), got ({:.6}, {:.6})", expected[index].x, expected[index].y, actual.x, actual.y);
                        }
                        assert_eq!(views.view(Some(paths[2])).zoom_percent(1.), Some(zoom[2]));
                        assert_eq!(views.view(Some(paths[2])).pan, pan[2]);
                        assert_eq!(photo_tools::selected(&ctx), tool);
                    }
                }
            }
        }
    }
}

#[test]
fn shift_drag_moves_each_displayed_photo_the_same_screen_distance() {
    use crate::photo_tools::{self, Tool};
    let paths = [
        Path::new("landscape.x3f"),
        Path::new("portrait.x3f"),
        Path::new("white.x3f"),
    ];
    for source in [0, 1] {
        for zoomed in [false, true] {
            for pixels_per_point in [1., 2.] {
                for tool in [Tool::Pointer, Tool::Rotate] {
                    let mut views = PhotoViews::default();
                    let ctx = egui::Context::default();
                    ctx.set_pixels_per_point(pixels_per_point);
                    photo_tools::select(&ctx, tool);
                    draw_mixed_aspects(&mut views, &ctx, 0., vec![], egui::Modifiers::NONE);
                    draw_mixed_aspects(&mut views, &ctx, 0.01, vec![], egui::Modifiers::NONE);
                    if zoomed {
                        for (index, path) in paths.iter().enumerate() {
                            let view = views.view_mut(Some(path));
                            view.zoom_by(4. + index as f32 * 2.);
                            view.pan_to(
                                [
                                    egui::vec2(0.65, 0.35),
                                    egui::vec2(0.3, 0.7),
                                    egui::vec2(0.55, 0.45),
                                ][index],
                            );
                        }
                    }
                    let space = if tool == Tool::Rotate {
                        vec![egui::Event::Key {
                            key: egui::Key::Space,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::SHIFT,
                        }]
                    } else {
                        vec![]
                    };
                    draw_mixed_aspects(&mut views, &ctx, 0.1, space, egui::Modifiers::SHIFT);
                    let before = paths.map(|path| views.view(Some(path)).pan);
                    let zoom =
                        paths.map(|path| views.view(Some(path)).zoom_percent(pixels_per_point));
                    let point = views
                        .view(Some(paths[source]))
                        .response()
                        .unwrap()
                        .rect
                        .center();
                    assert!(draw_mixed_aspects(
                        &mut views,
                        &ctx,
                        0.2,
                        button(point, true, egui::Modifiers::SHIFT),
                        egui::Modifiers::SHIFT,
                    )
                    .is_empty());
                    for (time, delta) in [(0.3, egui::vec2(17., -11.)), (0.4, egui::vec2(41., 25.))]
                    {
                        assert!(draw_mixed_aspects(
                            &mut views,
                            &ctx,
                            time,
                            vec![egui::Event::PointerMoved(point + delta)],
                            egui::Modifiers::SHIFT,
                        )
                        .is_empty());
                        for index in 0..2 {
                            let actual = views.view(Some(paths[index])).pan - before[index];
                            assert!((actual - delta).length() < 1e-3,
                                "source={source} peer={index} zoomed={zoomed} density={pixels_per_point} tool={tool:?}: expected movement ({:.6}, {:.6}), got ({:.6}, {:.6})", delta.x, delta.y, actual.x, actual.y);
                            assert_eq!(
                                views
                                    .view(Some(paths[index]))
                                    .zoom_percent(pixels_per_point),
                                zoom[index]
                            );
                        }
                        assert_eq!(views.view(Some(paths[2])).pan, before[2]);
                        assert_eq!(
                            views.view(Some(paths[2])).zoom_percent(pixels_per_point),
                            zoom[2]
                        );
                    }
                    assert!(draw_mixed_aspects(
                        &mut views,
                        &ctx,
                        0.5,
                        button(point + egui::vec2(41., 25.), false, egui::Modifiers::SHIFT),
                        egui::Modifiers::SHIFT,
                    )
                    .is_empty());
                    assert_eq!(photo_tools::selected(&ctx), tool);
                }
            }
        }
    }
}
