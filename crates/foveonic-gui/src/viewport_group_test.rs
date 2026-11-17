//! Checks composed group viewing intent independently of available layout geometry.

use super::*;

fn center(view: &Viewport, size: egui::Vec2, extent: [u32; 2]) -> egui::Vec2 {
    let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let image = view.image_rect(canvas, extent);
    (canvas.center() - image.min) / image.size()
}

fn near(actual: egui::Vec2, expected: egui::Vec2) {
    assert!(
        (actual - expected).length() < 2e-5,
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn detached_relative_changes_keep_their_order_through_carry_and_new_geometry() {
    for pixels_per_point in [1., 2.] {
        let size = egui::vec2(800., 600.);
        let extent = [1600, 1200];
        let scale = 1. / pixels_per_point;
        let make = || Viewport {
            scale: Some(scale),
            pan: egui::vec2(-0.1, 0.15) * egui::vec2(1600., 1200.) * scale,
            last_canvas: Some((size, extent)),
            ..Default::default()
        };
        let mut drawn = make();
        let mut detached = make();
        detached.detach_layout();
        for change in [
            ViewChange::Pan(egui::vec2(0.1, -0.05)),
            ViewChange::Zoom {
                factor: 2.,
                anchor: egui::vec2(0.25, 0.75),
            },
            ViewChange::Pan(egui::vec2(-0.025, 0.125)),
            ViewChange::Zoom {
                factor: 0.5,
                anchor: egui::vec2(0.8, 0.1),
            },
        ] {
            drawn.apply_view_change(change, pixels_per_point);
            detached.apply_view_change(change, pixels_per_point);
        }
        // The two different anchors leave a translation even though zoom returns to its start.
        near(center(&drawn, size, extent), egui::vec2(0., 0.8));
        detached.reframe(egui::Vec2::ZERO, [1200, 1600], pixels_per_point);
        let mut carried = Viewport::default();
        carried.set_transform(detached.transform());
        let new_size = egui::vec2(300., 700.);
        let new_extent = [1200, 1600];
        carried.reframe(new_size, new_extent, pixels_per_point);
        near(center(&carried, new_size, new_extent), egui::vec2(0., 0.8));
        assert_eq!(carried.zoom_percent(pixels_per_point), Some(100.));
        let pan = carried.pan;
        carried.reframe(new_size, new_extent, pixels_per_point);
        near(carried.pan, pan);
    }
}

#[test]
fn never_drawn_fit_retains_pan_and_distinct_zoom_anchors_until_geometry() {
    let mut view = Viewport::default();
    for change in [
        ViewChange::Pan(egui::vec2(0.1, -0.1)),
        ViewChange::Zoom {
            factor: 4.,
            anchor: egui::vec2(0.2, 0.8),
        },
        ViewChange::Zoom {
            factor: 0.5,
            anchor: egui::vec2(0.8, 0.2),
        },
        ViewChange::Pan(egui::vec2(0.1, 0.1)),
    ] {
        view.apply_view_change(change, 1.);
    }
    view.reframe(egui::Vec2::ZERO, [1600, 1200], 1.);
    view.detach_layout();
    let mut carried = Viewport::default();
    carried.set_transform(view.transform());
    let size = egui::vec2(800., 600.);
    carried.reframe(size, [1600, 1200], 1.);
    assert_eq!(carried.zoom_percent(1.), Some(100.));
    near(center(&carried, size, [1600, 1200]), egui::vec2(-0.4, 1.2));
    assert!(
        carried.visible_pixels().is_none(),
        "relative pan stays unbounded"
    );
}

#[test]
fn fixed_zoom_clamps_preserve_the_anchor_using_the_effective_multiplier() {
    for pixels_per_point in [1., 2.] {
        let size = egui::vec2(800., 600.);
        let extent = [1600, 1200];
        for (scale, start, factor, anchor, expected_scale, expected_center) in [
            (
                32.,
                egui::vec2(0.3, 0.8),
                4.,
                egui::vec2(-0.25, 1.25),
                64.,
                egui::vec2(0.025, 1.025),
            ),
            (
                0.1,
                egui::vec2(0.6, 0.3),
                0.1,
                egui::vec2(0.8, 0.2),
                0.05,
                egui::vec2(0.4, 0.4),
            ),
        ] {
            let mut view = Viewport {
                scale: Some(scale / pixels_per_point),
                pan: (egui::vec2(0.5, 0.5) - start)
                    * egui::vec2(1600., 1200.)
                    * (scale / pixels_per_point),
                last_canvas: Some((size, extent)),
                ..Default::default()
            };
            view.apply_view_change(ViewChange::Zoom { factor, anchor }, pixels_per_point);
            assert_eq!(
                view.zoom_percent(pixels_per_point),
                Some(expected_scale * 100.)
            );
            near(center(&view, size, extent), expected_center);
            let pan = view.pan;
            view.apply_view_change(ViewChange::Zoom { factor, anchor }, pixels_per_point);
            near(view.pan, pan);
        }
    }
}

#[test]
fn unknown_fit_defers_absolute_clamping_until_its_first_positive_layout() {
    for pixels_per_point in [1., 2.] {
        let mut unknown = Viewport::default();
        let mut known = Viewport::default();
        let size = egui::vec2(400., 300.);
        let extent = [800, 600];
        known.reframe(size, extent, pixels_per_point);
        for factor in [1024., 0.5] {
            let change = ViewChange::Zoom {
                factor,
                anchor: egui::vec2(0.5, 0.5),
            };
            unknown.apply_view_change(change, pixels_per_point);
            known.apply_view_change(change, pixels_per_point);
        }
        unknown.reframe(egui::Vec2::ZERO, extent, pixels_per_point);
        let mut carried = Viewport::default();
        carried.set_transform(unknown.transform());
        carried.reframe(size, extent, pixels_per_point);
        assert_eq!(carried.zoom_percent(pixels_per_point), Some(6400.));
        assert_eq!(known.zoom_percent(pixels_per_point), Some(3200.));
        near(center(&carried, size, extent), egui::vec2(0.5, 0.5));
    }
}

#[test]
fn fit_and_common_snap_replace_all_earlier_pending_relative_intent() {
    let size = egui::vec2(600., 400.);
    let extent = [1200, 800];
    for destination in [
        ViewChange::Fit,
        ViewChange::Snap {
            scale: 1.5,
            center: egui::vec2(0.2, 0.7),
        },
    ] {
        let mut view = Viewport::default();
        view.apply_view_change(ViewChange::Pan(egui::vec2(0.4, -0.3)), 2.);
        view.apply_view_change(
            ViewChange::Zoom {
                factor: 8.,
                anchor: egui::vec2(0.1, 0.9),
            },
            2.,
        );
        view.apply_view_change(destination, 2.);
        view.reframe(size, extent, 2.);
        match destination {
            ViewChange::Fit => {
                assert!(!view.explicit_zoom());
                near(center(&view, size, extent), egui::vec2(0.5, 0.5));
            }
            ViewChange::Snap { .. } => {
                assert_eq!(view.zoom_percent(2.), Some(300.));
                near(center(&view, size, extent), egui::vec2(0.2, 0.7));
            }
            _ => unreachable!(),
        }
    }
}
