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
            ViewChange::Pan(egui::vec2(41., 25.)),
            ViewChange::Zoom {
                factor: 2.,
                anchor: egui::vec2(0.25, 0.75),
            },
            ViewChange::Pan(egui::vec2(-17., 11.)),
            ViewChange::Zoom {
                factor: 0.5,
                anchor: egui::vec2(0.8, 0.1),
            },
        ] {
            drawn.apply_view_change(change, pixels_per_point);
            detached.apply_view_change(change, pixels_per_point);
        }
        // The two different anchors leave a translation even though zoom returns to its start.
        let expected = if pixels_per_point == 1. {
            egui::vec2(0.4421875, 0.48708333)
        } else {
            egui::vec2(0.27828125, 0.648125)
        };
        near(center(&drawn, size, extent), expected);
        detached.reframe(egui::Vec2::ZERO, [1200, 1600], pixels_per_point);
        let mut carried = Viewport::default();
        carried.set_transform(detached.transform());
        let new_size = egui::vec2(300., 700.);
        let new_extent = [1200, 1600];
        carried.reframe(new_size, new_extent, pixels_per_point);
        near(center(&carried, new_size, new_extent), expected);
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
        ViewChange::Pan(egui::vec2(41., -25.)),
        ViewChange::Zoom {
            factor: 4.,
            anchor: egui::vec2(0.2, 0.8),
        },
        ViewChange::Zoom {
            factor: 0.5,
            anchor: egui::vec2(0.8, 0.2),
        },
        ViewChange::Pan(egui::vec2(-17., 11.)),
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
    // Unknown geometry retains the physical pan separately from image-relative zoom anchors.
    near(
        center(&carried, size, [1600, 1200]),
        egui::vec2(-0.25, 1.25) - egui::vec2(24., -14.) / egui::vec2(1600., 1200.),
    );
    assert!(carried.visible_pixels().is_none(), "pan stays unbounded");
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
                egui::vec2(0.3, 0.8) + egui::vec2(-0.005859375, 0.005859375) * pixels_per_point,
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
        view.apply_view_change(ViewChange::Pan(egui::vec2(41., -25.)), 2.);
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

#[test]
fn hidden_zoom_uses_last_displayed_aspect_and_each_updated_visible_center() {
    for extent in [[1800, 1200], [900, 1600]] {
        let size = egui::vec2(420., 280.);
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let mut view = Viewport {
            scale: Some(1.),
            ..Default::default()
        };
        view.reframe(size, extent, 1.);
        view.pan_to(egui::vec2(0.35, 0.7));
        let center_before = center(&view, size, extent);
        view.detach_layout();
        assert!(view.current_image_size().is_none());
        assert!(view.visible_pixels().is_none());
        for (factor, position) in [
            (1.5, egui::vec2(0.5, 0.5)),
            (2., egui::vec2(0.2, 0.75)),
            (0.5, egui::vec2(0.8, 0.25)),
        ] {
            let mut before = Viewport::default();
            before.set_transform(view.transform());
            before.reframe(size, extent, 1.);
            let image = before.image_rect(canvas, extent);
            let visible = canvas.intersect(image);
            let point = visible.min + position * visible.size();
            let expected = (point - image.min) / image.size();
            let scale = view.zoom_percent(1.).unwrap();
            view.apply_view_change(
                ViewChange::Zoom {
                    factor,
                    anchor: position,
                },
                1.,
            );
            view.detach_layout();
            // Restoring the saved transform only for observation preserves hidden ownership.
            let mut observed = Viewport::default();
            observed.set_transform(view.transform());
            observed.reframe(size, extent, 1.);
            let image = observed.image_rect(canvas, extent);
            near((point - image.min) / image.size(), expected);
            assert!((observed.zoom_percent(1.).unwrap() / scale - factor).abs() < 1e-5);
            if position == egui::vec2(0.5, 0.5) {
                near(center(&observed, size, extent), center_before);
            }
            assert!(view.current_image_size().is_none());
            assert!(view.visible_pixels().is_none());
        }
    }
}

#[test]
fn never_drawn_center_zoom_keeps_center_and_relative_fit_multiplier() {
    let mut view = Viewport::default();
    for factor in [2., 1.5, 0.5] {
        view.apply_view_change(
            ViewChange::Zoom {
                factor,
                anchor: egui::vec2(0.5, 0.5),
            },
            1.,
        );
        view.detach_layout();
    }
    let size = egui::vec2(300., 480.);
    let extent = [1200, 1800];
    view.reframe(size, extent, 1.);
    let fit = 300. / 1200.;
    assert_eq!(view.zoom_percent(1.), Some(fit * 1.5 * 100.));
    near(center(&view, size, extent), egui::vec2(0.5, 0.5));
}

#[test]
fn hidden_fit_zoom_retains_last_displayed_geometry_through_repeated_anchors_and_fit() {
    let size = egui::vec2(800., 600.);
    let extent = [1600, 1200];
    let mut displayed = Viewport::default();
    let mut hidden = Viewport::default();
    displayed.reframe(size, extent, 1.);
    hidden.reframe(size, extent, 1.);
    hidden.detach_layout();
    for change in [
        ViewChange::Zoom {
            factor: 2.,
            anchor: egui::vec2(0.25, 0.5),
        },
        ViewChange::Zoom {
            factor: 2.,
            anchor: egui::vec2(0.75, 0.5),
        },
        ViewChange::Fit,
        ViewChange::Zoom {
            factor: 2.,
            anchor: egui::vec2(0.25, 0.5),
        },
        ViewChange::Zoom {
            factor: 2.,
            anchor: egui::vec2(0.75, 0.5),
        },
    ] {
        displayed.apply_view_change(change, 1.);
        hidden.apply_view_change(change, 1.);
        hidden.detach_layout();
        let mut observed = Viewport::default();
        observed.set_transform(hidden.transform());
        observed.reframe(size, extent, 1.);
        assert_eq!(observed.zoom_percent(1.), displayed.zoom_percent(1.));
        near(
            center(&observed, size, extent),
            center(&displayed, size, extent),
        );
        assert!(hidden.current_image_size().is_none());
    }
    near(center(&displayed, size, extent), egui::vec2(0.4375, 0.5));
}

#[test]
fn hidden_pan_keeps_screen_distance_with_known_or_not_yet_drawn_geometry() {
    let size = egui::vec2(420., 280.);
    let extent = [900, 1600];
    let delta = egui::vec2(24., 36.);
    for initial_scale in [None, Some(1.5)] {
        let mut view = Viewport {
            scale: initial_scale,
            ..Default::default()
        };
        view.reframe(size, extent, 1.);
        if initial_scale.is_some() {
            view.pan_to(egui::vec2(0.35, 0.65));
        }
        let before = view.pan;
        let zoom = view.zoom_percent(1.);
        view.detach_layout();
        for movement in [egui::vec2(41., 25.), egui::vec2(-17., 11.)] {
            view.apply_view_change(ViewChange::Pan(movement), 1.);
            view.detach_layout();
        }
        assert!(view.current_image_size().is_none());
        let mut observed = Viewport::default();
        observed.set_transform(view.transform());
        observed.reframe(size, extent, 1.);
        assert!((observed.pan - before - delta).length() < 1e-3);
        assert_eq!(observed.zoom_percent(1.), zoom);
    }
    let mut unknown = Viewport::default();
    for movement in [egui::vec2(41., 25.), egui::vec2(-17., 11.)] {
        unknown.apply_view_change(ViewChange::Pan(movement), 1.);
        unknown.detach_layout();
    }
    unknown.reframe(egui::Vec2::ZERO, extent, 1.);
    let mut carried = Viewport::default();
    carried.set_transform(unknown.transform());
    carried.reframe(size, extent, 1.);
    assert!((carried.pan - delta).length() < 1e-3);
    assert_eq!(carried.zoom_percent(1.), Some(280. / 1600. * 100.));
    carried.reframe(size, extent, 1.);
    assert!(
        (carried.pan - delta).length() < 1e-3,
        "first-layout pan is consumed once"
    );
}

#[test]
fn pan_before_a_carried_fixed_zoom_is_drawn_preserves_physical_scale() {
    let size = egui::vec2(420., 280.);
    let extent = [900, 1600];
    let mut source = Viewport {
        scale: Some(1.5),
        ..Default::default()
    };
    source.reframe(size, extent, 1.);
    source.pan_to(egui::vec2(0.35, 0.65));
    let original_center = source.transform().center.unwrap();
    let mut carried = Viewport::default();
    carried.set_transform(source.transform());
    carried.apply_view_change(ViewChange::Pan(egui::vec2(24., 36.)), 1.);
    carried.reframe(egui::Vec2::ZERO, extent, 1.);
    carried.detach_layout();
    let mut restored = Viewport::default();
    restored.set_transform(carried.transform());
    restored.reframe(size, extent, 1.);
    assert_eq!(restored.zoom_percent(1.), Some(150.));
    let expected = (egui::vec2(0.5, 0.5) - original_center) * egui::vec2(900., 1600.) * 1.5
        + egui::vec2(24., 36.);
    assert!((restored.pan - expected).length() < 1e-3);
    restored.reframe(size, extent, 1.);
    assert!((restored.pan - expected).length() < 1e-3);
}
