//! Displays sampled current-buffer histograms and density scopes with explicit signal meaning and range diagnostics.

use crate::preferences::Scope;
use eframe::egui;
use foveonic::{
    image::{ColorSpace, Space},
    scopes::{Scopes, BINS},
};
const COLORS: [egui::Color32; 3] = [
    egui::Color32::from_rgb(225, 75, 72),
    egui::Color32::from_rgb(65, 195, 110),
    egui::Color32::from_rgb(80, 140, 240),
];

pub struct Images {
    parade: egui::TextureHandle,
    waveform: egui::TextureHandle,
}
impl Images {
    pub fn new(ctx: &egui::Context, data: &Scopes) -> Self {
        Self {
            parade: ctx.load_texture(
                "current buffer parade",
                density_image(data, true),
                egui::TextureOptions::LINEAR,
            ),
            waveform: ctx.load_texture(
                "current buffer waveform",
                density_image(data, false),
                egui::TextureOptions::LINEAR,
            ),
        }
    }
}
fn density_image(data: &Scopes, parade: bool) -> egui::ColorImage {
    let channels = data.channels();
    let width = if parade { BINS * channels } else { BINS };
    let mut image = egui::ColorImage::new([width, BINS], egui::Color32::from_gray(10));
    let max = (0..channels)
        .flat_map(|c| data.density(c))
        .copied()
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    for y in 0..BINS {
        for x in 0..width {
            let mut rgb = [10_f32; 3];
            for (c, tint) in COLORS.iter().enumerate().take(channels) {
                if parade && x / BINS != c {
                    continue;
                }
                let n = data.density(c)[y * BINS + x % BINS] as f32;
                let strength = (1. + n).ln() / (1. + max).ln();
                let color = if channels == 1 {
                    egui::Color32::LIGHT_GRAY
                } else {
                    *tint
                };
                for k in 0..3 {
                    rgb[k] += color[k] as f32 * strength * 0.85;
                }
            }
            if !parade
                && channels == 3
                && matches!(data.space, Space::LinearRgb(_) | Space::ToneMappedRgb(_))
            {
                let n = data.density(3)[y * BINS + x] as f32;
                let white = ((1. + n).ln() / (1. + max).ln()) * 95.;
                for channel in &mut rgb {
                    *channel += white;
                }
            }
            image.pixels[y * width + x] = egui::Color32::from_rgb(
                rgb[0].min(255.) as u8,
                rgb[1].min(255.) as u8,
                rgb[2].min(255.) as u8,
            );
        }
    }
    image
}
pub fn meaning(space: Space) -> String {
    match space {
        Space::SensorLayers => "Sensor layers · codes 0–4095".into(),
        Space::CalibratedLayers => "Calibrated layers · nominal codes 0–4095".into(),
        Space::ScalarField => "Scalar field · nominal 0–1".into(),
        Space::LinearRgb(c) | Space::ToneMappedRgb(c) => format!(
            "{} {} · linear signal 0–1",
            if matches!(space, Space::LinearRgb(_)) {
                "Linear"
            } else {
                "Tone-mapped"
            },
            if c == ColorSpace::AdobeRgb {
                "Adobe RGB"
            } else {
                "sRGB"
            }
        ),
        Space::EncodedRgb16(_) => "Encoded RGB".into(),
    }
}
pub fn show(ui: &mut egui::Ui, mode: Scope, data: Option<&Scopes>, images: Option<&Images>) {
    let Some(data) = data else {
        ui.allocate_ui(egui::vec2(ui.available_width(), 150.), |ui| {
            ui.centered_and_justified(|ui| {
                ui.weak("Awaiting current image buffer");
            });
        });
        return;
    };
    ui.small(meaning(data.space));
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 170.), egui::Sense::hover());
    let plot = rect.shrink2(egui::vec2(5., 12.));
    ui.painter()
        .rect_filled(rect, 3., egui::Color32::from_gray(10));
    match mode {
        Scope::Histogram => {
            let max = (0..data.channels())
                .flat_map(|c| data.histogram(c))
                .copied()
                .max()
                .unwrap_or(1)
                .max(1) as f32;
            for (c, tint) in COLORS.iter().enumerate().take(data.channels()) {
                let points: Vec<_> = data
                    .histogram(c)
                    .iter()
                    .enumerate()
                    .map(|(x, n)| {
                        egui::pos2(
                            plot.left() + x as f32 / 255. * plot.width(),
                            plot.bottom() - (*n as f32 / max).sqrt() * plot.height(),
                        )
                    })
                    .collect();
                let color = if data.channels() == 1 {
                    egui::Color32::LIGHT_GRAY
                } else {
                    *tint
                };
                let mut fill = egui::Mesh::default();
                for (index, p) in points.iter().enumerate() {
                    fill.colored_vertex(*p, color.gamma_multiply(0.18));
                    fill.colored_vertex(egui::pos2(p.x, plot.bottom()), color.gamma_multiply(0.18));
                    if index > 0 {
                        let k = index as u32 * 2;
                        fill.add_triangle(k - 2, k - 1, k);
                        fill.add_triangle(k - 1, k + 1, k);
                    }
                }
                ui.painter().add(egui::Shape::mesh(fill));
                ui.painter()
                    .add(egui::Shape::line(points, egui::Stroke::new(1_f32, color)));
            }
            let high = if matches!(data.space, Space::SensorLayers | Space::CalibratedLayers) {
                "4095"
            } else {
                "1"
            };
            ui.painter().text(
                rect.left_bottom(),
                egui::Align2::LEFT_BOTTOM,
                "0",
                egui::FontId::proportional(10.),
                egui::Color32::GRAY,
            );
            ui.painter().text(
                rect.right_bottom(),
                egui::Align2::RIGHT_BOTTOM,
                high,
                egui::FontId::proportional(10.),
                egui::Color32::GRAY,
            );
        }
        Scope::Parade | Scope::Waveform => {
            if let Some(images) = images {
                let texture = if mode == Scope::Parade {
                    &images.parade
                } else {
                    &images.waveform
                };
                ui.painter().image(
                    texture.id(),
                    plot,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    egui::Color32::WHITE,
                );
            }
        }
    }
    for fraction in [0., 0.25, 0.5, 0.75, 1.] {
        let y = plot.bottom() - fraction * plot.height();
        ui.painter().line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(0.5_f32, egui::Color32::from_white_alpha(35)),
        );
    }
    let outside = (0..data.channels()).fold([0_u32; 3], |mut sum, c| {
        let v = data.outside(c);
        for i in 0..3 {
            sum[i] += v[i];
        }
        sum
    });
    ui.small(format!(
        "Sampled {}×{} · below {} / above {} / invalid {}",
        data.samples[0], data.samples[1], outside[0], outside[1], outside[2]
    ));
    if matches!(data.space, Space::SensorLayers | Space::CalibratedLayers) {
        ui.small("Sensor layers 1 / 2 / 3");
    }
}
