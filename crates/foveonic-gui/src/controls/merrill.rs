//! Composes Merrill color, monochrome, reconstruction and correction controls.

use super::common::{self, adjustment, adjustment_row, automatic_adjustment, RowControls};
use crate::help;
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

pub(super) fn show(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    metadata: Option<&Metadata>,
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
    ui.separator();
    if mono {
        changed |= crate::mixer::show(ui, &mut settings.monochrome.mix);
        ui.separator();
    }
    changed |= common::tone(ui, settings, mono);
    ui.separator();
    if !mono {
        changed |= common::reconstruction(ui, settings);
        if let Some(controls) = &mut settings.reconstruction {
            ui.scope(|ui| {
                changed |= adjustment(ui, "Chroma noise", &mut controls.chroma, 0..=4, 2);
                changed |= adjustment(ui, "Luma noise", &mut controls.luminance, 0..=4, 2);
                changed |= adjustment(ui, "Banding noise", &mut controls.banding, 0..=4, 2);
                changed |= adjustment(ui, "Detail", &mut controls.detail, 0. ..=2., 1.);
                changed |= crate::advanced::show(
                    ui,
                    controls,
                    &mut settings.color_dq_gain,
                    metadata,
                    &settings.white_balance,
                );
            });
        }
    } else {
        changed |= adjustment(ui, "Mono noise", &mut settings.monochrome.noise, 0..=4, 2);
        changed |= adjustment(
            ui,
            "Mono banding",
            &mut settings.monochrome.banding,
            0..=4,
            2,
        );
    }
    ui.separator();
    changed |= common::refinement(ui, settings, metadata);
    ui.separator();
    ui.weak("Corrections");
    ui.scope(|ui| {
        ui.horizontal(|ui| {
            help::control(
                ui.add_sized([92., 20.], egui::Label::new("Method")),
                "Highlight method",
            );
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
    if settings.highlight_mode == HighlightMode::Classic {
        changed |= automatic_adjustment(
            ui,
            "Highlight correction",
            &mut settings.highlight_threshold_ev,
            -2. ..=2.,
            0.,
            Some(0.),
            Some(&mut settings.highlight_restoration),
        );
    } else {
        changed |= adjustment_row(
            ui,
            "Color-field highlights",
            &mut settings.color_field.selectivity,
            0. ..=1.,
            0.,
            0.1,
            RowControls {
                enabled: Some(&mut settings.highlight_restoration),
                ..Default::default()
            },
        );
    }
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
