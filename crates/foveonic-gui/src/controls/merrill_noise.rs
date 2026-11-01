//! Composes Merrill denoising, speckle, shadow-neutrality and independent banding controls.

use super::super::common::{self, adjustment, adjustment_row, automatic_adjustment, RowControls};
use crate::{help, preferences::EditingSection};
use eframe::egui;
use foveonic::render::{
    BandingAxisControls, BandingBand, MerrillBandingMethod, MerrillNoiseMethod,
    MerrillSpeckleControls, Settings, ShadowNeutralizeControls,
};

pub(super) fn color(ui: &mut egui::Ui, settings: &mut Settings, rotation: u32) -> bool {
    let mut changed = false;
    let before = settings.reconstruction;
    let mut controls = before.unwrap_or(foveonic::noise::Controls {
        noise_enabled: false,
        detail_enabled: false,
        ..Default::default()
    });
    changed |= common::section_header(
        ui,
        EditingSection::Noise,
        "Noise",
        Some(&mut controls.noise_enabled),
    );
    ui.add_enabled_ui(controls.noise_enabled, |ui| {
        changed |= noise_method(ui, settings);
        let column = common::LabelColumn::new(ui, &["Chroma", "Luma"], &[]);
        changed |= column.adjustment(
            ui,
            ("Chroma noise", "Chroma"),
            &mut controls.chroma,
            0..=4,
            2,
        );
        changed |= column.adjustment(
            ui,
            ("Luma noise", "Luma"),
            &mut controls.luminance,
            0..=4,
            2,
        );
        changed |= neutralize_shadows(ui, settings);
        changed |= speckles(ui, settings);
        changed |= banding(ui, settings, &mut controls.banding, rotation);
        egui::CollapsingHeader::new("Advanced")
            .id_salt("advanced-reconstruction")
            .show(ui, |ui| {
                changed |= automatic_adjustment(
                    ui,
                    "ColorDQ gain",
                    &mut settings.color_dq_gain,
                    0. ..=2.,
                    1.,
                    None,
                    RowControls::default(),
                );
            });
    });
    // Keep both groups' inactive values. Loading an older explicit off
    // recipe must not create settings merely by drawing the panel.
    if before.is_some() || (controls.noise_enabled || controls.detail_enabled) {
        settings.reconstruction = Some(controls);
    }
    changed
}

fn method_menu<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: &str,
    help_key: &str,
    selected: &mut T,
    choices: &[(T, &str)],
) -> bool {
    let label = choices
        .iter()
        .find(|(value, _)| value == selected)
        .unwrap()
        .1;
    let combo = egui::ComboBox::from_id_salt(id)
        .width((ui.available_width() - 8.).max(80.))
        .selected_text(label);
    let mut changed = false;
    let response = crate::widgets::dropdown(ui, combo, |ui| {
        for &(value, label) in choices {
            changed |= crate::widgets::menu_value(ui, selected, value, label).changed();
        }
    });
    help::control(response.response, help_key);
    changed
}

fn noise_method(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        help::control(ui.label("Method"), "Merrill noise method");
        let methods = [
            (MerrillNoiseMethod::Reference, "Reference"),
            (MerrillNoiseMethod::MultiscaleRows, "Multiscale: Rows"),
            (
                MerrillNoiseMethod::MultiscaleRowsAndColumns,
                "Multiscale: Rows + Columns",
            ),
        ];
        changed |= method_menu(
            ui,
            "merrill-noise-method",
            "Merrill noise method",
            &mut settings.merrill_noise_method,
            &methods,
        );
    });
    changed
}

fn neutralize_shadows(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let controls = &mut settings.neutralize_shadows;
    let defaults = ShadowNeutralizeControls::default();
    let mut changed = adjustment(
        ui,
        "Neutralize shadows",
        &mut controls.strength,
        ShadowNeutralizeControls::STRENGTH_RANGE,
        defaults.strength,
    );
    egui::CollapsingHeader::new("Shadow selectivity")
        .id_salt("shadow-neutrality-options")
        .show(ui, |ui| {
            changed |= adjustment(
                ui,
                "Shadow limit",
                &mut controls.shadow_ev,
                ShadowNeutralizeControls::SHADOW_EV_RANGE,
                defaults.shadow_ev,
            );
            changed |= adjustment(
                ui,
                "Color tolerance",
                &mut controls.color_tolerance,
                ShadowNeutralizeControls::COLOR_TOLERANCE_RANGE,
                defaults.color_tolerance,
            );
            changed |= adjustment(
                ui,
                "Hue width",
                &mut controls.hue_width,
                ShadowNeutralizeControls::HUE_WIDTH_RANGE,
                defaults.hue_width,
            );
        });
    changed
}

fn speckles(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    use foveonic::render::MerrillSpeckleMethod;
    let defaults = MerrillSpeckleControls::default();
    let mut changed = false;
    let column = common::LabelColumn::new(ui, &["Speckles"], &[]);
    ui.horizontal(|ui| {
        changed |= help::control(
            column.toggle(ui, "Speckles", &mut settings.despeckle),
            "Speckles",
        )
        .changed();
        ui.add_enabled_ui(settings.despeckle, |ui| {
            let methods = [
                (MerrillSpeckleMethod::Reference, "Reference"),
                (MerrillSpeckleMethod::Compact, "Compact"),
            ];
            changed |= method_menu(
                ui,
                "speckle-method",
                "Speckle method",
                &mut settings.merrill_speckles.method,
                &methods,
            );
        });
    });
    ui.add_enabled_ui(settings.despeckle, |ui| {
        ui.indent("speckle-parameters", |ui| {
            changed |= adjustment(
                ui,
                "Speckle threshold",
                &mut settings.merrill_speckles.threshold,
                MerrillSpeckleControls::THRESHOLD_RANGE,
                defaults.threshold,
            );
            if settings.merrill_speckles.method == MerrillSpeckleMethod::Compact {
                changed |= adjustment_row(
                    ui,
                    "Speckle size",
                    &mut settings.merrill_speckles.size,
                    MerrillSpeckleControls::SIZE_RANGE,
                    defaults.size,
                    2.,
                    RowControls {
                        step: Some(2.),
                        step_origin: 1.,
                        ..Default::default()
                    },
                );
            }
        });
    });
    changed
}

fn banding(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    reference_amount: &mut u32,
    rotation: u32,
) -> bool {
    let mut changed = false;
    let column = common::LabelColumn::new(ui, &["Banding"], &["Strength"]);
    ui.horizontal(|ui| {
        changed |= help::control(
            column.toggle(ui, "Banding", &mut settings.merrill_banding.enabled),
            "Banding enabled",
        )
        .changed();
        ui.add_enabled_ui(settings.merrill_banding.enabled, |ui| {
            let methods = [
                (MerrillBandingMethod::Reference, "Reference"),
                (MerrillBandingMethod::Directional, "Directional"),
            ];
            changed |= method_menu(
                ui,
                "banding-method",
                "Banding method",
                &mut settings.merrill_banding.method,
                &methods,
            );
        });
    });
    ui.add_enabled_ui(settings.merrill_banding.enabled, |ui| {
        ui.indent("banding-parameters", |ui| {
            match settings.merrill_banding.method {
                MerrillBandingMethod::Reference => {
                    let help_key = if settings.is_monochrome() {
                        "Mono banding"
                    } else {
                        "Banding noise"
                    };
                    changed |=
                        column.adjustment(ui, (help_key, "Strength"), reference_amount, 0..=4, 2);
                    changed |= row_banding(ui, settings);
                }
                MerrillBandingMethod::Directional => {
                    let (horizontal, vertical) = if rotation.is_multiple_of(180) {
                        (
                            &mut settings.merrill_banding.rows,
                            &mut settings.merrill_banding.columns,
                        )
                    } else {
                        (
                            &mut settings.merrill_banding.columns,
                            &mut settings.merrill_banding.rows,
                        )
                    };
                    for (label, axis) in [
                        ("Horizontal bands", horizontal),
                        ("Vertical bands", vertical),
                    ] {
                        egui::CollapsingHeader::new(label).show(ui, |ui| {
                            changed |= band_axis(ui, axis);
                        });
                    }
                }
            }
        });
    });
    changed
}

fn row_banding(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    help::control(
        crate::widgets::checkbox(ui, &mut settings.row_correction, "Row banding"),
        "Row banding",
    )
    .changed()
}

fn band_axis(ui: &mut egui::Ui, axis: &mut BandingAxisControls) -> bool {
    let mut changed = false;
    let column = common::LabelColumn::new(ui, &BandingAxisControls::LABELS, &["Size"])
        .with_outer_padding(f32::from(
            crate::widgets::PARAMETER_GROUP_HORIZONTAL_PADDING,
        ));
    for (index, label) in BandingAxisControls::LABELS.into_iter().enumerate() {
        let range = axis.size_range(index);
        let default = BandingAxisControls::DEFAULT_BANDS[index];
        ui.push_id(index, |ui| {
            crate::widgets::parameter_group(ui, |ui| {
                changed |= column.adjustment(
                    ui,
                    ("Band strength", label),
                    &mut axis.bands[index].strength,
                    BandingBand::STRENGTH_RANGE,
                    default.strength,
                );
                changed |= adjustment_row(
                    ui,
                    "Band size",
                    &mut axis.bands[index].size,
                    range.clone(),
                    default.size.clamp(*range.start(), *range.end()),
                    2.,
                    RowControls {
                        step: Some(2.),
                        step_origin: 1.,
                        ..column.row_controls("Size")
                    },
                );
            });
        });
    }
    changed
}

pub(super) fn monochrome(ui: &mut egui::Ui, settings: &mut Settings, rotation: u32) -> bool {
    common::section_header(ui, EditingSection::Noise, "Noise", None);
    let mut changed = noise_method(ui, settings);
    changed |= adjustment(ui, "Mono noise", &mut settings.monochrome.noise, 0..=4, 2);
    changed |= speckles(ui, settings);
    let mut amount = settings.monochrome.banding;
    changed |= banding(ui, settings, &mut amount, rotation);
    settings.monochrome.banding = amount;
    changed
}
