//! Places source opening, destination and shared processing actions on one padded toolbar row.

use crate::{preferences::FolderMode, widgets};
use eframe::egui;
use foveonic::{render::Settings, source::Metadata};

/// Export requires a photographic recipe, rather than an inspection-only source.
pub fn can_export(settings: &Settings, metadata: Option<&Metadata>) -> bool {
    metadata.is_some_and(|metadata| {
        settings.validate_for_camera(metadata).is_ok()
            && foveonic::inspection::output_names(settings, Some(metadata)).contains(&"final")
    })
}

const GROUP_PADDING: i8 = 4;
const DESTINATION_MAX_WIDTH: f32 = 180.;

#[derive(Default)]
pub struct Actions {
    pub open_folder: Option<FolderMode>,
    pub destination: bool,
    pub settings: bool,
    pub process: bool,
}

pub struct State<'a> {
    pub enabled: bool,
    pub process_ready: bool,
    pub importing: bool,
    pub destination: Option<&'a std::path::Path>,
}

fn container_frame() -> egui::Frame {
    egui::Frame::new()
        .inner_margin(GROUP_PADDING)
        .corner_radius(6)
}

pub fn container<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    container_frame().show(ui, contents)
}

/// Reserve group height so grouped and ungrouped controls share a centre.
pub fn row<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), widgets::button_group_height(ui)),
        egui::Layout::left_to_right(egui::Align::Center),
        contents,
    )
}

/// Return actions so folder dialogs and processing stay owned by the app.
pub fn show(ui: &mut egui::Ui, state: State<'_>) -> Actions {
    let mut actions = Actions::default();
    ui.add_enabled_ui(state.enabled, |ui| {
        widgets::button_group(ui, |ui| {
            ui.horizontal_top(|ui| {
                for (mode, label, help) in [
                    (
                        FolderMode::Browse,
                        "Browse…",
                        "Open a folder and save edits beside its originals.",
                    ),
                    (
                        FolderMode::Import,
                        "Import…",
                        "Open a folder for import. Source edits stay temporary and are saved only with imported originals.",
                    ),
                ] {
                    let selected = state.importing == (mode == FolderMode::Import);
                    if widgets::button(ui, label, selected, egui::Vec2::ZERO)
                        .on_hover_text(help)
                        .clicked()
                    {
                        actions.open_folder = Some(mode);
                    }
                }
            });
        });
        let label = state.destination.map_or_else(
            || "Destination…".into(),
            |path| {
                format!(
                    "Destination: {}",
                    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy()
                )
            },
        );
        let response = ui.scope(|ui| {
            ui.spacing_mut().button_padding = widgets::button_padding(ui);
            let width = egui::WidgetText::from(label.as_str())
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::TextStyle::Button,
                )
                .size()
                .x + 2. * widgets::button_padding(ui).x;
            ui.add_sized(
                egui::vec2(width.min(DESTINATION_MAX_WIDTH), widgets::control_height(ui)),
                egui::Button::new(label).truncate(),
            )
        }).inner;
        let destination_help = state.destination.map_or_else(
            || "Choose the output folder".into(),
            |path| path.display().to_string(),
        );
        actions.destination = response.on_hover_text(destination_help).clicked();
        actions.settings = widgets::button(ui, "Processing settings…", false, egui::Vec2::ZERO).clicked();
        actions.process = ui.add_enabled_ui(state.process_ready, |ui| {
            let help = if state.importing {
                "Process picked photos and save hidden decisions using Import settings."
            } else {
                "Process all highlighted photos using Browse settings."
            };
            widgets::button(ui, "Process", false, egui::Vec2::ZERO).on_hover_text(help)
        }).inner.clicked();
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

    fn frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        enabled: bool,
        ready: bool,
        importing: bool,
    ) -> (Actions, egui::FullOutput) {
        let mut actions = Actions::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 300.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    container(ui, |ui| {
                        row(ui, |ui| {
                            actions = show(
                                ui,
                                State {
                                    enabled,
                                    process_ready: ready,
                                    importing,
                                    destination: None,
                                },
                            );
                        });
                    });
                });
            },
        );
        (actions, output)
    }

    #[test]
    fn both_modes_share_one_row_and_process_obeys_readiness() {
        for importing in [false, true] {
            for (enabled, ready) in [(true, true), (true, false), (false, true)] {
                let ctx = egui::Context::default();
                widgets::install_style(&ctx);
                frame(&ctx, vec![], enabled, ready, importing);
                let (_, output) = frame(&ctx, vec![], enabled, ready, importing);
                let y = text_center(&output, "Browse…").y;
                for label in ["Import…", "Destination…", "Processing settings…", "Process"] {
                    let point = text_center(&output, label);
                    assert!((point.y - y).abs() < 1., "{label}");
                    let pointer = |pressed| egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    };
                    frame(
                        &ctx,
                        vec![egui::Event::PointerMoved(point), pointer(true)],
                        enabled,
                        ready,
                        importing,
                    );
                    let (actions, _) = frame(&ctx, vec![pointer(false)], enabled, ready, importing);
                    assert_eq!(actions.process, enabled && ready && label == "Process");
                    assert_eq!(actions.settings, enabled && label == "Processing settings…");
                    assert_eq!(actions.destination, enabled && label == "Destination…");
                    assert_eq!(
                        actions.open_folder,
                        (enabled && label == "Import…").then_some(FolderMode::Import)
                    );
                }
            }
        }
    }
}
