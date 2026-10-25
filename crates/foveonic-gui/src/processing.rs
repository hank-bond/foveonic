//! Remembers per-mode output choices and edits them independently of photographic recipes.

use crate::{export::ExportFormat, preferences::FolderMode, widgets};
use eframe::egui;
use foveonic::{importing::ConflictPolicy, render::TiffColorSpace};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub format: ExportFormat,
    pub tiff_color_space: Option<TiffColorSpace>,
    pub copy_only: bool,
    pub conflict: ConflictPolicy,
}

impl Settings {
    pub fn formats(&self) -> Vec<ExportFormat> {
        if self.copy_only {
            Vec::new()
        } else {
            vec![self.format]
        }
    }
}

pub fn show(ctx: &egui::Context, open: &mut bool, mode: FolderMode, settings: &mut Settings) {
    if !*open {
        return;
    }
    let response = egui::Modal::new(egui::Id::new("processing-settings")).show(ctx, |ui| {
        ui.heading(match mode {
            FolderMode::Browse => "Browse processing settings",
            FolderMode::Import => "Import processing settings",
        });
        if mode == FolderMode::Import {
            ui.horizontal(|ui| {
                ui.label("Operation");
                let combo = egui::ComboBox::from_id_salt("processing-operation")
                    .width(widgets::dropdown_width(
                        ui,
                        ["Process + originals", "Copy originals"],
                    ))
                    .selected_text(if settings.copy_only {
                        "Copy originals"
                    } else {
                        "Process + originals"
                    });
                widgets::dropdown(ui, combo, |ui| {
                    widgets::menu_value(ui, &mut settings.copy_only, false, "Process + originals");
                    widgets::menu_value(ui, &mut settings.copy_only, true, "Copy originals");
                });
            });
        }
        if mode == FolderMode::Browse || !settings.copy_only {
            ui.horizontal(|ui| {
                ui.label("Format");
                let combo = egui::ComboBox::from_id_salt("processing-format")
                    .width(widgets::dropdown_width(ui, ["DNG", "TIFF"]))
                    .selected_text(settings.format.label());
                widgets::dropdown(ui, combo, |ui| {
                    for format in [ExportFormat::Dng, ExportFormat::Tiff] {
                        widgets::menu_value(ui, &mut settings.format, format, format.label());
                    }
                });
            });
            if settings.format == ExportFormat::Tiff {
                ui.horizontal(|ui| {
                    ui.label("Color space");
                    let combo = egui::ComboBox::from_id_salt("processing-tiff-space")
                        .width(widgets::dropdown_width(
                            ui,
                            std::iter::once("Camera")
                                .chain(TiffColorSpace::ALL.map(TiffColorSpace::label)),
                        ))
                        .selected_text(
                            settings
                                .tiff_color_space
                                .map_or("Camera", TiffColorSpace::label),
                        );
                    widgets::dropdown(ui, combo, |ui| {
                        widgets::menu_value(ui, &mut settings.tiff_color_space, None, "Camera");
                        for space in TiffColorSpace::ALL {
                            widgets::menu_value(
                                ui,
                                &mut settings.tiff_color_space,
                                Some(space),
                                space.label(),
                            );
                        }
                    });
                });
            }
        }
        ui.horizontal(|ui| {
            ui.label("On conflict");
            let label = |policy| match policy {
                ConflictPolicy::Overwrite => "Overwrite",
                ConflictPolicy::Rename => "Rename",
            };
            let combo = egui::ComboBox::from_id_salt("processing-conflict")
                .width(widgets::dropdown_width(ui, ["Overwrite", "Rename"]))
                .selected_text(label(settings.conflict));
            widgets::dropdown(ui, combo, |ui| {
                for policy in [ConflictPolicy::Overwrite, ConflictPolicy::Rename] {
                    widgets::menu_value(ui, &mut settings.conflict, policy, label(policy));
                }
            });
        });
        ui.weak("Remembered for this mode. Process uses these choices immediately.");
        if mode == FolderMode::Import {
            ui.weak(if settings.copy_only {
                "Copy X3Fs and edits directly into the destination."
            } else {
                "Write processed files into the destination and originals into x3f/."
            });
        }
        if widgets::button(ui, "Done", false, egui::Vec2::ZERO).clicked() {
            *open = false;
        }
    });
    if response.should_close() {
        *open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn has_label(output: &egui::FullOutput, label: &str) -> bool {
        output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == label))
    }

    #[test]
    fn modal_changes_format_and_tiff_space_and_closes_without_running_work() {
        let ctx = egui::Context::default();
        widgets::install_style(&ctx);
        let mut settings = Settings::default();
        let mut open = true;
        let mut run = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| show(ctx, &mut open, FolderMode::Import, &mut settings),
            )
        };
        run(vec![]);
        let output = run(vec![]);
        assert!(!has_label(&output, "Color space"));
        let mut click = |output: &egui::FullOutput, label: &str| {
            let point = text_center(output, label);
            let pointer = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            run(vec![egui::Event::PointerMoved(point), pointer(true)]);
            run(vec![pointer(false)]);
            run(vec![])
        };
        let popup = click(&output, "DNG");
        let output = click(&popup, "TIFF");
        assert!(has_label(&output, "Color space"));
        let popup = click(&output, "Overwrite");
        let output = click(&popup, "Rename");
        let popup = click(&output, "Camera");
        let output = click(&popup, "ProPhoto RGB");
        let popup = click(&output, "Process + originals");
        let output = click(&popup, "Copy originals");
        assert!(!has_label(&output, "Format"));
        assert!(!has_label(&output, "Color space"));
        let popup = click(&output, "Copy originals");
        let output = click(&popup, "Process + originals");
        assert!(has_label(&output, "TIFF"));
        assert!(has_label(&output, "ProPhoto RGB"));
        click(&output, "Done");
        assert!(!open);
        assert_eq!(settings.format, ExportFormat::Tiff);
        assert_eq!(settings.tiff_color_space, Some(TiffColorSpace::ProPhotoRgb));
        assert_eq!(settings.formats(), vec![ExportFormat::Tiff]);
        assert_eq!(settings.conflict, ConflictPolicy::Rename);
    }

    #[test]
    fn conflict_defaults_to_overwrite_and_roundtrips_independently_for_each_mode() {
        let mut preferences: crate::preferences::Preferences = serde_json::from_str("{}").unwrap();
        assert_eq!(
            preferences.browse_processing.conflict,
            ConflictPolicy::Overwrite
        );
        assert_eq!(
            preferences.import_processing.conflict,
            ConflictPolicy::Overwrite
        );
        preferences.browse_processing.conflict = ConflictPolicy::Rename;
        let restored: crate::preferences::Preferences =
            serde_json::from_slice(&serde_json::to_vec(&preferences).unwrap()).unwrap();
        assert_eq!(restored.browse_processing.conflict, ConflictPolicy::Rename);
        assert_eq!(
            restored.import_processing.conflict,
            ConflictPolicy::Overwrite
        );
    }
}
