//! Draws and edits hue and saturation bounds with continuous circular handle gestures.

use eframe::egui;
use foveonic::render::AdvancedColorRange;
const TAU: f32 = std::f32::consts::TAU;
#[derive(Clone, Copy)]
enum Handle {
    LowHue,
    HighHue,
    LowSaturation,
    HighSaturation,
    Move,
}
#[derive(Clone, Copy)]
struct Drag {
    handle: Handle,
    original: AdvancedColorRange,
    angle: f32,
}
fn point(center: egui::Pos2, radius: f32, hue: f32, saturation: f32) -> egui::Pos2 {
    let a = hue.to_radians();
    center + egui::vec2(a.cos(), -a.sin()) * (radius * saturation)
}
fn delta(a: f32, b: f32) -> f32 {
    (a - b + 540.).rem_euclid(360.) - 180.
}
fn color(h: f32, s: f32, alpha: f32) -> egui::Color32 {
    egui::ecolor::Hsva::new(h / 360., s, 0.85, alpha).into()
}
fn weight(h: f32, s: f32, r: AdvancedColorRange) -> f32 {
    let smooth = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a)).clamp(0., 1.);
        t * t * (3. - 2. * t)
    };
    let hue = if r.width >= 360. {
        1.
    } else {
        1. - smooth(
            r.width * 0.5,
            r.width * 0.5 + (r.smoothness * 60.).max(0.001),
            delta(h, r.hue).abs(),
        )
    };
    let feather = (r.smoothness * 0.5).max(0.00001);
    hue * smooth(r.saturation[0] - feather, r.saturation[0], s)
        * (1. - smooth(r.saturation[1], r.saturation[1] + feather, s))
}
pub(super) fn show(ui: &mut egui::Ui, range: &mut AdvancedColorRange) -> bool {
    wheel(ui, Some(range))
}
pub(super) fn empty(ui: &mut egui::Ui) {
    wheel(ui, None);
}
fn wheel(ui: &mut egui::Ui, range: Option<&mut AdvancedColorRange>) -> bool {
    let available = ui.available_width();
    let side = available.clamp(100., 300.);
    ui.horizontal(|ui| {
        ui.add_space(((available - side) * 0.5).max(0.));
        draw_wheel(ui, range, side)
    })
    .inner
}

fn draw_wheel(ui: &mut egui::Ui, mut range: Option<&mut AdvancedColorRange>, side: f32) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click_and_drag());
    let center = rect.center();
    let radius = side * 0.46;
    let painter = ui.painter();
    let mut mesh = egui::Mesh::default();
    for h in 0..96 {
        for s in 0..12 {
            let hue = h as f32 * 360. / 96.;
            let sat = s as f32 / 12.;
            let alpha = range.as_deref().map_or(0.12, |r| {
                0.08 + 0.85 * weight(hue + 1.875, sat + 1. / 24., *r)
            });
            let first = mesh.vertices.len() as u32;
            for (a, r) in [
                (hue, sat),
                (hue + 3.75, sat),
                (hue + 3.75, sat + 1. / 12.),
                (hue, sat + 1. / 12.),
            ] {
                mesh.colored_vertex(point(center, radius, a, r), color(a % 360., r, alpha));
            }
            mesh.add_triangle(first, first + 1, first + 2);
            mesh.add_triangle(first, first + 2, first + 3);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    for h in 0..96 {
        let a = h as f32 * TAU / 96.;
        let b = (h + 1) as f32 * TAU / 96.;
        painter.line_segment(
            [
                center + egui::vec2(a.cos(), -a.sin()) * radius,
                center + egui::vec2(b.cos(), -b.sin()) * radius,
            ],
            egui::Stroke::new(2.5_f32, color(h as f32 * 360. / 96., 1., 1.)),
        );
    }
    let Some(range) = range.as_mut() else {
        response.on_hover_text("Pick a color or add a range to edit its selection.");
        return false;
    };
    let lo = range.hue - range.width * 0.5;
    let hi = range.hue + range.width * 0.5;
    let handles = [
        (
            Handle::LowHue,
            point(center, radius, lo, range.saturation[1]),
        ),
        (
            Handle::HighHue,
            point(center, radius, hi, range.saturation[1]),
        ),
        (
            Handle::LowSaturation,
            point(center, radius, range.hue, range.saturation[0]),
        ),
        (
            Handle::HighSaturation,
            point(center, radius, range.hue, range.saturation[1]),
        ),
    ];
    for angle in [lo, hi] {
        painter.line_segment(
            [
                point(center, radius, angle, range.saturation[0]),
                point(center, radius, angle, range.saturation[1]),
            ],
            (1.5, egui::Color32::WHITE),
        );
    }
    for s in range.saturation {
        let points = (0..=64)
            .map(|i| point(center, radius, lo + range.width * i as f32 / 64., s))
            .collect();
        painter.add(egui::Shape::line(points, (1.5, egui::Color32::WHITE)));
    }
    for (_, p) in handles {
        painter.circle_filled(p, 4., egui::Color32::WHITE);
        painter.circle_stroke(p, 4., (1., egui::Color32::BLACK));
    }
    let mut changed = false;
    if response.drag_started() {
        if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
            let handle = handles
                .into_iter()
                .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)))
                .filter(|(_, p)| p.distance(pos) < 14.)
                .map_or(Handle::Move, |(h, _)| h);
            let d = pos - center;
            ui.data_mut(|data| {
                data.insert_temp(
                    response.id,
                    Drag {
                        handle,
                        original: **range,
                        angle: (-d.y).atan2(d.x).to_degrees().rem_euclid(360.),
                    },
                )
            });
        }
    }
    if response.dragged() {
        if let (Some(pos), Some(drag)) = (
            response.interact_pointer_pos(),
            ui.data(|d| d.get_temp::<Drag>(response.id)),
        ) {
            let d = pos - center;
            let angle = (-d.y).atan2(d.x).to_degrees().rem_euclid(360.);
            let saturation = (d.length() / radius).clamp(0., 1.);
            let o = drag.original;
            let mut new = o;
            match drag.handle {
                Handle::Move => new.hue = (o.hue + delta(angle, drag.angle)).rem_euclid(360.),
                Handle::LowHue => {
                    let high = o.hue + o.width * 0.5;
                    let low = (o.hue - o.width * 0.5 + delta(angle, drag.angle))
                        .clamp(high - 360., high - 2.);
                    new.hue = ((high + low) * 0.5).rem_euclid(360.);
                    new.width = high - low;
                }
                Handle::HighHue => {
                    let low = o.hue - o.width * 0.5;
                    let high = (o.hue + o.width * 0.5 + delta(angle, drag.angle))
                        .clamp(low + 2., low + 360.);
                    new.hue = ((high + low) * 0.5).rem_euclid(360.);
                    new.width = high - low;
                }
                Handle::LowSaturation => new.saturation[0] = saturation.min(o.saturation[1]),
                Handle::HighSaturation => new.saturation[1] = saturation.max(o.saturation[0]),
            }
            changed = **range != new;
            **range = new;
        }
    }
    response.on_hover_text("Drag the sides to select hues, the inner and outer handles to select saturation, or the wedge to rotate it.");
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dragging_a_hue_boundary_through_red_preserves_the_opposite_boundary() {
        let ctx = egui::Context::default();
        let mut range = AdvancedColorRange {
            hue: 355.,
            width: 40.,
            saturation: [0.2, 0.8],
            ..Default::default()
        };
        let center = egui::pos2(200., 158.);
        let radius = 138.;
        let anchor = point(center, radius, 15., 0.8);
        let end = point(center, radius, 40., 0.8);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for (time, events) in [
            (0., vec![]),
            (
                0.1,
                vec![egui::Event::PointerMoved(anchor), button(anchor, true)],
            ),
            (0.2, vec![egui::Event::PointerMoved(end)]),
            (0.3, vec![button(end, false)]),
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 400.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        show(ui, &mut range);
                    });
                },
            );
        }
        assert!((range.hue - 7.5).abs() < 0.01, "{range:?}");
        assert!((range.width - 65.).abs() < 0.01, "{range:?}");
        assert_eq!(range.saturation, [0.2, 0.8]);
    }
}
