//! Composes Quattro color and monochrome controls using file-derived defaults.

use super::common;
use eframe::egui;
use foveonic::render::{ControlCapabilities, Settings};
use foveonic::source::{Metadata, ProcessingFamily};
use std::collections::BTreeMap;

pub(super) fn show(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    capture: Option<&BTreeMap<String, String>>,
) -> bool {
    let early = capture
        .filter(|_| metadata.is_none())
        .map(|properties| crate::capture::processing_metadata(properties.clone()));
    let source = metadata.or(early.as_ref());
    let capabilities = ControlCapabilities::from_family(ProcessingFamily::Quattro);
    let camera_model = capture
        .and_then(|properties| properties.get("CAMMODEL"))
        .map(String::as_str);
    if let Some(source) = source.filter(|_| capabilities.editing_available(metadata, camera_model))
    {
        let mut changed = common::rendering(ui, settings);
        let mono = settings.is_monochrome();
        let defaults = capabilities
            .editing_defaults(source, settings.rendering)
            .unwrap_or_default();
        changed |= common::presets(
            ui,
            settings,
            metadata,
            common::PresetControls {
                white_balance: capabilities.white_balance_active(settings),
                color_mode: capabilities.color_mode_active(settings),
            },
        );
        ui.separator();
        if mono {
            changed |= crate::mixer::show_with_help(
                ui,
                &mut settings.monochrome.mix,
                defaults.monochrome.mix,
                "RGB Mix",
            );
            ui.separator();
        }
        changed |= ui
            .add_enabled_ui(settings.tone_enabled, |ui| {
                common::color_sensitivity(ui, settings)
            })
            .inner;
        changed |= common::tone_with_exposure_range(ui, settings, mono, -2. ..=2.);
        ui.separator();
        common::section_header(ui, "Noise and Detail", None);
        let noise_defaults = defaults
            .reconstruction
            .unwrap_or_else(|| capabilities.default_noise_controls());
        let mut noise = settings.reconstruction.unwrap_or(noise_defaults);
        let mut noise_changed = false;
        if !mono {
            noise_changed |= common::adjustment(
                ui,
                "Chroma noise",
                &mut noise.chroma,
                0..=4,
                noise_defaults.chroma,
            );
            noise_changed |= common::adjustment(
                ui,
                "Luma noise",
                &mut noise.luminance,
                0..=4,
                noise_defaults.luminance,
            );
        } else {
            changed |= common::adjustment(
                ui,
                "Mono noise",
                &mut settings.monochrome.noise,
                0..=4,
                defaults.monochrome.noise,
            );
            noise.detail = settings
                .monochrome_smoothness
                .unwrap_or(defaults.monochrome_smoothness.unwrap_or(0.5));
        }
        let detail_default = if mono {
            defaults.monochrome_smoothness.unwrap_or(0.5)
        } else {
            noise_defaults.detail
        };
        noise_changed |= common::stepped_adjustment(
            ui,
            "Smooth / Crisp",
            &mut noise.detail,
            0. ..=1.,
            detail_default,
            1. / 6.,
        );
        if noise_changed {
            if mono {
                settings.monochrome_smoothness =
                    (noise.detail != detail_default).then_some(noise.detail);
            } else {
                settings.reconstruction = if noise == noise_defaults {
                    None
                } else {
                    Some(noise)
                };
            }
        }
        ui.separator();
        changed |= common::sharpening_with_range(ui, settings, Some(source), -2. ..=2.);
        ui.separator();
        common::section_header(ui, "Corrections", None);
        let highlight_default = defaults.highlight_amount.unwrap_or(1.);
        let mut highlight = settings.highlight_amount.unwrap_or(highlight_default);
        if common::stepped_adjustment(
            ui,
            "Highlight recovery",
            &mut highlight,
            0. ..=1.,
            highlight_default,
            0.1,
        ) {
            settings.highlight_amount = if highlight == highlight_default {
                None
            } else {
                Some(highlight)
            };
            changed = true;
        }
        return changed || noise_changed;
    } else {
        ui.label("Rendering for this camera is not available yet.");
        ui.label("Use Inspect to view the available sensor planes.");
    }
    false
}
