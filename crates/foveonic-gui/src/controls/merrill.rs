//! Composes Merrill color, monochrome, reconstruction and correction controls.

#[path = "merrill_noise.rs"]
mod noise;

use super::common::{self, adjustment_row, automatic_adjustment};
use crate::{help, preferences::EditingSection};
use eframe::egui;
use foveonic::{
    render::{ColorFieldApproach, HighlightMode, Settings},
    source::Metadata,
};

pub(super) const HIGHLIGHT_METHODS: [((HighlightMode, ColorFieldApproach), &str); 3] = [
    (
        (HighlightMode::Classic, ColorFieldApproach::Faithful),
        "Classic",
    ),
    (
        (HighlightMode::ColorField, ColorFieldApproach::LocalGradient),
        "Color Field: Local",
    ),
    (
        (HighlightMode::ColorField, ColorFieldApproach::Faithful),
        "Color Field: Global",
    ),
];

pub(super) fn highlight_choice(settings: &Settings) -> (HighlightMode, ColorFieldApproach) {
    (
        settings.highlight_mode,
        if settings.highlight_mode == HighlightMode::Classic {
            ColorFieldApproach::Faithful
        } else {
            settings.color_field.approach
        },
    )
}

pub(super) fn select_highlight(
    settings: &mut Settings,
    choice: (HighlightMode, ColorFieldApproach),
) -> bool {
    let before = (settings.highlight_mode, settings.color_field);
    settings.highlight_mode = choice.0;
    if choice.0 == HighlightMode::ColorField {
        settings.color_field.approach = choice.1;
    }
    before != (settings.highlight_mode, settings.color_field)
}

fn detail(
    ui: &mut egui::Ui,
    pointer: &str,
    enabled: &mut bool,
    texture: &mut foveonic::noise::TextureControls,
) -> bool {
    let mut changed = false;
    changed |= crate::multi_edit_controls::choice(
        ui,
        format!("{pointer}/detail_enabled"),
        enabled,
        |ui, enabled| common::section_header(ui, EditingSection::Detail, "Detail", Some(enabled)),
    );
    ui.add_enabled_ui(*enabled, |ui| {
        let column = common::LabelColumn::new(ui);
        crate::widgets::parameter_group(ui, |ui| {
            changed |=
                crate::multi_edit_controls::field(ui, format!("{pointer}/texture/fine"), |ui| {
                    column.adjustment(
                        ui,
                        ("Fine detail", "Fine"),
                        &mut texture.fine,
                        foveonic::noise::TextureControls::AMOUNT_RANGE,
                        1.,
                    )
                });
            changed |= crate::multi_edit_controls::field(
                ui,
                format!("{pointer}/texture/fine_size"),
                |ui| {
                    column.adjustment(
                        ui,
                        ("Fine size", "Size"),
                        &mut texture.fine_size,
                        foveonic::noise::TextureControls::SIZE_RANGE,
                        1.,
                    )
                },
            );
        });
        crate::widgets::parameter_group(ui, |ui| {
            changed |=
                crate::multi_edit_controls::field(ui, format!("{pointer}/texture/coarse"), |ui| {
                    column.adjustment(
                        ui,
                        ("Coarse detail", "Coarse"),
                        &mut texture.coarse,
                        foveonic::noise::TextureControls::AMOUNT_RANGE,
                        1.,
                    )
                });
            changed |= crate::multi_edit_controls::field(
                ui,
                format!("{pointer}/texture/coarse_size"),
                |ui| {
                    column.adjustment(
                        ui,
                        ("Coarse size", "Size"),
                        &mut texture.coarse_size,
                        foveonic::noise::TextureControls::SIZE_RANGE,
                        1.,
                    )
                },
            );
        });
    });
    changed
}

pub(super) fn show(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    exposure: Option<foveonic::source::CaptureExposureCompensation>,
    rotation: u32,
    order: &mut Vec<EditingSection>,
    menus: &mut crate::editor::Menus<'_>,
) -> bool {
    let mut changed = common::rendering(ui, settings);
    let mono = settings.is_monochrome();
    changed |= common::color_mode(ui, settings, metadata, !mono);
    let available = if mono {
        EditingSection::ALL.as_slice()
    } else {
        &[
            EditingSection::WhiteBalance,
            EditingSection::Tone,
            EditingSection::Saturation,
            EditingSection::ColorEditor,
            EditingSection::Noise,
            EditingSection::Detail,
            EditingSection::Sharpening,
            EditingSection::Corrections,
        ]
    };
    changed |= super::sections::show(ui, order, available, menus, |ui, section| match section {
        EditingSection::WhiteBalance => common::white_balance(
            ui,
            settings,
            metadata,
            foveonic::render::ControlCapabilities::from_family(
                foveonic::source::ProcessingFamily::Merrill,
            )
            .white_balance_active(settings),
        ),
        EditingSection::Mix => crate::mixer::show(ui, &mut settings.monochrome.mix),
        EditingSection::Tone => common::tone(ui, settings, exposure),
        EditingSection::Saturation => super::saturation::show(ui, settings),
        EditingSection::ColorEditor => super::color_editor::show(ui, settings),
        EditingSection::Noise => {
            if mono {
                noise::monochrome(ui, settings, rotation)
            } else {
                noise::color(ui, settings, rotation)
            }
        }
        EditingSection::Detail => {
            if mono {
                detail(
                    ui,
                    "/monochrome",
                    &mut settings.monochrome.detail_enabled,
                    &mut settings.monochrome.texture,
                )
            } else {
                color_detail(ui, settings)
            }
        }
        EditingSection::Sharpening => common::sharpening(ui, settings, metadata),
        EditingSection::Corrections => corrections(ui, settings),
    });
    changed
}

fn color_detail(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let before = settings.reconstruction;
    let mut controls = before.unwrap_or(foveonic::noise::Controls {
        noise_enabled: false,
        detail_enabled: false,
        ..Default::default()
    });
    let changed = detail(
        ui,
        "/reconstruction",
        &mut controls.detail_enabled,
        &mut controls.texture,
    );
    if before.is_some() || controls.noise_enabled || controls.detail_enabled {
        settings.reconstruction = Some(controls);
    }
    changed
}

fn corrections(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut changed = common::section_header(
        ui,
        EditingSection::Corrections,
        "Corrections",
        Some(&mut settings.corrections_enabled),
    );
    ui.add_enabled_ui(settings.corrections_enabled, |ui| {
        changed |= super::dehaze::show(ui, settings);
        crate::widgets::parameter_group(ui, |ui| {
            let column = common::LabelColumn::new(ui);
            ui.horizontal(|ui| {
                changed |= help::control(
                    column.toggle(ui, "Highlights", &mut settings.highlight_restoration),
                    "Highlight correction",
                )
                .changed();
                ui.add_enabled_ui(settings.highlight_restoration, |ui| {
                    let mut choice = highlight_choice(settings);
                    let label = HIGHLIGHT_METHODS
                        .iter()
                        .find(|(value, _)| *value == choice)
                        .unwrap()
                        .1;
                    let combo = egui::ComboBox::from_id_salt("highlight-method")
                        .width(ui.available_width())
                        .truncate()
                        .selected_text(label);
                    let properties: &[&str] = if choice.0 == HighlightMode::Classic
                        && !crate::multi_edit_controls::different(ui.ctx(), "/highlight_mode")
                    {
                        &["/highlight_mode"]
                    } else {
                        &["/highlight_mode", "/color_field/approach"]
                    };
                    let response = crate::multi_edit_controls::compound(ui, properties, |ui| {
                        crate::widgets::dropdown(ui, combo, |ui| {
                            for (value, label) in HIGHLIGHT_METHODS {
                                if crate::widgets::menu_value(ui, &mut choice, value, label)
                                    .clicked()
                                {
                                    changed |= select_highlight(settings, choice);
                                    crate::multi_edit_controls::assign(
                                        ui.ctx(),
                                        "/highlight_mode",
                                        choice.0,
                                    );
                                    crate::multi_edit_controls::assign(
                                        ui.ctx(),
                                        "/color_field/approach",
                                        choice.1,
                                    );
                                }
                            }
                        })
                    });
                    help::control(response.response, "Highlight method");
                });
            });
            ui.add_enabled_ui(settings.highlight_restoration, |ui| {
                if settings.highlight_mode == HighlightMode::Classic {
                    changed |=
                        crate::multi_edit_controls::field(ui, "/highlight_threshold_ev", |ui| {
                            automatic_adjustment(
                                ui,
                                "Highlight correction",
                                &mut settings.highlight_threshold_ev,
                                -2. ..=2.,
                                0.,
                                Some(0.),
                                column.row_controls("Threshold"),
                            )
                        });
                } else {
                    changed |=
                        crate::multi_edit_controls::field(ui, "/color_field/selectivity", |ui| {
                            adjustment_row(
                                ui,
                                "Color-field highlights",
                                &mut settings.color_field.selectivity,
                                0. ..=1.,
                                0.,
                                column.row_controls("Threshold"),
                            )
                        });
                }
            });
        });
        let advanced = crate::widgets::disclosure(
            ui,
            egui::CollapsingHeader::new("Advanced").id_salt("advanced-corrections"),
            |ui| {
                changed |= common::bad_pixels(ui, settings);
            },
        );
        help::control(advanced.header_response, "Advanced corrections");
    });
    changed
}

#[cfg(test)]
mod mixed_tests {
    use super::*;
    #[test]
    fn highlight_method_mixed_label_ignores_inactive_color_field_preference() {
        for color_field in [false, true] {
            let ctx = egui::Context::default();
            crate::widgets::install_style(&ctx);
            let mut a = Settings::new("Sunlight", "Standard");
            if color_field {
                a.highlight_mode = HighlightMode::ColorField;
            }
            let mut b = a.clone();
            b.color_field.approach = ColorFieldApproach::LocalGradient;
            let mut active = a.clone();
            crate::multi_edit_controls::begin_with_numeric(
                &ctx,
                vec![a.clone(), b.clone()],
                vec![a, b],
            );
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 1400.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        corrections(ui, &mut active);
                    });
                },
            );
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(labels.contains(&"Multi"), color_field);
            assert!(crate::multi_edit_controls::finish(&ctx).is_empty());
        }
    }
}
