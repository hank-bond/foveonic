//! Composes Merrill denoising, speckle, shadow-neutrality and independent banding controls.

use super::super::common::{self, adjustment_row, automatic_adjustment, LabelColumn, RowControls};
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
        let column = LabelColumn::new(ui);
        changed |= noise_method(ui, settings, column);
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
        changed |= neutralize_shadows(ui, settings, column);
        changed |= speckles(ui, settings, column);
        changed |= banding(ui, settings, &mut controls.banding, rotation, column);
        crate::widgets::disclosure(
            ui,
            egui::CollapsingHeader::new("Advanced").id_salt("advanced-reconstruction"),
            |ui| {
                changed |= automatic_adjustment(
                    ui,
                    "ColorDQ gain",
                    &mut settings.color_dq_gain,
                    0. ..=2.,
                    1.,
                    None,
                    column.row_controls("ColorDQ gain"),
                );
            },
        );
    });
    // Keep both groups' inactive values. Loading an older explicit off
    // recipe must not create settings merely by drawing the panel.
    if before.is_some() || (controls.noise_enabled || controls.detail_enabled) {
        settings.reconstruction = Some(controls);
    }
    changed
}

fn choice_menu<T: Copy + PartialEq>(
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
        .width(ui.available_width())
        .truncate()
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

fn noise_method(ui: &mut egui::Ui, settings: &mut Settings, column: LabelColumn) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        help::control(column.label(ui, "Method"), "Merrill noise method");
        let methods = [
            (MerrillNoiseMethod::Reference, "Reference"),
            (MerrillNoiseMethod::MultiscaleRows, "Multiscale: Rows"),
            (
                MerrillNoiseMethod::MultiscaleRowsAndColumns,
                "Multiscale: Rows + Columns",
            ),
        ];
        changed |= choice_menu(
            ui,
            "merrill-noise-method",
            "Merrill noise method",
            &mut settings.merrill_noise_method,
            &methods,
        );
    });
    changed
}

fn neutralize_shadows(ui: &mut egui::Ui, settings: &mut Settings, column: LabelColumn) -> bool {
    let controls = &mut settings.neutralize_shadows;
    let defaults = ShadowNeutralizeControls::default();
    let mut changed = false;
    crate::widgets::parameter_group(ui, |ui| {
        changed |= column.adjustment(
            ui,
            ("Neutralize shadows", "Neutralize"),
            &mut controls.strength,
            ShadowNeutralizeControls::STRENGTH_RANGE,
            defaults.strength,
        );
        crate::widgets::disclosure(
            ui,
            egui::CollapsingHeader::new("Neutral Range").id_salt("shadow-neutrality-options"),
            |ui| {
                changed |= column.adjustment(
                    ui,
                    ("Shadow limit", "Shadow limit"),
                    &mut controls.shadow_ev,
                    ShadowNeutralizeControls::SHADOW_EV_RANGE,
                    defaults.shadow_ev,
                );
                changed |= column.adjustment(
                    ui,
                    ("Color tolerance", "Color tolerance"),
                    &mut controls.color_tolerance,
                    ShadowNeutralizeControls::COLOR_TOLERANCE_RANGE,
                    defaults.color_tolerance,
                );
                changed |= column.adjustment(
                    ui,
                    ("Hue width", "Hue width"),
                    &mut controls.hue_width,
                    ShadowNeutralizeControls::HUE_WIDTH_RANGE,
                    defaults.hue_width,
                );
            },
        );
    });
    changed
}

fn speckles(ui: &mut egui::Ui, settings: &mut Settings, column: LabelColumn) -> bool {
    use foveonic::render::MerrillSpeckleMethod;
    let defaults = MerrillSpeckleControls::default();
    let mut changed = false;
    crate::widgets::parameter_group(ui, |ui| {
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
                changed |= choice_menu(
                    ui,
                    "speckle-method",
                    "Speckle method",
                    &mut settings.merrill_speckles.method,
                    &methods,
                );
            });
        });
        ui.add_enabled_ui(settings.despeckle, |ui| {
            let threshold_range =
                if settings.merrill_speckles.method == MerrillSpeckleMethod::Compact {
                    0.2..=*MerrillSpeckleControls::THRESHOLD_RANGE.end()
                } else {
                    MerrillSpeckleControls::THRESHOLD_RANGE
                };
            changed |= column.adjustment(
                ui,
                ("Speckle threshold", "Threshold"),
                &mut settings.merrill_speckles.threshold,
                threshold_range,
                defaults.threshold,
            );
            if settings.merrill_speckles.method == MerrillSpeckleMethod::Compact {
                ui.horizontal(|ui| {
                    help::control(column.label(ui, "Size"), "Speckle size");
                    changed |= choice_menu(
                        ui,
                        "speckle-size",
                        "Speckle size",
                        &mut settings.merrill_speckles.size,
                        &[(3, "3"), (5, "5"), (7, "7")],
                    );
                });
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
    column: LabelColumn,
) -> bool {
    let mut changed = false;
    crate::widgets::parameter_group(ui, |ui| {
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
                changed |= choice_menu(
                    ui,
                    "banding-method",
                    "Banding method",
                    &mut settings.merrill_banding.method,
                    &methods,
                );
            });
        });
        ui.add_enabled_ui(settings.merrill_banding.enabled, |ui| {
            match settings.merrill_banding.method {
                MerrillBandingMethod::Reference => {
                    let help_key = if settings.is_monochrome() {
                        "Mono banding"
                    } else {
                        "Banding noise"
                    };
                    changed |= column.adjustment(
                        ui,
                        (help_key, "Column strength"),
                        reference_amount,
                        0..=4,
                        2,
                    );
                    changed |= reference_row_offsets(ui, settings);
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
                        crate::widgets::disclosure(ui, egui::CollapsingHeader::new(label), |ui| {
                            changed |= band_axis(ui, axis, column);
                        });
                    }
                }
            }
        });
    });
    changed
}

fn reference_row_offsets(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    help::control(
        crate::widgets::checkbox(ui, &mut settings.row_correction, "Correct row offsets"),
        "Correct row offsets",
    )
    .changed()
}

fn band_axis(ui: &mut egui::Ui, axis: &mut BandingAxisControls, column: LabelColumn) -> bool {
    let mut changed = false;
    for (index, label) in BandingAxisControls::LABELS.into_iter().enumerate() {
        let range = axis.size_range(index);
        let default = BandingAxisControls::DEFAULT_BANDS[index];
        ui.push_id(index, |ui| {
            crate::widgets::parameter_subgroup(ui, |ui| {
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
    let column = LabelColumn::new(ui);
    let mut changed = noise_method(ui, settings, column);
    changed |= column.adjustment(
        ui,
        ("Mono noise", "Mono noise"),
        &mut settings.monochrome.noise,
        0..=4,
        2,
    );
    changed |= speckles(ui, settings, column);
    let mut amount = settings.monochrome.banding;
    changed |= banding(ui, settings, &mut amount, rotation, column);
    settings.monochrome.banding = amount;
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_grid_stays_aligned_through_groups_and_disclosures() {
        for em in [13., 18.] {
            for method in [
                MerrillBandingMethod::Reference,
                MerrillBandingMethod::Directional,
            ] {
                let ctx = egui::Context::default();
                crate::widgets::install_style(&ctx);
                ctx.style_mut(|s| {
                    s.text_styles
                        .insert(egui::TextStyle::Body, egui::FontId::proportional(em));
                });
                ctx.memory_mut(|m| m.set_everything_is_visible(true));
                let mut settings = Settings {
                    reconstruction: Some(Default::default()),
                    despeckle: true,
                    ..Default::default()
                };
                settings.merrill_speckles.method =
                    foveonic::render::MerrillSpeckleMethod::Reference;
                settings.merrill_banding.method = method;
                let before = settings.clone();
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 2000.),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            assert!(!color(ui, &mut settings, 0));
                        });
                    },
                );
                assert_eq!(settings, before);
                let tracks: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Rect(r)
                            if r.fill
                                == crate::widgets::slider_track_fill(&ctx.style().visuals)
                                && r.rect.width() > 40.
                                && r.rect.height() < 12. =>
                        {
                            Some(r.rect)
                        }
                        _ => None,
                    })
                    .collect();
                assert!(tracks.len() >= 7, "missing tracks: {tracks:?}");
                let left = tracks[0].left();
                for track in &tracks {
                    assert!(
                        (track.left() - left).abs() < 0.1,
                        "{method:?}, {em}: tracks lost the shared column: {tracks:?}"
                    );
                }
                // Only the final ColorDQ row needs the additional Auto button.
                for track in &tracks[..tracks.len() - 1] {
                    assert!(
                        (track.right() - tracks[0].right()).abs() < 0.1,
                        "integer and decimal rows must share track lengths: {tracks:?}"
                    );
                }
                let references: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Text(t) if t.galley.job.text == "Reference" => Some(t.pos.x),
                        _ => None,
                    })
                    .collect();
                assert!(references.len() >= 2);
                assert!(references.iter().all(|x| (*x - references[0]).abs() < 0.1));
                assert!(references[0] >= left && references[0] < left + em);
                let title = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Text(t) if t.galley.job.text == "Noise" => Some(t),
                        _ => None,
                    })
                    .unwrap();
                let format = &title.galley.job.sections[0].format;
                assert!(format.font_id.size > em);
                assert_eq!(format.color, egui::Color32::from_gray(225));
            }
        }
    }
}
