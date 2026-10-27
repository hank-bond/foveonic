//! Composes Quattro color and monochrome controls using file-derived defaults.

use super::common;
use crate::preferences::EditingSection;
use eframe::egui;
use foveonic::render::{ControlCapabilities, Settings};
use foveonic::source::{Metadata, ProcessingFamily};
use std::collections::BTreeMap;

pub(super) fn show(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    capture: Option<&BTreeMap<String, String>>,
    order: &mut Vec<EditingSection>,
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
        let available = if mono {
            &[
                EditingSection::Mix,
                EditingSection::Tone,
                EditingSection::ColorEditor,
                EditingSection::Noise,
                EditingSection::Sharpening,
                EditingSection::Corrections,
            ][..]
        } else {
            &[
                EditingSection::Tone,
                EditingSection::ColorEditor,
                EditingSection::Noise,
                EditingSection::Sharpening,
                EditingSection::Corrections,
            ][..]
        };
        changed |= super::sections::show(ui, order, available, |ui, section| match section {
            EditingSection::Mix => crate::mixer::show_with_help(
                ui,
                &mut settings.monochrome.mix,
                defaults.monochrome.mix,
                "RGB Mix",
            ),
            EditingSection::Tone => common::tone_with_exposure_range(ui, settings, mono, -2. ..=2.),
            EditingSection::ColorEditor => super::color_editor::show(ui, settings),
            EditingSection::Noise => noise_and_detail(ui, settings, &defaults, capabilities),
            EditingSection::Sharpening => {
                common::sharpening_with_range(ui, settings, Some(source), -2. ..=2.)
            }
            EditingSection::Corrections => corrections(ui, settings, &defaults),
            _ => false,
        });
        return changed;
    } else {
        ui.label("Rendering for this camera is not available yet.");
        ui.label("Use Inspect to view the available sensor planes.");
    }
    false
}

fn noise_and_detail(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    defaults: &Settings,
    capabilities: ControlCapabilities,
) -> bool {
    common::section_header(ui, EditingSection::Noise, "Noise and Detail", None);
    let mono = settings.is_monochrome();
    let mut changed = false;
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
    changed || noise_changed
}

fn corrections(ui: &mut egui::Ui, settings: &mut Settings, defaults: &Settings) -> bool {
    let mut changed = false;
    common::section_header(ui, EditingSection::Corrections, "Corrections", None);
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
    changed
}
