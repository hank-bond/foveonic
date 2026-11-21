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
    changed |= crate::multi_edit_controls::choice(
        ui,
        "/reconstruction/noise_enabled",
        &mut controls.noise_enabled,
        |ui, enabled| common::section_header(ui, EditingSection::Noise, "Noise", Some(enabled)),
    );
    ui.add_enabled_ui(controls.noise_enabled, |ui| {
        let column = LabelColumn::new(ui);
        changed |= noise_method(ui, settings, column);
        changed |= crate::multi_edit_controls::field(ui, "/reconstruction/chroma", |ui| {
            column.adjustment(
                ui,
                ("Chroma noise", "Chroma"),
                &mut controls.chroma,
                0..=4,
                2,
            )
        });
        changed |= crate::multi_edit_controls::field(ui, "/reconstruction/luminance", |ui| {
            column.adjustment(
                ui,
                ("Luma noise", "Luma"),
                &mut controls.luminance,
                0..=4,
                2,
            )
        });
        changed |= neutralize_shadows(ui, settings, column);
        changed |= speckles(ui, settings, column);
        changed |= banding(ui, settings, &mut controls.banding, rotation, column);
        crate::widgets::disclosure(
            ui,
            egui::CollapsingHeader::new("Advanced").id_salt("advanced-reconstruction"),
            |ui| {
                changed |= crate::multi_edit_controls::field(ui, "/color_dq_gain", |ui| {
                    automatic_adjustment(
                        ui,
                        "ColorDQ gain",
                        &mut settings.color_dq_gain,
                        0. ..=2.,
                        1.,
                        None,
                        column.row_controls("ColorDQ gain"),
                    )
                });
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

fn choice_menu<T: Copy + PartialEq + serde::Serialize>(
    ui: &mut egui::Ui,
    id: &str,
    help_key: &str,
    pointer: &str,
    selected: &mut T,
    choices: &[(T, &str)],
) -> bool {
    let label = choices
        .iter()
        .find(|(value, _)| value == selected)
        .unwrap()
        .1;
    crate::multi_edit_controls::choice(ui, pointer, selected, |ui, selected| {
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
    })
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
            "/merrill_noise_method",
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
        changed |= crate::multi_edit_controls::field(ui, "/neutralize_shadows/strength", |ui| {
            column.adjustment(
                ui,
                ("Neutralize shadows", "Neutralize"),
                &mut controls.strength,
                ShadowNeutralizeControls::STRENGTH_RANGE,
                defaults.strength,
            )
        });
        crate::widgets::disclosure(
            ui,
            egui::CollapsingHeader::new("Neutral Range").id_salt("shadow-neutrality-options"),
            |ui| {
                changed |=
                    crate::multi_edit_controls::field(ui, "/neutralize_shadows/shadow_ev", |ui| {
                        column.adjustment(
                            ui,
                            ("Shadow limit", "Shadow limit"),
                            &mut controls.shadow_ev,
                            ShadowNeutralizeControls::SHADOW_EV_RANGE,
                            defaults.shadow_ev,
                        )
                    });
                changed |= crate::multi_edit_controls::field(
                    ui,
                    "/neutralize_shadows/color_tolerance",
                    |ui| {
                        column.adjustment(
                            ui,
                            ("Color tolerance", "Color tolerance"),
                            &mut controls.color_tolerance,
                            ShadowNeutralizeControls::COLOR_TOLERANCE_RANGE,
                            defaults.color_tolerance,
                        )
                    },
                );
                changed |=
                    crate::multi_edit_controls::field(ui, "/neutralize_shadows/hue_width", |ui| {
                        column.adjustment(
                            ui,
                            ("Hue width", "Hue width"),
                            &mut controls.hue_width,
                            ShadowNeutralizeControls::HUE_WIDTH_RANGE,
                            defaults.hue_width,
                        )
                    });
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
                    "/merrill_speckles/method",
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
            changed |= crate::multi_edit_controls::field(ui, "/merrill_speckles/threshold", |ui| {
                column.adjustment(
                    ui,
                    ("Speckle threshold", "Threshold"),
                    &mut settings.merrill_speckles.threshold,
                    threshold_range,
                    defaults.threshold,
                )
            });
            if settings.merrill_speckles.method == MerrillSpeckleMethod::Compact {
                ui.horizontal(|ui| {
                    help::control(column.label(ui, "Size"), "Speckle size");
                    changed |= choice_menu(
                        ui,
                        "speckle-size",
                        "Speckle size",
                        "/merrill_speckles/size",
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
    let reference_pointer = if settings.is_monochrome() {
        "/monochrome/banding"
    } else {
        "/reconstruction/banding"
    };
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
                    "/merrill_banding/method",
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
                    changed |= crate::multi_edit_controls::field(ui, reference_pointer, |ui| {
                        column.adjustment(
                            ui,
                            (help_key, "Column strength"),
                            reference_amount,
                            0..=4,
                            2,
                        )
                    });
                    changed |= reference_row_offsets(ui, settings);
                }
                MerrillBandingMethod::Directional => {
                    let physical = crate::multi_edit_controls::active(ui.ctx());
                    let (horizontal, vertical) = if physical || rotation.is_multiple_of(180) {
                        (
                            (&mut settings.merrill_banding.rows, "/merrill_banding/rows"),
                            (
                                &mut settings.merrill_banding.columns,
                                "/merrill_banding/columns",
                            ),
                        )
                    } else {
                        (
                            (
                                &mut settings.merrill_banding.columns,
                                "/merrill_banding/columns",
                            ),
                            (&mut settings.merrill_banding.rows, "/merrill_banding/rows"),
                        )
                    };
                    let labels = if physical {
                        ["Rows", "Columns"]
                    } else {
                        ["Horizontal bands", "Vertical bands"]
                    };
                    for (label, (axis, pointer)) in labels.into_iter().zip([horizontal, vertical]) {
                        crate::widgets::disclosure(ui, egui::CollapsingHeader::new(label), |ui| {
                            changed |= band_axis(ui, axis, column, pointer);
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

fn band_axis(
    ui: &mut egui::Ui,
    axis: &mut BandingAxisControls,
    column: LabelColumn,
    pointer: &str,
) -> bool {
    let mut changed = false;
    for (index, label) in BandingAxisControls::LABELS.into_iter().enumerate() {
        let range = axis.size_range(index);
        let default = BandingAxisControls::DEFAULT_BANDS[index];
        ui.push_id(index, |ui| {
            crate::widgets::parameter_subgroup(ui, |ui| {
                changed |= crate::multi_edit_controls::field(
                    ui,
                    format!("{pointer}/bands/{index}/strength"),
                    |ui| {
                        column.adjustment(
                            ui,
                            ("Band strength", label),
                            &mut axis.bands[index].strength,
                            BandingBand::STRENGTH_RANGE,
                            default.strength,
                        )
                    },
                );
                changed |= crate::multi_edit_controls::field(
                    ui,
                    format!("{pointer}/bands/{index}/size"),
                    |ui| {
                        adjustment_row(
                            ui,
                            "Band size",
                            &mut axis.bands[index].size,
                            range.clone(),
                            default.size.clamp(*range.start(), *range.end()),
                            RowControls {
                                step: Some(2.),
                                step_origin: 1.,
                                ..column.row_controls("Size")
                            },
                        )
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
    changed |= crate::multi_edit_controls::field(ui, "/monochrome/noise", |ui| {
        column.adjustment(
            ui,
            ("Mono noise", "Mono noise"),
            &mut settings.monochrome.noise,
            0..=4,
            2,
        )
    });
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
    fn mixed_chroma_drag_records_only_its_property_and_clamps_each_photo() {
        fn draw(
            ctx: &egui::Context,
            active: &mut Settings,
            targets: &[Settings],
            time: f64,
            events: Vec<egui::Event>,
        ) -> (egui::FullOutput, Vec<crate::multi_edit_controls::Edit>) {
            crate::multi_edit_controls::begin_with_numeric(ctx, targets.to_vec(), targets.to_vec());
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 1800.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        color(ui, active, 0);
                    });
                },
            );
            (output, crate::multi_edit_controls::finish(ctx))
        }
        let ctx = egui::Context::default();
        crate::widgets::install_style(&ctx);
        let mut a = Settings::new("Sunlight", "Standard");
        a.reconstruction = Some(foveonic::noise::Controls {
            chroma: 1,
            ..Default::default()
        });
        let mut b = a.clone();
        b.reconstruction.as_mut().unwrap().chroma = 3;
        b.reconstruction.as_mut().unwrap().luminance = 0;
        b.reconstruction.as_mut().unwrap().texture.fine = 2.3;
        let mut active = a.clone();
        let (initial, edits) = draw(&ctx, &mut active, &[a.clone(), b.clone()], 0., vec![]);
        assert!(edits.is_empty());
        let track = initial
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill == crate::widgets::slider_track_fill(&ctx.style().visuals)
                        && rect.rect.width() > 40.
                        && rect.rect.height() < 12. =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("chroma slider track");
        let start = track.center();
        let end = start + egui::vec2(track.width() * 0.3, 0.);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let mut recorded = Vec::new();
        for (time, events) in [
            (
                0.1,
                vec![egui::Event::PointerMoved(start), button(start, true)],
            ),
            (0.2, vec![egui::Event::PointerMoved(end)]),
            (0.3, vec![button(end, false)]),
        ] {
            let (_, edits) = draw(&ctx, &mut active, &[a.clone(), b.clone()], time, events);
            for edit in &edits {
                assert_eq!(edit.pointer(), "/reconstruction/chroma");
            }
            crate::multi_edit_controls::apply(&mut a, &edits);
            crate::multi_edit_controls::apply(&mut b, &edits);
            active = a.clone();
            recorded.extend(edits);
        }
        assert!(
            !recorded.is_empty(),
            "drag must emit an explicit property edit"
        );
        assert!(a.reconstruction.unwrap().chroma > 1);
        assert_eq!(b.reconstruction.unwrap().chroma, 4);
        assert_eq!(b.reconstruction.unwrap().luminance, 0);
        assert_eq!(b.reconstruction.unwrap().texture.fine, 2.3);
    }

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
