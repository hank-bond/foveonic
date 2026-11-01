//! Draws a CIE 1931 xy guide with fixed visual anchors for the layer mixer.

use eframe::egui::{self, Color32, Pos2};
use std::sync::OnceLock;

// CIE 2019, DOI 10.25039/CIE.DS.mifmy4x4, CC BY-SA 4.0.
// The unchanged table and attribution are retained beside the other GUI assets.
fn locus() -> &'static [[f32; 2]] {
    static POINTS: OnceLock<Vec<[f32; 2]>> = OnceLock::new();
    POINTS.get_or_init(|| {
        include_str!("../assets/cie1931-locus.csv")
            .lines()
            .filter_map(|line| {
                let mut columns = line.split(',');
                let wavelength: u32 = columns.next()?.parse().ok()?;
                let x = columns.next()?.parse().ok()?;
                let y = columns.next()?.parse().ok()?;
                (wavelength.is_multiple_of(5)).then_some([x, y])
            })
            .collect()
    })
}

// Representative spectral red, green and blue points used only as visual anchors.
// They do not describe measured sensor primaries or the display's gamut.
const ANCHOR_WAVELENGTHS: [usize; 3] = [700, 520, 460];
const FIRST_WAVELENGTH: usize = 360;
const WAVELENGTH_STEP: usize = 5;

fn anchors() -> [[f32; 2]; 3] {
    ANCHOR_WAVELENGTHS.map(|wavelength| locus()[(wavelength - FIRST_WAVELENGTH) / WAVELENGTH_STEP])
}

const X_MAX: f32 = 0.80;
const Y_MAX: f32 = 0.90;
const MARGIN: f32 = 18.;

fn plot_rect(rect: egui::Rect) -> egui::Rect {
    let available = rect.shrink(MARGIN);
    let scale = (available.width() / X_MAX).min(available.height() / Y_MAX);
    egui::Rect::from_center_size(available.center(), egui::vec2(scale * X_MAX, scale * Y_MAX))
}

fn point(rect: egui::Rect, xy: [f32; 2]) -> Pos2 {
    egui::pos2(
        rect.left() + rect.width() * xy[0] / X_MAX,
        rect.bottom() - rect.height() * xy[1] / Y_MAX,
    )
}

pub fn corners(rect: egui::Rect) -> [Pos2; 3] {
    anchors().map(|xy| point(plot_rect(rect), xy))
}

// Decorative sRGB preview only. Normalize brightness after removing negative RGB.
// The fixed anchors and background geometry remain in xy.
fn color(xy: [f32; 2]) -> Color32 {
    let [x, y] = xy;
    let z = 1. - x - y;
    let rgb = [
        3.2406 * x - 1.5372 * y - 0.4986 * z,
        -0.9689 * x + 1.8758 * y + 0.0415 * z,
        0.0557 * x - 0.2040 * y + 1.0570 * z,
    ]
    .map(|v| v.max(0.));
    let maximum = rgb.into_iter().fold(f32::EPSILON, f32::max);
    let encoded = rgb.map(|v| {
        let v = v / maximum;
        let v = if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        };
        (v * 255.).round() as u8
    });
    Color32::from_rgb(encoded[0], encoded[1], encoded[2])
}

pub fn paint(ui: &egui::Ui, rect: egui::Rect) {
    let plot = plot_rect(rect);
    let points = locus();
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(point(plot, [1. / 3.; 2]), Color32::WHITE);
    for xy in points {
        mesh.colored_vertex(point(plot, *xy), color(*xy));
    }
    for i in 0..points.len() {
        mesh.add_triangle(0, (i + 1) as u32, ((i + 1) % points.len() + 1) as u32);
    }
    ui.painter().add(mesh);
    ui.painter().add(egui::Shape::closed_line(
        points.iter().map(|xy| point(plot, *xy)).collect(),
        egui::Stroke::new(1_f32, ui.visuals().weak_text_color()),
    ));
    ui.painter().text(
        egui::pos2(plot.right(), plot.bottom() + 10.),
        egui::Align2::CENTER_CENTER,
        "x",
        egui::FontId::proportional(11.),
        ui.visuals().weak_text_color(),
    );
    ui.painter().text(
        egui::pos2(plot.left() - 10., plot.top()),
        egui::Align2::CENTER_CENTER,
        "y",
        egui::FontId::proportional(11.),
        ui.visuals().weak_text_color(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visual_anchors_lie_on_the_locus_and_fit_equal_scale_axes() {
        let rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(260., 275.));
        let plot = plot_rect(rect);
        assert!((plot.width() / X_MAX - plot.height() / Y_MAX).abs() < 0.0001);
        assert_eq!(locus().len(), 95);
        for anchor in anchors() {
            assert!(locus().contains(&anchor));
        }
        for corner in corners(rect) {
            assert!(rect.contains(corner));
        }
    }
}
