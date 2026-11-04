//! Composes independent Reference and Universal saturation with visible tonal amounts.

use super::common;
use crate::preferences::EditingSection;
use eframe::egui;
use foveonic::render::{PhotographicMethod, Settings};

pub(super) fn show(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut changed = common::section_header(
        ui,
        EditingSection::Saturation,
        "Saturation",
        Some(&mut settings.saturation_enabled),
    );
    ui.add_enabled_ui(
        settings.saturation_enabled && !settings.is_monochrome(),
        |ui| {
            let column = common::LabelColumn::new(ui);
            changed |= column.method_choice(
                ui,
                "Saturation method",
                "Method",
                &mut settings.photographic.saturation_method,
            );
            if settings.photographic.saturation_method == PhotographicMethod::Reference {
                changed |= column.adjustment(
                    ui,
                    ("Reference saturation", "Amount"),
                    &mut settings.saturation,
                    0. ..=2.,
                    1.,
                );
            } else {
                let c = &mut settings.photographic.perceptual;
                changed |= column.saturation_adjustment(
                    ui,
                    ("Perceptual saturation", "Amount"),
                    &mut c.saturation,
                );
                changed |= column.adjustment(
                    ui,
                    ("Saturation contrast", "Contrast"),
                    &mut c.contrast,
                    -1. ..=1.,
                    0.,
                );
                crate::widgets::parameter_group(ui, |ui| {
                    for (index, label) in ["Shadows", "Midtones", "Highlights"]
                        .into_iter()
                        .enumerate()
                    {
                        changed |= column.saturation_adjustment(
                            ui,
                            ("Perceptual ranges", label),
                            &mut c.bands[index].saturation,
                        );
                    }
                });
            }
        },
    );
    changed
}
