//! Draws histograms, waveforms, and parades for the selected image buffer.

use crate::{help, preferences::Scope, widgets};
use eframe::egui::{self, emath::GuiRounding};
use foveonic::{
    image::Space,
    scopes::{Scopes, BINS, DENSITY_SCALE},
};
const COLORS: [egui::Color32; 3] = [
    egui::Color32::from_rgb(225, 75, 72),
    egui::Color32::from_rgb(65, 195, 110),
    egui::Color32::from_rgb(80, 140, 240),
];
const VIEWPORT_HEIGHT: f32 = 170.;

// Leave breathing room between the dropdown frame and the viewport checkbox.
const CONTROL_PADDING: f32 = 8.;

pub fn controls(ui: &mut egui::Ui, scope: &mut Scope, viewport: &mut bool) {
    ui.horizontal(|ui| {
        let viewport_label = egui::WidgetText::from("Viewport").into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        );
        let viewport_width =
            viewport_label.size().x + ui.spacing().icon_width + ui.spacing().icon_spacing;
        let dropdown_width =
            (ui.available_width() - viewport_width - ui.spacing().item_spacing.x - CONTROL_PADDING)
                .max(0.);
        let combo = egui::ComboBox::from_id_salt("scope mode")
            .width(dropdown_width)
            .selected_text(match *scope {
                Scope::Histogram => "Histogram",
                Scope::Parade => "RGB / layer parade",
                Scope::Waveform => "Combined waveform",
            });
        widgets::dropdown(ui, combo, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            for (value, name) in [
                (Scope::Histogram, "Histogram"),
                (Scope::Parade, "RGB / layer parade"),
                (Scope::Waveform, "Combined waveform"),
            ] {
                widgets::menu_value(ui, scope, value, name).on_hover_text(help::scope(value));
            }
        })
        .response
        .on_hover_text(help::scope(*scope));
        widgets::checkbox(ui, viewport, "Viewport").on_hover_text(concat!(
            "Show scopes for the visible part of the image. Updates as you pan or zoom. ",
            "Channel, Input/Output, and Difference views do not change scope values.",
        ));
    });
}

pub struct Images {
    parade: PlotImage,
    waveform: PlotImage,
}
impl Images {
    pub fn new(data: &Scopes) -> Self {
        Self {
            parade: PlotImage::new(density_image(data, true), data.channels()),
            waveform: PlotImage::new(density_image(data, false), 1),
        }
    }
}

struct PlotImage {
    source: image::RgbaImage,
    panels: usize,
    texture: Option<egui::TextureHandle>,
}
impl PlotImage {
    fn new(source: image::RgbaImage, panels: usize) -> Self {
        Self {
            source,
            panels,
            texture: None,
        }
    }

    fn texture(&mut self, ctx: &egui::Context, size: [usize; 2]) -> egui::TextureId {
        if self
            .texture
            .as_ref()
            .is_none_or(|texture| texture.size() != size)
        {
            // Resize only the display image. Leave scope measurements unchanged.
            let resized = resample_display(&self.source, size, self.panels);
            let image = egui::ColorImage::from_rgba_unmultiplied(size, resized.as_raw());
            if let Some(texture) = &mut self.texture {
                texture.set(image, egui::TextureOptions::NEAREST);
            } else {
                self.texture = Some(ctx.load_texture(
                    "Pixel-sized scope plot",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
            }
        }
        self.texture.as_ref().unwrap().id()
    }
}

fn resample_display(
    source: &image::RgbaImage,
    size: [usize; 2],
    panels: usize,
) -> image::RgbaImage {
    if panels == 1 {
        return image::imageops::resize(
            source,
            size[0] as u32,
            size[1] as u32,
            image::imageops::FilterType::Lanczos3,
        );
    }
    let mut output = image::RgbaImage::new(size[0] as u32, size[1] as u32);
    let source_width = source.width() / panels as u32;
    for panel in 0..panels {
        let left = panel * size[0] / panels;
        let right = (panel + 1) * size[0] / panels;
        if left == right {
            continue;
        }
        // Resize parade channels separately so their colors do not mix.
        let input = image::imageops::crop_imm(
            source,
            panel as u32 * source_width,
            0,
            source_width,
            source.height(),
        );
        let resized = image::imageops::resize(
            &*input,
            (right - left) as u32,
            size[1] as u32,
            image::imageops::FilterType::Lanczos3,
        );
        image::imageops::replace(&mut output, &resized, left as i64, 0);
    }
    output
}

fn display_geometry(plot: egui::Rect, pixels_per_point: f32) -> (egui::Rect, [usize; 2]) {
    let aligned = plot.round_to_pixels(pixels_per_point);
    let size = [
        (aligned.width() * pixels_per_point).round().max(1.) as usize,
        (aligned.height() * pixels_per_point).round().max(1.) as usize,
    ];
    (aligned, size)
}
fn density_image(data: &Scopes, parade: bool) -> image::RgbaImage {
    let channels = data.channels();
    let width = if parade { BINS * channels } else { BINS };
    let mut image = image::RgbaImage::new(width as u32, BINS as u32);
    let max = (0..channels)
        .flat_map(|c| data.density(c))
        .copied()
        .max()
        .unwrap_or(DENSITY_SCALE)
        .max(DENSITY_SCALE) as f32
        / DENSITY_SCALE as f32;
    let log_max = (1. + max).ln();
    for y in 0..BINS {
        for x in 0..width {
            let mut rgb = [10_f32; 3];
            for (c, tint) in COLORS.iter().enumerate().take(channels) {
                if parade && x / BINS != c {
                    continue;
                }
                let n = data.density(c)[y * BINS + x % BINS] as f32 / DENSITY_SCALE as f32;
                let strength = (1. + n).ln() / log_max;
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
                let n = data.density(3)[y * BINS + x] as f32 / DENSITY_SCALE as f32;
                let white = ((1. + n).ln() / log_max) * 95.;
                for channel in &mut rgb {
                    *channel += white;
                }
            }
            image.put_pixel(
                x as u32,
                y as u32,
                image::Rgba([
                    rgb[0].min(255.) as u8,
                    rgb[1].min(255.) as u8,
                    rgb[2].min(255.) as u8,
                    255,
                ]),
            );
        }
    }
    image
}
pub fn show(ui: &mut egui::Ui, mode: Scope, data: Option<&Scopes>, images: Option<&mut Images>) {
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
            "Waiting for image",
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
            let linear_white = data.linear_white.to_string();
            let high = if matches!(data.space, Space::LinearRgb(_)) {
                linear_white.as_str()
            } else if matches!(
                data.space,
                Space::SensorLayers | Space::CalibratedLayers | Space::LinearRgb(_)
            ) {
                "4095"
            } else if data.space == Space::UnsignedCodeRgb {
                "65535"
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
                let image = if mode == Scope::Parade {
                    &mut images.parade
                } else {
                    &mut images.waveform
                };
                let (aligned, size) = display_geometry(plot, ui.ctx().pixels_per_point());
                let texture = image.texture(ui.ctx(), size);
                ui.painter().image(
                    texture,
                    aligned,
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
    use image::GenericImageView;

    #[test]
    fn scope_display_bounds_are_aligned_to_physical_pixels() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.23, 20.61), egui::vec2(397.45, 162.));
        for density in [1., 1.25, 1.5, 2., 3.] {
            let (aligned, size) = display_geometry(rect, density);
            for point in [aligned.min, aligned.max] {
                for coordinate in [point.x, point.y] {
                    assert!((coordinate * density - (coordinate * density).round()).abs() < 0.0001);
                }
            }
            assert!((aligned.width() * density - size[0] as f32).abs() < 0.0001);
            assert!((aligned.height() * density - size[1] as f32).abs() < 0.0001);
        }
    }

    #[test]
    fn one_to_one_display_is_unchanged_and_parade_channels_do_not_bleed() {
        let mut source = image::RgbaImage::new(6, 2);
        for (x, _, pixel) in source.enumerate_pixels_mut() {
            *pixel = image::Rgba(match x / 2 {
                0 => [200, 10, 10, 255],
                1 => [10, 200, 10, 255],
                _ => [10, 10, 200, 255],
            });
        }
        assert_eq!(resample_display(&source, [6, 2], 3), source);
        let resized = resample_display(&source, [11, 7], 3);
        for (x, _, pixel) in resized.enumerate_pixels() {
            let expected = if x < 3 {
                [200, 10, 10, 255]
            } else if x < 7 {
                [10, 200, 10, 255]
            } else {
                [10, 10, 200, 255]
            };
            assert_eq!(pixel.0, expected);
        }
    }

    #[test]
    fn display_minification_filters_high_frequency_pixels_without_beating() {
        let source = image::RgbaImage::from_fn(256, 256, |x, y| {
            let value = if (x + y) % 2 == 0 { 0 } else { 255 };
            image::Rgba([value, value, value, 255])
        });
        for size in [[43, 37], [79, 81], [123, 129]] {
            let resized = resample_display(&source, size, 1);
            for (_, _, pixel) in
                image::imageops::crop_imm(&resized, 3, 3, size[0] as u32 - 6, size[1] as u32 - 6)
                    .pixels()
            {
                assert!((125..=130).contains(&pixel[0]));
                assert_eq!(pixel[3], 255);
            }
        }
    }

    #[test]
    fn density_weights_use_sample_units_for_the_existing_brightness_mapping() {
        let mut data = Scopes {
            linear_white: 4095,
            space: Space::SensorLayers,
            samples: [4, 1],
            counts: vec![0; 4 * BINS + 4 * BINS * BINS + 12],
        };
        data.counts[4 * BINS] = 4 * DENSITY_SCALE;
        data.counts[4 * BINS + 1] = DENSITY_SCALE / 2;
        let display = density_image(&data, false);
        assert_eq!(display.get_pixel(0, 0).0, [201, 73, 71, 255]);
        let strength = 1.5_f32.ln() / 5_f32.ln();
        let rgb: [u8; 3] =
            std::array::from_fn(|i| (10. + COLORS[0][i] as f32 * strength * 0.85) as u8);
        assert_eq!(display.get_pixel(1, 0).0, [rgb[0], rgb[1], rgb[2], 255]);
        assert_eq!(display.get_pixel(2, 0).0, [10, 10, 10, 255]);
    }

    #[test]
    fn display_resampling_preserves_counts_and_reuses_its_texture() {
        let ctx = egui::Context::default();
        let mut data = Scopes {
            linear_white: 4095,
            space: Space::ToneMappedRgb(foveonic::image::ColorSpace::Srgb),
            samples: [512, 512],
            counts: vec![0; 4 * BINS + 4 * BINS * BINS + 12],
        };
        data.counts[4 * BINS + 100 * BINS + 50] = 25 * DENSITY_SCALE;
        let original = data.counts.clone();
        let mut images = Images::new(&data);
        let source = images.waveform.source.clone();
        let id = images.waveform.texture(&ctx, [794, 324]);
        assert_eq!(images.waveform.texture.as_ref().unwrap().size(), [794, 324]);
        assert_eq!(images.waveform.texture(&ctx, [794, 324]), id);
        assert_eq!(images.waveform.texture(&ctx, [633, 324]), id);
        assert_eq!(images.waveform.source, source);
        assert_eq!(data.counts, original);
        assert!(images.parade.texture.is_none());
    }

    fn scope_height(space: Option<Space>) -> f32 {
        let data = space.map(|space| Scopes {
            linear_white: 4095,
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
