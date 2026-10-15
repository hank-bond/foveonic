//! Presents embedded photographic comparisons through one image-switching popover template.

use eframe::egui;

struct Comparison {
    id: &'static str,
    title: &'static str,
    source_name: &'static str,
    description: &'static str,
    note: &'static str,
    image_size: [u32; 2],
    variants: &'static [Variant],
}

struct Variant {
    id: &'static str,
    label: &'static str,
    caption: &'static str,
    png: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/help_comparisons.rs"));

fn comparison(id: &str) -> &'static Comparison {
    COMPARISONS
        .iter()
        .find(|example| example.id == id)
        .expect("registered help comparison")
}

pub fn text(id: &str) -> &'static str {
    comparison(id).description
}

pub fn attach(response: egui::Response, id: &'static str) -> egui::Response {
    response
        .on_hover_ui(|ui| show(ui, comparison(id)))
        .on_disabled_hover_ui(|ui| show(ui, comparison(id)))
}

fn show(ui: &mut egui::Ui, example: &Comparison) {
    let width = (ui.ctx().screen_rect().width() - 32.).clamp(100., 400.);
    let height = (ui.ctx().screen_rect().height() - 48.).max(100.);
    ui.set_width(width);
    // Tooltip Areas initially offer a short default rectangle. Let the scroll
    // region use screen height instead of permanently clipping to that default.
    ui.set_max_height(height);
    egui::ScrollArea::vertical()
        .id_salt(("help-example-scroll", example.id))
        .max_height(height)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(example.title);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(example.source_name)
                                .text_style(egui::TextStyle::Body)
                                .italics(),
                        )
                        .truncate(),
                    );
                });
            });
            ui.label(example.description);
            ui.separator();
            ui.label(example.note);
            let selected_id = egui::Id::new(("help-example-selection", example.id));
            let mut selected = ui
                .ctx()
                .data_mut(|data| data.get_temp::<usize>(selected_id))
                .unwrap_or(0);
            selected = selected.min(example.variants.len() - 1);
            ui.horizontal(|ui| {
                for (index, variant) in example.variants.iter().enumerate() {
                    crate::widgets::selectable_value(ui, &mut selected, index, variant.label);
                }
            });
            ui.ctx()
                .data_mut(|data| data.insert_temp(selected_id, selected));
            let variant = &example.variants[selected];
            let texture_id = egui::Id::new(("help-example-texture", example.id, variant.id));
            let texture = ui
                .ctx()
                .data_mut(|data| data.get_temp::<egui::TextureHandle>(texture_id))
                .unwrap_or_else(|| {
                    let image = image::load_from_memory(variant.png)
                        .expect("validated help PNG")
                        .into_rgba8();
                    let color = egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    );
                    let texture = ui.ctx().load_texture(
                        format!("help/{}/{}", example.id, variant.id),
                        color,
                        egui::TextureOptions::LINEAR,
                    );
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(texture_id, texture.clone()));
                    texture
                });
            egui::Frame::new()
                .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                .corner_radius(4)
                .inner_margin(4)
                .show(ui, |ui| {
                    // The caption meets the image edge, inside one offset border.
                    ui.spacing_mut().item_spacing.y = 0.;
                    let image_width = ui.available_width();
                    let size =
                        egui::vec2(example.image_size[0] as f32, example.image_size[1] as f32);
                    ui.image((texture.id(), size * (image_width / size.x)));
                    egui::Frame::new()
                        .fill(egui::Color32::from_gray(32))
                        .inner_margin(6)
                        .show(ui, |ui| {
                            // Reserve the largest caption height so switching variants
                            // does not move an above-anchored tooltip or its image.
                            let caption_width = ui.available_width();
                            let caption_height = example
                                .variants
                                .iter()
                                .map(|variant| {
                                    ui.painter()
                                        .layout(
                                            variant.caption.into(),
                                            egui::TextStyle::Body.resolve(ui.style()),
                                            ui.visuals().text_color(),
                                            caption_width,
                                        )
                                        .size()
                                        .y
                                })
                                .fold(0_f32, f32::max);
                            ui.set_min_size(egui::vec2(caption_width, caption_height));
                            ui.label(variant.caption);
                        });
                });
        });
}

#[cfg(test)]
#[path = "../build_support.rs"]
mod build_checks;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_embedded_images_decode_at_the_manifest_size() {
        assert!(!COMPARISONS.is_empty());
        for example in COMPARISONS {
            for variant in example.variants {
                let image = image::load_from_memory(variant.png).unwrap();
                assert_eq!([image.width(), image.height()], example.image_size);
            }
        }
    }

    #[test]
    fn popover_typography_and_photo_credit_follow_the_app_style() {
        let ctx = egui::Context::default();
        crate::widgets::install_style(&ctx);
        let example = comparison("highlight-correction");
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 1000.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show(ui, example));
            },
        );
        assert_eq!(example.source_name, "sunlit-red-flower");
        for (text, size, italic) in [
            (example.title, 18., false),
            (example.source_name, 13., true),
            (example.description, 13., false),
            (example.note, 13., false),
            (example.variants[0].caption, 13., false),
        ] {
            let galley = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(shape) if shape.galley.text() == text => Some(&shape.galley),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing popover text: {text}"));
            for section in &galley.job.sections {
                assert_eq!(section.format.font_id.size, size, "{text}");
                assert_eq!(section.format.italics, italic, "{text}");
            }
        }
        let text_rect = |label| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
                .unwrap()
        };
        let heading = text_rect(example.title);
        let credit = text_rect(example.source_name);
        assert!(heading.right() < credit.left());
        assert!((heading.center().y - credit.center().y).abs() <= 1.);
        let texture_id =
            egui::Id::new(("help-example-texture", example.id, example.variants[0].id));
        let texture = ctx
            .data_mut(|data| data.get_temp::<egui::TextureHandle>(texture_id))
            .unwrap();
        let image = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill_texture_id() == texture.id() => {
                    Some(rect.rect)
                }
                _ => None,
            })
            .unwrap();
        let caption = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == egui::Color32::from_gray(32) => {
                    Some(rect.rect)
                }
                _ => None,
            })
            .unwrap();
        assert!((caption.top() - image.bottom()).abs() <= 1.);
        assert!((caption.left() - image.left()).abs() <= 1.);
        assert!((caption.right() - image.right()).abs() <= 1.);
        assert!(caption.contains_rect(text_rect(example.variants[0].caption)));
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(rect) if rect.stroke.width == 1.
                && rect.rect.contains_rect(image.expand(4.))
                && rect.rect.contains_rect(caption.expand(4.)))));
    }

    #[test]
    fn variant_buttons_switch_only_the_example_image() {
        for example in COMPARISONS {
            let ctx = egui::Context::default();
            let mut time = 0.;
            let mut frame = |events| {
                time += 0.1;
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800., 1000.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| show(ui, example));
                    },
                )
            };
            frame(vec![]);
            let mut image_rect = None;
            for (index, variant) in example.variants.iter().enumerate() {
                let label = variant.label;
                let output = frame(vec![]);
                let center = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some(text.pos + text.galley.size() * 0.5)
                        }
                        _ => None,
                    })
                    .unwrap();
                for pressed in [true, false] {
                    frame(vec![
                        egui::Event::PointerMoved(center),
                        egui::Event::PointerButton {
                            pos: center,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ]);
                }
                let output = frame(vec![]);
                let selected_id = egui::Id::new(("help-example-selection", example.id));
                assert_eq!(
                    ctx.data_mut(|data| data.get_temp::<usize>(selected_id)),
                    Some(index)
                );
                assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == example.variants[index].caption)));
                let texture_id = egui::Id::new((
                    "help-example-texture",
                    example.id,
                    example.variants[index].id,
                ));
                let texture = ctx
                    .data_mut(|data| data.get_temp::<egui::TextureHandle>(texture_id))
                    .unwrap();
                let rect = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) if rect.fill_texture_id() == texture.id() => {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .expect("selected variant image is drawn");
                assert_eq!(*image_rect.get_or_insert(rect), rect);
            }
        }
    }
}
