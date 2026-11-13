//! Retains per-photo viewing geometry independently of rendered pixels and editing selection.

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

    fn draw(
        views: &mut PhotoViews,
        ctx: &egui::Context,
        paths: &[&Path],
        time: f64,
        events: Vec<egui::Event>,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 400.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let width = ui.available_width() / paths.len() as f32;
                    for (index, path) in paths.iter().enumerate() {
                        let rect = egui::Rect::from_min_size(
                            ui.min_rect().min + egui::vec2(width * index as f32, 0.),
                            egui::vec2(width, ui.available_height()),
                        );
                        let mut child =
                            ui.new_child(egui::UiBuilder::new().id_salt(index).max_rect(rect));
                        let view = views.view_mut(Some(path));
                        view.show_preview(
                            &mut child,
                            (egui::TextureId::User(index as u64), egui::vec2(200., 150.)),
                            [1600, 1200],
                            Default::default(),
                        );
                    }
                });
            },
        );
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
    fn double_click_and_hand_drag_affect_only_the_hovered_photo() {
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
        crate::photo_tools::select(&ctx, crate::photo_tools::Tool::Hand);
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
