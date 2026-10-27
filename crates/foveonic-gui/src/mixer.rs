//! Edits three-channel proportions with a triangular barycentric monochrome mixer.

use eframe::egui::{self, Color32, Pos2, Vec2};

fn vertices(rect: egui::Rect) -> [Pos2; 3] {
    [
        egui::pos2(rect.center().x, rect.top() + 15.),
        egui::pos2(rect.left() + 22., rect.bottom() - 20.),
        egui::pos2(rect.right() - 22., rect.bottom() - 20.),
    ]
}
fn position(corners: [Pos2; 3], mix: [f32; 3]) -> Pos2 {
    (corners[0].to_vec2() * mix[0] + corners[1].to_vec2() * mix[1] + corners[2].to_vec2() * mix[2])
        .to_pos2()
}
fn barycentric(corners: [Pos2; 3], point: Pos2) -> [f32; 3] {
    let [a, b, c] = corners;
    let r = (b.y - c.y).mul_add(point.x - c.x, (c.x - b.x) * (point.y - c.y))
        / (b.y - c.y).mul_add(a.x - c.x, (c.x - b.x) * (a.y - c.y));
    let g = (c.y - a.y).mul_add(point.x - c.x, (a.x - c.x) * (point.y - c.y))
        / (b.y - c.y).mul_add(a.x - c.x, (c.x - b.x) * (a.y - c.y));
    [r, g, 1. - r - g]
}
fn mix_at(corners: [Pos2; 3], point: Pos2) -> [f32; 3] {
    let inside = barycentric(corners, point);
    if inside.iter().all(|v| *v >= 0.) {
        return inside;
    }
    // Project to the nearest triangle edge, not to a separately clamped rectangular control.
    let mut closest = corners[0];
    let mut distance = f32::INFINITY;
    for i in 0..3 {
        let start = corners[i];
        let edge = corners[(i + 1) % 3] - start;
        let projected = start + edge * ((point - start).dot(edge) / edge.length_sq()).clamp(0., 1.);
        if point.distance_sq(projected) < distance {
            closest = projected;
            distance = point.distance_sq(projected);
        }
    }
    let mix = barycentric(corners, closest).map(|v| v.max(0.));
    let sum = mix.iter().sum::<f32>();
    mix.map(|v| v / sum)
}

pub fn show(ui: &mut egui::Ui, mix: &mut [f32; 3]) -> bool {
    show_with_help(ui, mix, [1. / 3.; 3], "Color Mix")
}

pub fn show_with_help(
    ui: &mut egui::Ui,
    mix: &mut [f32; 3],
    default: [f32; 3],
    help: &str,
) -> bool {
    let before = *mix;
    ui.horizontal(|ui| {
        crate::help::control(ui.label("Color Mix"), help);
        if crate::help::control(
            crate::widgets::button(ui, "Reset", false, egui::vec2(52., 20.)),
            help,
        )
        .clicked()
        {
            *mix = default;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            crate::sections::editing_handle(ui, crate::preferences::EditingSection::Mix);
        });
    });
    let width = ui.available_width().min(280.);
    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::new(width, 155.), egui::Sense::click_and_drag());
    let corners = vertices(rect);
    if response.dragged() || response.clicked() {
        if let Some(point) = response.interact_pointer_pos() {
            *mix = mix_at(corners, point);
            for (i, corner) in corners.iter().enumerate() {
                if corner.distance_sq(point) <= 9. * 9. {
                    *mix = [0.; 3];
                    mix[i] = 1.;
                    break;
                }
            }
            response.request_focus();
        }
    }
    if response.double_clicked() {
        *mix = default;
    }
    if response.has_focus() {
        let step = ui.input(|i| {
            Vec2::new(
                f32::from(i.key_pressed(egui::Key::ArrowRight))
                    - f32::from(i.key_pressed(egui::Key::ArrowLeft)),
                f32::from(i.key_pressed(egui::Key::ArrowDown))
                    - f32::from(i.key_pressed(egui::Key::ArrowUp)),
            )
        }) * 2.;
        if step != Vec2::ZERO {
            *mix = mix_at(corners, position(corners, *mix) + step);
        }
    }
    let colors = [
        Color32::from_rgb(148, 64, 64),
        Color32::from_rgb(57, 122, 68),
        Color32::from_rgb(64, 91, 155),
    ];
    let mut mesh = egui::Mesh::default();
    for (point, color) in corners.into_iter().zip(colors) {
        mesh.colored_vertex(point, color);
    }
    mesh.add_triangle(0, 1, 2);
    ui.painter().add(mesh);
    ui.painter().add(egui::Shape::closed_line(
        corners.to_vec(),
        egui::Stroke::new(1_f32, ui.visuals().widgets.noninteractive.fg_stroke.color),
    ));
    for (i, label) in ["R", "G", "B"].into_iter().enumerate() {
        let offset = if i == 0 {
            Vec2::new(0., -10.)
        } else {
            Vec2::new(if i == 1 { -12. } else { 12. }, 0.)
        };
        ui.painter().text(
            corners[i] + offset,
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.),
            ui.visuals().text_color(),
        );
    }
    let selected = position(corners, *mix);
    ui.painter().circle_filled(selected, 5., Color32::WHITE);
    ui.painter()
        .circle_stroke(selected, 5., egui::Stroke::new(1.5_f32, Color32::BLACK));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Slider, ui.is_enabled(), "Color Mix")
    });
    if before != *mix {
        response.mark_changed();
    }
    crate::help::control(response, "Color Mix");
    ui.horizontal(|ui| {
        for (name, amount) in ["R", "G", "B"].into_iter().zip(*mix) {
            ui.label(format!("{name} {:.1}%", amount * 100.));
        }
    });
    before != *mix
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corners_center_edges_and_outside_are_valid_mixtures() {
        let corners = vertices(egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(260., 155.)));
        for i in 0..3 {
            let mut expected = [0.; 3];
            expected[i] = 1.;
            assert_eq!(mix_at(corners, corners[i]), expected);
        }
        for mix in [
            [1. / 3.; 3],
            [0.5, 0.5, 0.],
            [0., 0.25, 0.75],
            [0.8, 0.1, 0.1],
        ] {
            let actual = mix_at(corners, position(corners, mix));
            for i in 0..3 {
                assert!((actual[i] - mix[i]).abs() < 0.00001);
            }
        }
        for point in [
            Pos2::new(-500., -500.),
            Pos2::new(800., 500.),
            Pos2::new(130., 800.),
        ] {
            let mix = mix_at(corners, point);
            assert!(mix.iter().all(|v| *v >= 0.));
            assert!((mix.iter().sum::<f32>() - 1.).abs() < 0.00001);
        }
    }
}
