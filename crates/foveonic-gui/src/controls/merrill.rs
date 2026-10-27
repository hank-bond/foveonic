//! Composes Merrill color, monochrome, reconstruction and correction controls.

use super::common::{self, adjustment, adjustment_row, automatic_adjustment, RowControls};
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

fn noise(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
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
        let column = common::LabelColumn::new(ui, &["Chroma", "Luma", "Banding"], &[]);
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
        changed |= column.adjustment(
            ui,
            ("Banding noise", "Banding"),
            &mut controls.banding,
            0..=4,
            2,
        );
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

fn detail(
    ui: &mut egui::Ui,
    enabled: &mut bool,
    texture: &mut foveonic::noise::TextureControls,
) -> bool {
    let mut changed = false;
    changed |= common::section_header(ui, EditingSection::Detail, "Detail", Some(enabled));
    ui.add_enabled_ui(*enabled, |ui| {
        let column = common::LabelColumn::new(ui, &["Fine", "Coarse"], &["Size"])
            .with_outer_padding(f32::from(
                crate::widgets::PARAMETER_GROUP_HORIZONTAL_PADDING,
            ));
        crate::widgets::parameter_group(ui, |ui| {
            changed |= column.adjustment(
                ui,
                ("Fine detail", "Fine"),
                &mut texture.fine,
                foveonic::noise::TextureControls::AMOUNT_RANGE,
                1.,
            );
            changed |= column.indented().adjustment(
                ui,
                ("Fine size", "Size"),
                &mut texture.fine_size,
                foveonic::noise::TextureControls::SIZE_RANGE,
                1.,
            );
        });
        crate::widgets::parameter_group(ui, |ui| {
            changed |= column.adjustment(
                ui,
                ("Coarse detail", "Coarse"),
                &mut texture.coarse,
                foveonic::noise::TextureControls::AMOUNT_RANGE,
                1.,
            );
            changed |= column.indented().adjustment(
                ui,
                ("Coarse size", "Size"),
                &mut texture.coarse_size,
                foveonic::noise::TextureControls::SIZE_RANGE,
                1.,
            );
        });
    });
    changed
}

pub(super) fn show(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
    order: &mut Vec<EditingSection>,
) -> bool {
    let mut changed = common::rendering(ui, settings);
    let mono = settings.is_monochrome();
    changed |= common::presets(
        ui,
        settings,
        metadata,
        common::PresetControls {
            white_balance: true,
            color_mode: !mono,
        },
    );
    let available = if mono {
        EditingSection::ALL.as_slice()
    } else {
        &[
            EditingSection::Tone,
            EditingSection::ColorEditor,
            EditingSection::Noise,
            EditingSection::Detail,
            EditingSection::Sharpening,
            EditingSection::Corrections,
        ]
    };
    changed |= super::sections::show(ui, order, available, |ui, section| match section {
        EditingSection::Mix => crate::mixer::show(ui, &mut settings.monochrome.mix),
        EditingSection::Tone => common::tone_with_exposure_range(ui, settings, mono, -4. ..=4.),
        EditingSection::ColorEditor => super::color_editor::show(ui, settings),
        EditingSection::Noise => {
            if mono {
                mono_noise(ui, settings)
            } else {
                noise(ui, settings)
            }
        }
        EditingSection::Detail => {
            if mono {
                detail(
                    ui,
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
    let changed = detail(ui, &mut controls.detail_enabled, &mut controls.texture);
    if before.is_some() || controls.noise_enabled || controls.detail_enabled {
        settings.reconstruction = Some(controls);
    }
    changed
}

fn mono_noise(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    common::section_header(ui, EditingSection::Noise, "Noise", None);
    let mut changed = adjustment(ui, "Mono noise", &mut settings.monochrome.noise, 0..=4, 2);
    changed |= adjustment(
        ui,
        "Mono banding",
        &mut settings.monochrome.banding,
        0..=4,
        2,
    );
    changed
}

fn corrections(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut changed = false;
    common::section_header(ui, EditingSection::Corrections, "Corrections", None);
    crate::widgets::parameter_group(ui, |ui| {
        let column = common::LabelColumn::new(ui, &["Highlights"], &["Threshold"])
            .with_toggle(ui, "Highlights")
            .with_outer_padding(f32::from(
                crate::widgets::PARAMETER_GROUP_HORIZONTAL_PADDING,
            ));
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
                    .width(ui.available_width() - 8.)
                    .selected_text(label);
                let response = crate::widgets::dropdown(ui, combo, |ui| {
                    for (value, label) in HIGHLIGHT_METHODS {
                        if crate::widgets::menu_value(ui, &mut choice, value, label).clicked() {
                            changed |= select_highlight(settings, choice);
                        }
                    }
                });
                help::control(response.response, "Highlight method");
            });
        });
        ui.add_enabled_ui(settings.highlight_restoration, |ui| {
            if settings.highlight_mode == HighlightMode::Classic {
                changed |= automatic_adjustment(
                    ui,
                    "Highlight correction",
                    &mut settings.highlight_threshold_ev,
                    -2. ..=2.,
                    0.,
                    Some(0.),
                    column.indented().row_controls("Threshold"),
                );
            } else {
                changed |= adjustment_row(
                    ui,
                    "Color-field highlights",
                    &mut settings.color_field.selectivity,
                    0. ..=1.,
                    0.,
                    0.1,
                    column.indented().row_controls("Threshold"),
                );
            }
        });
    });
    let advanced = egui::CollapsingHeader::new("Advanced")
        .id_salt("advanced-corrections")
        .show(ui, |ui| {
            changed |= common::pixel_corrections(ui, settings);
            changed |= ui
                .scope(|ui| {
                    help::control(
                        crate::widgets::checkbox(ui, &mut settings.row_correction, "Row banding"),
                        "Row banding",
                    )
                    .changed()
                })
                .inner;
        });
    help::control(advanced.header_response, "Advanced corrections");
    changed
}
