//! Composes independent Reference and Universal saturation with visible tonal amounts.

use super::common;
use crate::preferences::EditingSection;
use eframe::egui;
use foveonic::render::{PhotographicMethod, Settings};

/// Reference saturation's amount, which multiplies chroma.
pub(crate) const REFERENCE_SATURATION_RANGE: std::ops::RangeInclusive<f32> = 0. ..=2.;

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
                changed |= column.parameter(
                    ui,
                    "/saturation",
                    ("Reference saturation", "Amount"),
                    &mut settings.saturation,
                    REFERENCE_SATURATION_RANGE,
                    1.,
                );
            } else {
                let c = &mut settings.photographic.perceptual;
                changed |= column.saturation_parameter(
                    ui,
                    "/photographic/perceptual/saturation",
                    ("Perceptual saturation", "Amount"),
                    &mut c.saturation,
                );
                changed |= column.parameter(
                    ui,
                    "/photographic/perceptual/contrast",
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
                        changed |= column.saturation_parameter(
                            ui,
                            format!("/photographic/perceptual/bands/{index}/saturation"),
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
