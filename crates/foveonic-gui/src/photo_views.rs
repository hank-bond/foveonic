//! Retains per-photo viewing geometry and shares Shift viewing changes across editing targets.

use crate::viewport::{ViewTransform, Viewport};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct PhotoViews {
    photos: HashMap<PathBuf, Viewport>,
    empty: Viewport,
}

impl PhotoViews {
    pub fn view(&self, path: Option<&Path>) -> &Viewport {
        path.and_then(|path| self.photos.get(path))
            .unwrap_or(&self.empty)
    }

    pub fn view_mut(&mut self, path: Option<&Path>) -> &mut Viewport {
        match path {
            Some(path) => self
                .photos
                .entry(path.to_owned())
                .or_insert_with(|| Viewport::for_photo(path)),
            None => &mut self.empty,
        }
    }

    /// Fan out source-owned changes once, after all tiles have interpreted input.
    pub fn apply_group_input(
        &mut self,
        targets: &[PathBuf],
        displayed: &[PathBuf],
        pixels_per_point: f32,
    ) -> bool {
        let mut changes = Vec::new();
        for path in targets {
            if let Some(view) = self.photos.get_mut(path) {
                changes.extend(
                    view.take_group_changes()
                        .into_iter()
                        .map(|change| (path.clone(), change)),
                );
            }
        }
        let changed = !changes.is_empty();
        for (source, change) in changes {
            for path in targets.iter().filter(|path| **path != source) {
                let peer = self.view_mut(Some(path));
                if !displayed.contains(path) {
                    peer.detach_layout();
                }
                peer.apply_view_change(change, pixels_per_point);
            }
        }
        changed
    }

    pub fn select(&mut self, path: &Path, carried: Option<ViewTransform>) {
        let view = self.view_mut(Some(path));
        if let Some(transform) = carried {
            view.set_transform(transform);
        } else {
            // Reuse this photograph's transform, without borrowing its last
            // screen rectangle for a render before the new layout is drawn.
            view.detach_layout();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    pub(super) fn draw(
        views: &mut PhotoViews,
        ctx: &egui::Context,
        paths: &[&Path],
        time: f64,
        events: Vec<egui::Event>,
    ) -> Vec<crate::viewport_selection::Request> {
        draw_with_modifiers(views, ctx, paths, time, events, egui::Modifiers::NONE)
    }

    fn draw_with_modifiers(
        views: &mut PhotoViews,
        ctx: &egui::Context,
        paths: &[&Path],
        time: f64,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> Vec<crate::viewport_selection::Request> {
        draw_targets(views, ctx, paths, &[], time, events, modifiers)
    }

    pub(super) fn draw_targets(
        views: &mut PhotoViews,
        ctx: &egui::Context,
        paths: &[&Path],
        targets: &[std::path::PathBuf],
        time: f64,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> Vec<crate::viewport_selection::Request> {
        let mut requests = Vec::new();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 400.),
                )),
                time: Some(time),
                modifiers,
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let selecting = crate::photo_tools::selects_photos(ctx)
                        && !ui.input(|input| input.key_down(egui::Key::Space));
                    let width = ui.available_width() / paths.len() as f32;
                    for (index, path) in paths.iter().enumerate() {
                        let rect = egui::Rect::from_min_size(
                            ui.min_rect().min + egui::vec2(width * index as f32, 0.),
                            egui::vec2(width, ui.available_height()),
                        );
                        let mut child =
                            ui.new_child(egui::UiBuilder::new().id_salt(index).max_rect(rect));
                        let view = views.view_mut(Some(path));
                        view.editing = crate::photo_tools::takes_pointer(ctx);
                        view.group_target =
                            targets.len() > 1 && targets.iter().any(|target| target == *path);
                        view.show_preview(
                            &mut child,
                            (egui::TextureId::User(index as u64), egui::vec2(200., 150.)),
                            [1600, 1200],
                            Default::default(),
                        );
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
            targets,
            &paths
                .iter()
                .map(|path| path.to_path_buf())
                .collect::<Vec<_>>(),
            ctx.pixels_per_point(),
        );
        requests
    }

    fn group_draw(
        views: &mut PhotoViews,
        ctx: &egui::Context,
        paths: &[&Path],
        time: f64,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> Vec<crate::viewport_selection::Request> {
        let targets = paths
            .iter()
            .map(|path| path.to_path_buf())
            .collect::<Vec<_>>();
        draw_targets(views, ctx, paths, &targets, time, events, modifiers)
    }

    fn click(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
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
    fn shift_group_pinch_keeps_relative_zoom_for_every_target() {
        let mut views = PhotoViews::default();
        let ctx = egui::Context::default();
        let paths = [Path::new("a.x3f"), Path::new("b.x3f")];
        draw(&mut views, &ctx, &paths, 0., vec![]);
        views.view_mut(Some(paths[1])).zoom_by(2.);
        draw(&mut views, &ctx, &paths, 0.1, vec![]);
        let before = paths.map(|path| views.view(Some(path)).zoom_percent(1.).unwrap());
        let point = views.view(Some(paths[0])).response().unwrap().rect.center();
        group_draw(
            &mut views,
            &ctx,
            &paths,
            0.2,
            vec![egui::Event::PointerMoved(point), egui::Event::Zoom(1.5)],
            egui::Modifiers::SHIFT,
        );
        for (index, path) in paths.iter().enumerate() {
            let ratio = views.view(Some(path)).zoom_percent(1.).unwrap() / before[index];
            assert!(
                (ratio - 1.5).abs() < 1e-5,
                "Shift zoom must apply once to every blue photo: {index} changed by {ratio}"
            );
        }
    }

    #[test]
    fn shift_group_double_click_uses_one_snap_then_fit_destination() {
        let mut views = PhotoViews::default();
        let ctx = egui::Context::default();
        let paths = [Path::new("a.x3f"), Path::new("b.x3f")];
        draw(&mut views, &ctx, &paths, 0., vec![]);
        views.view_mut(Some(paths[1])).zoom_by(8.);
        let point = views.view(Some(paths[0])).response().unwrap().rect.center();
        let mut requests = Vec::new();
        for (time, pressed) in [(0.1, true), (0.15, false), (0.2, true), (0.25, false)] {
            requests.extend(group_draw(
                &mut views,
                &ctx,
                &paths,
                time,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::SHIFT,
                    },
                ],
                egui::Modifiers::SHIFT,
            ));
        }
        assert!(
            requests.is_empty(),
            "Shift double-click must preserve the editing group"
        );
        for path in paths {
            assert_eq!(views.view(Some(path)).zoom_percent(1.), Some(200.));
        }
        for (time, pressed) in [(1.1, true), (1.15, false), (1.2, true), (1.25, false)] {
            group_draw(
                &mut views,
                &ctx,
                &paths,
                time,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::SHIFT,
                    },
                ],
                egui::Modifiers::SHIFT,
            );
        }
        for path in paths {
            assert!(
                !views.view(Some(path)).explicit_zoom(),
                "Every photo returns to Fit"
            );
        }
    }

    #[test]
    fn pointer_click_modifiers_select_on_release_and_drags_preserve_editing_targets() {
        use crate::viewport_selection::{Gesture, Request, Selection};
        for modifiers in [
            egui::Modifiers::NONE,
            egui::Modifiers::ALT,
            egui::Modifiers::SHIFT,
            egui::Modifiers::SHIFT.plus(egui::Modifiers::ALT),
        ] {
            for drag in [false, true] {
                let mut views = PhotoViews::default();
                let ctx = egui::Context::default();
                let paths = [Path::new("a.x3f"), Path::new("b.x3f"), Path::new("c.x3f")];
                let mut selection = Selection::default();
                selection.sync_browser(&paths.map(Path::to_path_buf), 8);
                selection.apply(
                    Request {
                        path: paths[0].into(),
                        gesture: Gesture::Replace,
                    },
                    8,
                );
                draw(&mut views, &ctx, &paths, 0., vec![]);
                let point = views.view(Some(paths[2])).response().unwrap().rect.center();
                let before_zoom = views.view(Some(paths[2])).zoom_percent(1.);
                let end = point
                    + if drag {
                        egui::vec2(40., 20.)
                    } else {
                        egui::vec2(1., 1.)
                    };
                let button = |pos, pressed| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                };
                let requests = draw_with_modifiers(
                    &mut views,
                    &ctx,
                    &paths,
                    0.1,
                    vec![egui::Event::PointerMoved(point), button(point, true)],
                    modifiers,
                );
                assert!(requests.is_empty(), "press must not select");
                let requests = draw_with_modifiers(
                    &mut views,
                    &ctx,
                    &paths,
                    0.2,
                    vec![egui::Event::PointerMoved(end)],
                    modifiers,
                );
                assert!(requests.is_empty(), "movement must not select");
                let requests = draw_with_modifiers(
                    &mut views,
                    &ctx,
                    &paths,
                    0.3,
                    vec![button(end, false)],
                    modifiers,
                );
                if drag {
                    assert!(
                        requests.is_empty(),
                        "drag release must not select: {modifiers:?}"
                    );
                    assert!(views.view(Some(paths[2])).pan.length() > 30.);
                    assert_eq!(selection.paths(), &[paths[0].to_path_buf()]);
                } else {
                    assert_eq!(requests.len(), 1);
                    assert_eq!(requests[0].path, paths[2]);
                    assert_eq!(
                        requests[0].gesture,
                        if modifiers.shift {
                            Gesture::Range
                        } else if modifiers.alt {
                            Gesture::Toggle
                        } else {
                            Gesture::Replace
                        }
                    );
                    selection.apply(requests.into_iter().next().unwrap(), 8);
                    let expected = if modifiers.shift {
                        vec![paths[0], paths[1], paths[2]]
                    } else if modifiers.alt {
                        vec![paths[0], paths[2]]
                    } else {
                        vec![paths[2]]
                    };
                    assert_eq!(
                        selection.paths(),
                        expected
                            .into_iter()
                            .map(Path::to_path_buf)
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(views.view(Some(paths[2])).zoom_percent(1.), before_zoom);
                    assert_eq!(views.view(Some(paths[2])).pan, egui::Vec2::ZERO);
                }
                assert_eq!(views.view(Some(paths[0])).pan, egui::Vec2::ZERO);
                assert_eq!(views.view(Some(paths[1])).pan, egui::Vec2::ZERO);
                assert_eq!(
                    crate::photo_tools::selected(&ctx),
                    crate::photo_tools::Tool::Pointer
                );
            }
        }
    }

    #[test]
    fn tool_clicks_and_temporary_space_never_request_viewport_selection() {
        use crate::photo_tools::{self, Tool};
        for tool in [
            Tool::Pointer,
            Tool::Loupe,
            Tool::Crop,
            Tool::Rotate,
            Tool::WhiteBalance,
        ] {
            for space in [false, true] {
                if tool == Tool::Pointer && !space {
                    continue;
                }
                let mut views = PhotoViews::default();
                let ctx = egui::Context::default();
                photo_tools::select(&ctx, tool);
                let paths = [Path::new("a.x3f"), Path::new("b.x3f")];
                draw(&mut views, &ctx, &paths, 0., vec![]);
                let point = views.view(Some(paths[1])).response().unwrap().rect.center();
                let key = |pressed| egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                };
                if space {
                    draw(&mut views, &ctx, &paths, 0.05, vec![key(true)]);
                }
                assert!(draw(&mut views, &ctx, &paths, 0.1, click(point, true)).is_empty());
                assert!(draw(&mut views, &ctx, &paths, 0.2, click(point, false)).is_empty());
                if space {
                    assert!(draw(&mut views, &ctx, &paths, 0.3, vec![key(false)]).is_empty());
                }
                assert_eq!(views.view(Some(paths[1])).pan, egui::Vec2::ZERO);
                assert_eq!(photo_tools::selected(&ctx), tool);
            }
        }
    }

    #[test]
    fn each_photo_keeps_zoom_and_center_through_selection_and_layout_changes() {
        let mut views = PhotoViews::default();
        let ctx = egui::Context::default();
        let a = Path::new("a.x3f");
        let b = Path::new("b.x3f");
        draw(&mut views, &ctx, &[a, b], 0., vec![]);
        views.view_mut(Some(a)).zoom_by(8.);
        views.view_mut(Some(a)).pan_to(egui::vec2(0.7, 0.3));
        let before_zoom = views.view(Some(a)).zoom_percent(1.);
        draw(&mut views, &ctx, &[a, b], 0.05, vec![]);
        let center = |view: &Viewport| {
            let (canvas, image) = view.image_rectangles().unwrap();
            (canvas.center() - image.min) / image.size()
        };
        let before_center = center(views.view(Some(a)));
        let id = views.view(Some(a)).id();
        assert!(!views.view(Some(b)).explicit_zoom());
        views.select(b, None);
        draw(&mut views, &ctx, &[b], 0.1, vec![]);
        views.select(a, None);
        draw(&mut views, &ctx, &[a], 0.2, vec![]);
        assert_eq!(views.view(Some(a)).id(), id);
        assert_eq!(before_zoom, views.view(Some(a)).zoom_percent(1.));
        assert!((before_center - center(views.view(Some(a)))).length() < 1e-6);
        draw(&mut views, &ctx, &[b, a], 0.3, vec![]);
        assert_eq!(views.view(Some(a)).id(), id);
        assert!(views.view(Some(a)).explicit_zoom());
        assert!(!views.view(Some(b)).explicit_zoom());
    }

    #[test]
    fn explicit_carry_copies_only_the_view_transform() {
        let mut views = PhotoViews::default();
        let ctx = egui::Context::default();
        let a = Path::new("a.x3f");
        let b = Path::new("b.x3f");
        draw(&mut views, &ctx, &[a], 0., vec![]);
        views.view_mut(Some(a)).zoom_by(4.);
        let transform = views.view(Some(a)).transform();
        views.select(b, Some(transform));
        assert_ne!(views.view(Some(a)).id(), views.view(Some(b)).id());
        assert!(views.view(Some(b)).response().is_none());
        draw(&mut views, &ctx, &[b], 0.1, vec![]);
        assert_eq!(
            views.view(Some(a)).zoom_percent(1.),
            views.view(Some(b)).zoom_percent(1.)
        );
        views.view_mut(Some(b)).reset();
        assert!(views.view(Some(a)).explicit_zoom());
        assert!(!views.view(Some(b)).explicit_zoom());
    }

    #[test]
    fn rapid_clicks_on_different_photos_do_not_zoom_the_second() {
        for modifiers in [
            egui::Modifiers::NONE,
            egui::Modifiers::ALT,
            egui::Modifiers::SHIFT,
            egui::Modifiers::COMMAND,
        ] {
            let mut views = PhotoViews::default();
            let ctx = egui::Context::default();
            let a = Path::new("a.x3f");
            let b = Path::new("b.x3f");
            draw(&mut views, &ctx, &[a, b], 0., vec![]);
            let first = views.view(Some(a)).response().unwrap().rect.center();
            let second = views.view(Some(b)).response().unwrap().rect.center();
            draw(&mut views, &ctx, &[a, b], 0.1, click(first, true));
            draw(&mut views, &ctx, &[a, b], 0.15, click(first, false));
            let button = |pressed| egui::Event::PointerButton {
                pos: second,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            };
            draw_with_modifiers(
                &mut views,
                &ctx,
                &[a, b],
                0.2,
                vec![egui::Event::PointerMoved(second), button(true)],
                modifiers,
            );
            draw_with_modifiers(
                &mut views,
                &ctx,
                &[a, b],
                0.25,
                vec![button(false)],
                modifiers,
            );
            assert!(!views.view(Some(a)).explicit_zoom());
            assert!(!views.view(Some(b)).explicit_zoom());
            draw_with_modifiers(
                &mut views,
                &ctx,
                &[a, b],
                0.3,
                vec![button(true)],
                modifiers,
            );
            draw_with_modifiers(
                &mut views,
                &ctx,
                &[a, b],
                0.35,
                vec![button(false)],
                modifiers,
            );
            assert_eq!(
                views.view(Some(b)).explicit_zoom(),
                modifiers == egui::Modifiers::NONE
            );
        }
    }

    #[test]
    fn coalesced_second_click_still_zooms_the_same_photo() {
        for coalesce_first in [false, true] {
            let mut views = PhotoViews::default();
            let ctx = egui::Context::default();
            let a = Path::new("a.x3f");
            draw(&mut views, &ctx, &[a], 0., vec![]);
            let point = views.view(Some(a)).response().unwrap().rect.center();
            if coalesce_first {
                let mut events = click(point, true);
                events.extend(click(point, false));
                draw(&mut views, &ctx, &[a], 0.15, events);
            } else {
                draw(&mut views, &ctx, &[a], 0.1, click(point, true));
                draw(&mut views, &ctx, &[a], 0.15, click(point, false));
            }
            let mut events = click(point, true);
            events.extend(click(point, false));
            draw(&mut views, &ctx, &[a], 0.2, events);
            assert!(views.view(Some(a)).explicit_zoom());
        }
    }

    #[test]
    fn slow_second_click_does_not_zoom() {
        let mut views = PhotoViews::default();
        let ctx = egui::Context::default();
        let a = Path::new("a.x3f");
        draw(&mut views, &ctx, &[a], 0., vec![]);
        let point = views.view(Some(a)).response().unwrap().rect.center();
        for (time, pressed) in [(0.1, true), (0.15, false), (0.2, true), (0.6, false)] {
            draw(&mut views, &ctx, &[a], time, click(point, pressed));
        }
        assert!(!views.view(Some(a)).explicit_zoom());
    }

    #[test]
    fn tool_click_does_not_prime_pointer_zoom() {
        for tool in [
            crate::photo_tools::Tool::Loupe,
            crate::photo_tools::Tool::Crop,
        ] {
            let mut views = PhotoViews::default();
            let ctx = egui::Context::default();
            let a = Path::new("a.x3f");
            crate::photo_tools::select(&ctx, tool);
            draw(&mut views, &ctx, &[a], 0., vec![]);
            let point = views.view(Some(a)).response().unwrap().rect.center();
            draw(&mut views, &ctx, &[a], 0.1, click(point, true));
            draw(&mut views, &ctx, &[a], 0.15, click(point, false));
            crate::photo_tools::pointer(&ctx);
            draw(&mut views, &ctx, &[a], 0.2, click(point, true));
            draw(&mut views, &ctx, &[a], 0.25, click(point, false));
            assert!(!views.view(Some(a)).explicit_zoom());
        }
    }

    #[test]
    fn rapid_clicks_far_apart_on_one_photo_do_not_zoom() {
        let mut views = PhotoViews::default();
        let ctx = egui::Context::default();
        let a = Path::new("a.x3f");
        draw(&mut views, &ctx, &[a], 0., vec![]);
        let rect = views.view(Some(a)).response().unwrap().rect;
        let first = rect.center() - egui::vec2(50., 0.);
        let second = rect.center() + egui::vec2(50., 0.);
        for (time, pos, pressed) in [
            (0.1, first, true),
            (0.15, first, false),
            (0.2, second, true),
            (0.25, second, false),
        ] {
            draw(&mut views, &ctx, &[a], time, click(pos, pressed));
        }
        assert!(!views.view(Some(a)).explicit_zoom());
    }

    #[test]
    fn double_click_and_pointer_drag_affect_only_the_hovered_photo() {
        let mut views = PhotoViews::default();
        let ctx = egui::Context::default();
        let a = Path::new("a.x3f");
        let b = Path::new("b.x3f");
        draw(&mut views, &ctx, &[a, b], 0., vec![]);
        let point = views.view(Some(b)).response().unwrap().rect.center();
        for (time, pressed) in [(0.1, true), (0.15, false), (0.2, true), (0.25, false)] {
            draw(&mut views, &ctx, &[a, b], time, click(point, pressed));
        }
        assert!(!views.view(Some(a)).explicit_zoom());
        assert!(views.view(Some(b)).explicit_zoom());
        draw(&mut views, &ctx, &[a, b], 1., click(point, true));
        draw(
            &mut views,
            &ctx,
            &[a, b],
            1.1,
            vec![egui::Event::PointerMoved(point + egui::vec2(40., 20.))],
        );
        draw(
            &mut views,
            &ctx,
            &[a, b],
            1.2,
            click(point + egui::vec2(40., 20.), false),
        );
        assert_eq!(views.view(Some(a)).pan, egui::Vec2::ZERO);
        assert!(views.view(Some(b)).pan.length() > 30.);
        draw(&mut views, &ctx, &[b, a], 2., vec![]);
        let point = views.view(Some(b)).response().unwrap().rect.center();
        for (time, pressed) in [(2.1, true), (2.15, false), (2.2, true), (2.25, false)] {
            draw(&mut views, &ctx, &[b, a], time, click(point, pressed));
        }
        assert!(!views.view(Some(b)).explicit_zoom());
        assert_eq!(views.view(Some(b)).pan, egui::Vec2::ZERO);
    }
}

#[cfg(test)]
#[path = "photo_views_group_test.rs"]
mod group_tests;
