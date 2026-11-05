//! Shares the rotated editing plane, crop frame, exterior shading and dimensions across framing tools.

use eframe::egui::{self, Rect, Vec2};
use foveonic::{crop::Crop, rotation::Rotation};

#[derive(Clone, Copy)]
pub struct Presentation {
    pub rotation: Rotation,
    pub native_extent: [u32; 2],
}
impl Presentation {
    pub fn extent(self) -> [u32; 2] {
        self.rotation.oriented_extent(self.native_extent)
    }
    pub fn angle(self) -> f32 {
        (f64::from(self.rotation.turns) * 90. + self.rotation.degrees) as f32
    }
}

/// Both controls edit one plane and differ only in their pointer gestures and guides.
pub fn photo(
    ui: &mut egui::Ui,
    response: &egui::Response,
    canvas: Rect,
    image: Rect,
    native: [u32; 2],
    crop: &mut Crop,
    rotation: &mut Rotation,
) -> bool {
    let selected = crate::photo_tools::selected(ui.ctx());
    let shown = crate::rotate_tool::displayed(ui.ctx(), *rotation);
    let changed = match selected {
        crate::photo_tools::Tool::Crop => crate::crop_tool::photo(
            ui,
            response,
            interior(image, native, shown),
            shown.extent(native),
            crop,
        ),
        crate::photo_tools::Tool::Rotate => {
            crate::rotate_tool::photo(ui, response, image, rotation)
        }
        _ => return false,
    };
    let shown = crate::rotate_tool::displayed(ui.ctx(), *rotation);
    let (crop, dragging) = crate::crop_tool::displayed(ui.ctx(), *crop);
    paint(
        ui,
        canvas,
        interior(image, native, shown),
        crop,
        shown.extent(native),
        dragging,
    );
    if selected == crate::photo_tools::Tool::Rotate {
        crate::rotate_tool::paint_guides(ui, canvas, *rotation);
    }
    changed
}

/// The available interior image inside the untrimmed, rotated editing canvas.
pub fn interior(image: Rect, native: [u32; 2], rotation: Rotation) -> Rect {
    let [l, t, r, b] = rotation
        .framing_bounds(native, Crop::default())
        .map(|v| v as f32);
    Rect::from_min_max(
        image.min + image.size() * egui::vec2(l, t),
        image.min + image.size() * egui::vec2(r, b),
    )
}

pub fn crop_rect(crop: Crop, extent: [u32; 2], image: Rect) -> Rect {
    let [l, t, r, b] = crop.pixels(extent);
    let factor = image.size() / egui::vec2(extent[0] as f32, extent[1] as f32);
    Rect::from_min_max(
        image.min + egui::vec2(l as f32, t as f32) * factor,
        image.min + egui::vec2(r as f32, b as f32) * factor,
    )
}

fn paint(ui: &egui::Ui, canvas: Rect, image: Rect, crop: Crop, extent: [u32; 2], dragging: bool) {
    let painter = ui
        .painter()
        .with_clip_rect(canvas.intersect(ui.clip_rect()));
    let rect = crop_rect(crop, extent, image);
    let shade = egui::Color32::from_black_alpha(145);
    for outside in [
        Rect::from_min_max(canvas.min, egui::pos2(canvas.right(), rect.top())),
        Rect::from_min_max(egui::pos2(canvas.left(), rect.bottom()), canvas.max),
        Rect::from_min_max(egui::pos2(canvas.left(), rect.top()), rect.left_bottom()),
        Rect::from_min_max(rect.right_top(), egui::pos2(canvas.right(), rect.bottom())),
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
