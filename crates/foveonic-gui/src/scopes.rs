//! Displays current-buffer histograms and density scopes for the selected inspection stage.

use crate::preferences::Scope;
use eframe::egui;
use foveonic::{
    image::Space,
    scopes::{Scopes, BINS},
};
const COLORS: [egui::Color32; 3] = [
    egui::Color32::from_rgb(225, 75, 72),
    egui::Color32::from_rgb(65, 195, 110),
    egui::Color32::from_rgb(80, 140, 240),
];
const VIEWPORT_HEIGHT: f32 = 170.;

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
pub fn show(ui: &mut egui::Ui, mode: Scope, data: Option<&Scopes>, images: Option<&Images>) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), VIEWPORT_HEIGHT),
        egui::Sense::hover(),
    );
    let plot = rect.shrink(4.);
    ui.painter()
        .rect_filled(rect, 3., egui::Color32::from_gray(10));
    let Some(data) = data else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Awaiting current image buffer",
            egui::FontId::proportional(12.5),
            ui.visuals().weak_text_color(),
        );
        return;
    };
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
            let high = if matches!(
                data.space,
                Space::SensorLayers | Space::CalibratedLayers | Space::LinearRgb(_)
            ) {
                "4095"
            } else if data.space == Space::LuminanceRatio {
                "2"
            } else if data.space == Space::SignedDetail {
                "1000"
            } else {
                "1"
            };
            let low = if data.space == Space::SignedDetail {
                "−1000"
            } else {
                "0"
            };
            let font = egui::FontId::proportional(10.);
            for (pos, align, text) in [
                (plot.left_bottom(), egui::Align2::LEFT_BOTTOM, low),
                (plot.right_bottom(), egui::Align2::RIGHT_BOTTOM, high),
            ] {
                ui.painter().text(
                    pos + egui::vec2(1., 1.),
                    align,
                    text,
                    font.clone(),
                    egui::Color32::from_black_alpha(200),
                );
                ui.painter()
                    .text(pos, align, text, font.clone(), egui::Color32::GRAY);
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope_height(space: Option<Space>) -> f32 {
        let data = space.map(|space| Scopes {
            space,
            samples: [1, 1],
            counts: vec![0; 4 * BINS + 4 * BINS * BINS + 12],
        });
        let ctx = egui::Context::default();
        let mut height = 0.;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show(ui, Scope::Histogram, data.as_ref(), None);
                height = ui.min_rect().height();
            });
        });
        height
    }

    #[test]
    fn awaiting_scope_keeps_the_populated_height() {
        let awaiting = scope_height(None);
        assert_eq!(awaiting, scope_height(Some(Space::SensorLayers)));
        assert_eq!(awaiting, scope_height(Some(Space::ScalarField)));
    }
}
