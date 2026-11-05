//! Resizes and moves crop rectangles in native coordinates and paints their viewport overlay.

use eframe::egui::{self, Pos2, Rect, Vec2};
use foveonic::crop::Crop;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Handle {
    Corner([f32; 2]),
    Edge(usize, f32),
    Move,
    New,
}

fn bounds(extent: [u32; 2]) -> Rect {
    Rect::from_min_size(Pos2::ZERO, egui::vec2(extent[0] as f32, extent[1] as f32))
}

pub(super) fn rectangle(crop: Crop, extent: [u32; 2]) -> Rect {
    let [l, t, r, b] = crop.pixels(extent);
    Rect::from_min_max(
        egui::pos2(l as f32, t as f32),
        egui::pos2(r as f32, b as f32),
    )
}

fn save(mut crop: Crop, rect: Rect, extent: [u32; 2]) -> Crop {
    let full = bounds(extent);
    let size = rect.size().max(Vec2::splat(1.)).min(full.size());
    let min = rect.min.to_vec2().max(Vec2::ZERO).min(full.size() - size);
    let start = min.round();
    let end = (min + size)
        .round()
        .max(start + Vec2::splat(1.))
        .min(full.size());
    crop.bounds = [
        f64::from(start.x) / f64::from(extent[0]),
        f64::from(start.y) / f64::from(extent[1]),
        f64::from(end.x) / f64::from(extent[0]),
        f64::from(end.y) / f64::from(extent[1]),
    ];
    crop
}

fn ratio(crop: Crop, extent: [u32; 2]) -> Option<f32> {
    // The native grid cannot represent a rectangle smaller than one pixel.
    crop.ratio(extent)
        .map(|r| r.clamp(1. / f64::from(extent[1]), f64::from(extent[0])) as f32)
}

pub(super) fn fit(crop: Crop, extent: [u32; 2]) -> Crop {
    let rect = rectangle(crop, extent);
    let Some(ratio) = ratio(crop, extent) else {
        return crop;
    };
    let area = rect.area();
    let mut size = egui::vec2((area * ratio).sqrt(), (area / ratio).sqrt());
    let full = bounds(extent);
    size *= (full.width() / size.x).min(full.height() / size.y).min(1.);
    save(crop, Rect::from_center_size(rect.center(), size), extent)
}

pub(super) fn hit(rect: Rect, position: Pos2) -> Handle {
    let points = [
        ([-1., -1.], rect.left_top()),
        ([1., -1.], rect.right_top()),
        ([-1., 1.], rect.left_bottom()),
        ([1., 1.], rect.right_bottom()),
    ];
    for (sign, p) in points {
        if p.distance(position) <= 8. {
            return Handle::Corner(sign);
        }
    }
    for (axis, sign, p) in [
        (0, -1., rect.left_center()),
        (0, 1., rect.right_center()),
        (1, -1., rect.center_top()),
        (1, 1., rect.center_bottom()),
    ] {
        if p.distance(position) <= 7. {
            return Handle::Edge(axis, sign);
        }
    }
    if rect.contains(position) {
        Handle::Move
    } else {
        Handle::New
    }
}

pub(super) fn cursor(handle: Handle, dragging: bool) -> egui::CursorIcon {
    match handle {
        Handle::Corner([x, y]) if x == y => egui::CursorIcon::ResizeNwSe,
        Handle::Corner(_) => egui::CursorIcon::ResizeNeSw,
        Handle::Edge(0, _) => egui::CursorIcon::ResizeHorizontal,
        Handle::Edge(_, _) => egui::CursorIcon::ResizeVertical,
        Handle::Move if dragging => egui::CursorIcon::Grabbing,
        Handle::Move => egui::CursorIcon::Grab,
        Handle::New => egui::CursorIcon::Crosshair,
    }
}

#[derive(Clone)]
pub(super) struct Drag {
    pub original: Crop,
    pub handle: Handle,
    pub draft: Crop,
    pub moved: bool,
    start: Pos2,
    rect: Rect,
    extent: [u32; 2],
}
impl Drag {
    pub fn new(crop: Crop, extent: [u32; 2], handle: Handle, start: Pos2) -> Self {
        Self {
            original: crop,
            draft: crop,
            handle,
            moved: false,
            start,
            rect: rectangle(crop, extent),
            extent,
        }
    }

    pub fn update(&mut self, position: Pos2) {
        self.moved |= position.distance(self.start) > 0.5;
        if !self.moved {
            return;
        }
        let full = bounds(self.extent);
        let position = position.clamp(full.min, full.max);
        let ratio = ratio(self.original, self.extent);
        let rect = match self.handle {
            Handle::Move => self.rect.translate(position - self.start),
            Handle::Corner(sign) => {
                let anchor = egui::pos2(
                    if sign[0] < 0. {
                        self.rect.right()
                    } else {
                        self.rect.left()
                    },
                    if sign[1] < 0. {
                        self.rect.bottom()
                    } else {
                        self.rect.top()
                    },
                );
                corner(anchor, position, sign, full, ratio)
            }
            Handle::New => corner(self.start, position, [1., 1.], full, ratio),
            Handle::Edge(axis, sign) => {
                let other = 1 - axis;
                let mut min = self.rect.min;
                let mut max = self.rect.max;
                if sign < 0. {
                    min[axis] = position[axis].min(max[axis] - 1.)
                } else {
                    max[axis] = position[axis].max(min[axis] + 1.)
                }
                if let Some(r) = ratio {
                    let length = max[axis] - min[axis];
                    let cross = if axis == 0 { length / r } else { length * r };
                    let center = self.rect.center()[other];
                    min[other] = center - cross * 0.5;
                    max[other] = center + cross * 0.5;
                    let size = max - min;
                    let scale = (full.width() / size.x).min(full.height() / size.y).min(1.);
                    let size = size * scale;
                    return self.draft = save(
                        self.original,
                        Rect::from_center_size(Rect::from_min_max(min, max).center(), size),
                        self.extent,
                    );
                }
                Rect::from_min_max(min, max)
            }
        };
        self.draft = save(self.original, rect, self.extent);
    }
}

fn corner(
    anchor: Pos2,
    position: Pos2,
    fallback: [f32; 2],
    full: Rect,
    ratio: Option<f32>,
) -> Rect {
    let delta = position - anchor;
    let sign: [f32; 2] = std::array::from_fn(|i| {
        if delta[i] < 0. {
            -1.
        } else if delta[i] > 0. {
            1.
        } else if (fallback[i] > 0. && anchor[i] >= full.max[i])
            || (fallback[i] < 0. && anchor[i] <= 0.)
        {
            -fallback[i]
        } else {
            fallback[i]
        }
    });
    let room = egui::vec2(
        if sign[0] < 0. {
            anchor.x
        } else {
            full.right() - anchor.x
        },
        if sign[1] < 0. {
            anchor.y
        } else {
            full.bottom() - anchor.y
        },
    );
    let mut size = delta.abs().max(Vec2::splat(1.));
    if let Some(r) = ratio {
        let direction = egui::vec2(r, 1.).normalized();
        size = direction * size.dot(direction);
        size *= (room.x / size.x).min(room.y / size.y).min(1.);
    } else {
        size = size.min(room)
    }
    let end = anchor + size * egui::vec2(sign[0], sign[1]);
    Rect::from_two_pos(anchor, end)
}

pub(super) fn screen_rect(crop: Crop, extent: [u32; 2], image: Rect) -> Rect {
    let rect = rectangle(crop, extent);
    let factor = image.size() / bounds(extent).size();
    Rect::from_min_max(
        image.min + rect.min.to_vec2() * factor,
        image.min + rect.max.to_vec2() * factor,
    )
}

pub(super) fn paint(
    ui: &egui::Ui,
    canvas: Rect,
    image: Rect,
    crop: Crop,
    extent: [u32; 2],
    dragging: bool,
) {
    let painter = ui
        .painter()
        .with_clip_rect(canvas.intersect(ui.clip_rect()));
    let rect = screen_rect(crop, extent, image);
    let shade = egui::Color32::from_black_alpha(145);
    for outside in [
        Rect::from_min_max(image.min, egui::pos2(image.right(), rect.top())),
        Rect::from_min_max(egui::pos2(image.left(), rect.bottom()), image.max),
        Rect::from_min_max(egui::pos2(image.left(), rect.top()), rect.left_bottom()),
        Rect::from_min_max(rect.right_top(), egui::pos2(image.right(), rect.bottom())),
    ] {
        if outside.is_positive() {
            painter.rect_filled(outside, 0., shade);
        }
    }
    let stroke = egui::Stroke::new(
        if dragging { 1_f32 } else { 2_f32 },
        egui::Color32::from_white_alpha(205),
    );
    if dragging {
        painter.rect_stroke(rect, 0., stroke, egui::StrokeKind::Inside);
        for f in [1. / 3., 2. / 3.] {
            let x = rect.left() + f * rect.width();
            let y = rect.top() + f * rect.height();
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                stroke,
            );
        }
    } else {
        let length = 12_f32.min(rect.width() * 0.2).min(rect.height() * 0.2);
        for (p, s) in [
            (rect.left_top(), [1., 1.]),
            (rect.right_top(), [-1., 1.]),
            (rect.left_bottom(), [1., -1.]),
            (rect.right_bottom(), [-1., -1.]),
        ] {
            painter.line_segment([p, p + egui::vec2(s[0] * length, 0.)], stroke);
            painter.line_segment([p, p + egui::vec2(0., s[1] * length)], stroke);
        }
        for p in [rect.left_center(), rect.right_center()] {
            painter.line_segment([p - egui::vec2(0., 5.), p + egui::vec2(0., 5.)], stroke);
        }
        for p in [rect.center_top(), rect.center_bottom()] {
            painter.line_segment([p - egui::vec2(5., 0.), p + egui::vec2(5., 0.)], stroke);
        }
    }
    let [l, t, r, b] = crop.pixels(extent);
    for (text, position) in [
        (
            format!("{} px", r - l),
            rect.center_top() + egui::vec2(0., -11.),
        ),
        (
            format!("{} px", b - t),
            rect.left_center() + egui::vec2(0., 0.),
        ),
    ] {
        let font = egui::TextStyle::Small.resolve(ui.style());
        let text = painter.layout_no_wrap(text, font, ui.visuals().selection.stroke.color);
        let size = text.size() + egui::vec2(8., 4.);
        let mut label = Rect::from_center_size(position, size);
        let minimum = canvas.min + Vec2::splat(2.);
        let maximum = (canvas.max - size - Vec2::splat(2.)).max(minimum);
        label = Rect::from_min_size(label.min.max(minimum).min(maximum), size);
        painter.rect_filled(label, 3., egui::Color32::from_black_alpha(220));
        painter.galley(
            label.center() - text.size() * 0.5,
            text,
            egui::Color32::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_and_resizing_stay_inside_the_native_image_and_preserve_the_constraint() {
        let crop = Crop {
            bounds: [0.2, 0.2, 0.8, 0.8],
            ..Default::default()
        };
        let extent = [600, 400];
        let mut drag = Drag::new(crop, extent, Handle::Move, egui::pos2(300., 200.));
        drag.update(egui::pos2(900., 900.));
        assert_eq!(drag.draft.pixels(extent), [240, 160, 600, 400]);
        for handle in [
            Handle::Corner([-1., -1.]),
            Handle::Corner([1., 1.]),
            Handle::Edge(0, -1.),
            Handle::Edge(1, 1.),
        ] {
            let mut drag = Drag::new(crop, extent, handle, egui::pos2(200., 200.));
            drag.update(egui::pos2(590., 390.));
            drag.draft.validate().unwrap();
            let [l, t, r, b] = drag.draft.pixels(extent);
            assert!(r <= 600 && b <= 400 && l < r && t < b);
            assert!(
                (f64::from(r - l) - 1.5 * f64::from(b - t)).abs() <= 1.5,
                "{handle:?}: {:?}",
                drag.draft.pixels(extent)
            );
        }
    }
    #[test]
    fn new_rectangles_can_grow_in_either_direction_and_repeated_updates_do_not_accumulate() {
        let crop = Crop {
            aspect: foveonic::crop::Aspect::Unconstrained,
            ..Default::default()
        };
        let mut drag = Drag::new(crop, [600, 400], Handle::New, egui::pos2(300., 200.));
        drag.update(egui::pos2(100., 50.));
        assert_eq!(drag.draft.pixels([600, 400]), [100, 50, 300, 200]);
        drag.update(egui::pos2(500., 350.));
        assert_eq!(drag.draft.pixels([600, 400]), [300, 200, 500, 350]);
    }
}
