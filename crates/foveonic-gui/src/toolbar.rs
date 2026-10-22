//! Groups Open/Export actions in nested padded containers outside the editor.

use crate::{help, widgets, worker::ExportFormat};
use eframe::egui;
use foveonic::{
    render::{Settings, TiffColorSpace},
    source::Metadata,
};

/// Export requires a photographic recipe, rather than an inspection-only source.
pub fn can_export(settings: &Settings, metadata: Option<&Metadata>) -> bool {
    metadata.is_some_and(|metadata| {
        settings.validate_for_camera(metadata).is_ok()
            && foveonic::inspection::output_names(settings, Some(metadata)).contains(&"final")
    })
}

#[derive(Default)]
pub struct Actions {
    pub open_folder: bool,
    pub open_file: bool,
    pub export: Option<ExportFormat>,
}

fn container_frame() -> egui::Frame {
    egui::Frame::new().inner_margin(4).corner_radius(6)
}

/// The outer bar adds the same padding as its groups, without decoration.
pub fn container<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    container_frame().show(ui, contents)
}

/// Draw inside the toolbar's horizontal layout. Actions are handled by the app
/// so tests can exercise the controls without opening native file dialogs.
pub fn show(
    ui: &mut egui::Ui,
    enabled: bool,
    export_ready: bool,
    destination: &mut Option<TiffColorSpace>,
) -> Actions {
    let mut actions = Actions::default();
    ui.add_enabled_ui(enabled, |ui| {
        // Top-aligned inner rows keep intrinsic control heights from shifting
        // later controls. The outer row centers groups and the ungrouped DNG button.
        widgets::button_group(ui, |ui| {
            ui.horizontal_top(|ui| {
                actions.open_folder =
                    widgets::button(ui, "Open folder", false, egui::Vec2::ZERO).clicked();
                actions.open_file =
                    widgets::button(ui, "Open X3F", false, egui::Vec2::ZERO).clicked();
            });
        });
        ui.add_enabled_ui(export_ready, |ui| {
            widgets::button_group(ui, |ui| {
                ui.horizontal_top(|ui| {
                    if widgets::button(ui, "Export TIFF", false, egui::Vec2::ZERO).clicked() {
                        actions.export = Some(ExportFormat::Tiff);
                    }
                    let combo = egui::ComboBox::from_id_salt("tiff-color-space")
                        .width(widgets::dropdown_width(
                            ui,
                            std::iter::once("Camera")
                                .chain(TiffColorSpace::ALL.map(TiffColorSpace::label)),
                        ))
                        .selected_text(destination.map_or("Camera", TiffColorSpace::label));
                    let response = widgets::dropdown(ui, combo, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        widgets::menu_value(ui, destination, None, "Camera");
                        for space in TiffColorSpace::ALL {
                            widgets::menu_value(ui, destination, Some(space), space.label());
                        }
                    })
                    .response;
                    help::control(response, "TIFF space");
                });
            });
            if widgets::button(ui, "Export DNG", false, egui::Vec2::ZERO).clicked() {
                actions.export = Some(ExportFormat::Dng);
            }
        });
    });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_requires_a_supported_photographic_output() {
        use foveonic::source::{Matrix, MatrixEncoding};
        let settings = Settings::new("Sunlight", "Standard");
        let mut metadata = Metadata {
            width: 5888,
            height: 3776,
            rotation: 0,
            processing_version: Some([3, 1, 2, 0]),
            properties: [("CAMMODEL".into(), "SIGMA sd Quattro".into())].into(),
            matrices: Default::default(),
            lists: Default::default(),
        };
        for (name, value) in [("CAMERAID", 40.), ("CaptureISO", 100.)] {
            metadata.matrices.insert(
                name.into(),
                Matrix {
                    encoding: MatrixEncoding::Float32,
                    representation: "test",
                    dimensions: vec![1],
                    values: vec![value],
                },
            );
        }
        assert!(!can_export(&settings, None));
        assert!(can_export(&settings, Some(&metadata)));
        let mut mono = settings.clone();
        mono.rendering = foveonic::render::Rendering::Monochrome;
        assert!(can_export(&mono, Some(&metadata)));
        metadata.width /= 2;
        assert!(!can_export(&settings, Some(&metadata)), "reduced input");
        metadata.width *= 2;
        metadata.processing_version = Some([4, 0, 1, 0]);
        metadata
            .properties
            .insert("CAMMODEL".into(), "SIGMA sd Quattro H".into());
        assert!(!can_export(&settings, Some(&metadata)), "Tiger inspection");
    }

    fn text_center(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .next_back()
            .unwrap()
    }
    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    fn frame(
        ctx: &egui::Context,
        time: f64,
        events: Vec<egui::Event>,
        enabled: bool,
        ready: bool,
        destination: &mut Option<TiffColorSpace>,
    ) -> (Actions, egui::FullOutput) {
        let mut actions = Actions::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 300.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let spacing = ui.spacing().clone();
                    container(ui, |ui| {
                        ui.horizontal(|ui| {
                            actions = show(ui, enabled, ready, destination);
                        });
                    });
                    assert_eq!(ui.spacing().button_padding, spacing.button_padding);
                    assert_eq!(ui.spacing().item_spacing, spacing.item_spacing);
                });
            },
        );
        (actions, output)
    }

    #[test]
    fn selector_changes_export_choice_and_actions_use_it_without_touching_edits() {
        let ctx = egui::Context::default();
        widgets::install_style(&ctx);
        let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
        settings.sharpen_amount = None;
        let original = settings.clone();
        let mut run = |time, events| {
            let (actions, output) = frame(
                &ctx,
                time,
                events,
                true,
                true,
                &mut settings.tiff_color_space,
            );
            (actions, output, settings.tiff_color_space)
        };
        run(0., vec![]);
        let (_, output, _) = run(0.1, vec![]);
        let mut point = text_center(&output, "Camera");
        for (index, (label, destination)) in [
            ("sRGB", Some(TiffColorSpace::Srgb)),
            ("Adobe RGB", Some(TiffColorSpace::AdobeRgb)),
            ("ProPhoto RGB", Some(TiffColorSpace::ProPhotoRgb)),
            ("Camera", None),
        ]
        .into_iter()
        .enumerate()
        {
            let time = 1. + index as f64;
            run(
                time,
                vec![egui::Event::PointerMoved(point), pointer(point, true)],
            );
            run(time + 0.1, vec![pointer(point, false)]);
            let (_, popup, _) = run(time + 0.2, vec![]);
            let choice = text_center(&popup, label);
            run(
                time + 0.3,
                vec![egui::Event::PointerMoved(choice), pointer(choice, true)],
            );
            let (action, _, selected) = run(time + 0.4, vec![pointer(choice, false)]);
            assert!(action.export.is_none());
            let (_, output, _) = run(time + 0.5, vec![]);
            point = text_center(&output, label);
            assert_eq!(selected, destination);
        }
        assert_eq!(settings, original);
        for (index, (label, expected)) in [
            ("Open folder", None),
            ("Open X3F", None),
            ("Export TIFF", Some(ExportFormat::Tiff)),
            ("Export DNG", Some(ExportFormat::Dng)),
        ]
        .into_iter()
        .enumerate()
        {
            let time = 10. + index as f64;
            let (_, output) = frame(
                &ctx,
                time,
                vec![],
                true,
                true,
                &mut settings.tiff_color_space,
            );
            let point = text_center(&output, label);
            frame(
                &ctx,
                time + 0.1,
                vec![egui::Event::PointerMoved(point), pointer(point, true)],
                true,
                true,
                &mut settings.tiff_color_space,
            );
            let (action, _) = frame(
                &ctx,
                time + 0.2,
                vec![pointer(point, false)],
                true,
                true,
                &mut settings.tiff_color_space,
            );
            assert_eq!(action.export, expected);
            assert_eq!(action.open_folder, label == "Open folder");
            assert_eq!(action.open_file, label == "Open X3F");
        }
    }

    #[test]
    fn disabled_exports_keep_selection_and_open_buttons_need_only_idle_state() {
        for (enabled, ready) in [(false, true), (true, false)] {
            for label in [
                "Export TIFF",
                "Export DNG",
                "ProPhoto RGB",
                "Open folder",
                "Open X3F",
            ] {
                let ctx = egui::Context::default();
                let mut destination = Some(TiffColorSpace::ProPhotoRgb);
                frame(&ctx, 0., vec![], enabled, ready, &mut destination);
                let (_, output) = frame(&ctx, 0.1, vec![], enabled, ready, &mut destination);
                let point = text_center(&output, label);
                frame(
                    &ctx,
                    1.,
                    vec![egui::Event::PointerMoved(point), pointer(point, true)],
                    enabled,
                    ready,
                    &mut destination,
                );
                let (action, output) = frame(
                    &ctx,
                    1.1,
                    vec![pointer(point, false)],
                    enabled,
                    ready,
                    &mut destination,
                );
                assert!(action.export.is_none());
                assert_eq!(action.open_folder, enabled && label == "Open folder");
                assert_eq!(action.open_file, enabled && label == "Open X3F");
                assert_eq!(destination, Some(TiffColorSpace::ProPhotoRgb));
                assert!(output.shapes.iter().all(|shape| !matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.job.text == "sRGB")));
            }
        }
    }

    #[test]
    fn color_space_dropdown_fits_its_longest_option_without_resizing_for_selection() {
        for em in [13., 20.] {
            let ctx = egui::Context::default();
            widgets::install_style(&ctx);
            ctx.style_mut(|style| style.override_font_id = Some(egui::FontId::proportional(em)));
            let mut widths = Vec::new();
            for (index, mut destination) in [
                None,
                Some(TiffColorSpace::Srgb),
                Some(TiffColorSpace::AdobeRgb),
                Some(TiffColorSpace::ProPhotoRgb),
            ]
            .into_iter()
            .enumerate()
            {
                let label = destination.map_or("Camera", TiffColorSpace::label);
                let (_, output) = frame(&ctx, index as f64, vec![], true, true, &mut destination);
                let point = text_center(&output, label);
                let rect = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect)
                            if rect.stroke.width > 0. && rect.rect.contains(point) =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .min_by(|a, b| a.area().total_cmp(&b.area()))
                    .unwrap();
                widths.push(rect.width());
                if destination == Some(TiffColorSpace::ProPhotoRgb) {
                    let text_width = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.job.text == label => {
                                Some(text.galley.size().x)
                            }
                            _ => None,
                        })
                        .unwrap();
                    // Button side padding totals one em, plus the arrow and its gap.
                    let style = ctx.style();
                    let expected =
                        text_width + em + style.spacing.icon_width + style.spacing.icon_spacing;
                    assert!((rect.width() - expected).abs() < 1.);
                }
            }
            assert!(widths.iter().all(|width| (width - widths[0]).abs() < 0.01));
        }
    }

    #[test]
    fn inner_and_outer_containers_have_the_same_four_pixel_padding() {
        let ctx = egui::Context::default();
        let mut outer = egui::Rect::NOTHING;
        let mut inner = egui::Rect::NOTHING;
        let mut button = egui::Rect::NOTHING;
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                outer = container(ui, |ui| {
                    inner = widgets::button_group(ui, |ui| {
                        button = widgets::button(ui, "Padding", false, egui::Vec2::ZERO).rect;
                    })
                    .response
                    .rect;
                })
                .response
                .rect;
            });
        });
        for (parent, child) in [(outer, inner), (inner, button)] {
            for gap in [
                child.left() - parent.left(),
                child.top() - parent.top(),
                parent.right() - child.right(),
                parent.bottom() - child.bottom(),
            ] {
                assert!((gap - 4.).abs() < 0.01);
            }
        }
        for shape in &output.shapes {
            if let egui::Shape::Rect(rect) = &shape.shape {
                if rect.rect == outer {
                    assert_eq!(rect.stroke.width, 0.);
                    assert_eq!(rect.fill, egui::Color32::TRANSPARENT);
                }
            }
        }
    }

    #[test]
    fn toolbar_uses_two_borderless_gray_groups_and_standard_control_padding() {
        for em in [13., 20.] {
            let ctx = egui::Context::default();
            widgets::install_style(&ctx);
            ctx.style_mut(|style| style.override_font_id = Some(egui::FontId::proportional(em)));
            frame(&ctx, 0., vec![], true, true, &mut None);
            let (_, output) = frame(&ctx, 0.1, vec![], true, true, &mut None);
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if ["Open folder", "Open X3F", "Export TIFF", "Export DNG"]
                            .contains(&text.galley.job.text.as_str()) =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(texts.len(), 4);
            let center_y = text_center(&output, "Open folder").y;
            for label in ["Open X3F", "Export TIFF", "Camera", "Export DNG"] {
                assert!(
                    (text_center(&output, label).y - center_y).abs() < 1.,
                    "{label} y={} vs {center_y}",
                    text_center(&output, label).y
                );
            }
            for text in &texts {
                let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                let button = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect)
                            if rect.rect.contains_rect(bounds) && rect.stroke.width > 0. =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .min_by(|a, b| a.area().total_cmp(&b.area()))
                    .unwrap();
                assert!((button.width() - bounds.width() - em).abs() < 1.);
                assert!((button.height() - bounds.height() - 0.5 * em).abs() < 1.);
            }
            let groups: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.fill == widgets::MENU_BACKGROUND && rect.stroke.width == 0. =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(groups.len(), 2);
            assert!(groups[0].contains(text_center(&output, "Open folder")));
            assert!(groups[0].contains(text_center(&output, "Open X3F")));
            assert!(groups[1].contains(text_center(&output, "Export TIFF")));
            assert!(groups[1].contains(text_center(&output, "Camera")));
            for rect in &groups {
                assert!(!rect.contains(text_center(&output, "Export DNG")));
            }
            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Rect(rect) if rect.rect.contains(text_center(&output, "Open folder"))
                    && rect.rect.contains(text_center(&output, "Export DNG")) && rect.stroke.width > 0.)));
        }
    }
}
