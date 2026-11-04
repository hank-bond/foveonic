//! Selects color ranges for shared linear Hue, Saturation and Lightness adjustments.
use super::common::LabelColumn;
use crate::preferences::EditingSection;
use eframe::egui;
use foveonic::render::{ColorEditorControls, Settings};

const COLORS: [[u8; 3]; 8] = [
    [176, 72, 87],
    [190, 122, 61],
    [184, 171, 63],
    [104, 172, 69],
    [93, 180, 179],
    [66, 105, 178],
    [127, 55, 171],
    [177, 76, 144],
];

pub(super) fn show(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut state = crate::editor::advanced_color::state(ui.ctx());
    let mut changed = super::common::section_header_with(
        ui,
        EditingSection::ColorEditor,
        "Color Range",
        Some(&mut settings.color_editor_enabled),
        |ui, enabled| modes(ui, &mut state, enabled),
    );
    crate::editor::advanced_color::store(ui.ctx(), state.clone());
    ui.add_enabled_ui(settings.color_editor_enabled, |ui| {
        if state.advanced {
            changed |= super::advanced_color::show(ui, settings);
            return;
        }
        if settings.color_range_preview.take().is_some() {
            changed = true;
        }

        let (mut selected, picked) = crate::editor::direct_color::selection(ui.ctx());
        let picking =
            crate::editor::direct_color::dragging(ui.ctx()) && picked.iter().any(|w| *w > 0.);
        let controls = &mut settings.photographic.color_editor;
        let count = ColorEditorControls::LABELS.len() as f32;
        let gaps = (count - 1.) * ui.spacing().item_spacing.x;
        let padding = 2. * f32::from(crate::widgets::BUTTON_GROUP_MARGIN);
        let available = ui.available_width();
        let side = ((available - padding - gaps) / count)
            .min(ui.spacing().interact_size.y)
            .max(12.);
        let width = count * side + gaps + padding;
        ui.horizontal(|ui| {
            ui.add_space(((available - width) * 0.5).max(0.));
            ui.allocate_ui_with_layout(
                egui::vec2(width, side + padding),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    crate::widgets::button_group(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (index, label) in
                                ColorEditorControls::LABELS.into_iter().enumerate()
                            {
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(side, side),
                                    egui::Sense::click(),
                                );
                                if response.clicked() {
                                    selected = index;
                                    crate::editor::direct_color::select(ui.ctx(), selected);
                                }
                                let visuals =
                                    ui.style().interact_selectable(&response, selected == index);
                                let border = if selected == index {
                                    egui::Stroke::new(2_f32, egui::Color32::WHITE)
                                } else {
                                    visuals.bg_stroke
                                };
                                ui.painter().rect(
                                    rect,
                                    3.,
                                    visuals.bg_fill,
                                    border,
                                    egui::StrokeKind::Inside,
                                );
                                let fill = rect.shrink(3.);
                                if picking && picked.get(index).is_none_or(|w| *w == 0.) {
                                    // Keep only the affected swatches colored during the gesture.
                                } else if index < COLORS.len() {
                                    ui.painter().rect_filled(
                                        fill,
                                        2.,
                                        egui::Color32::from_rgb(
                                            COLORS[index][0],
                                            COLORS[index][1],
                                            COLORS[index][2],
                                        ),
                                    );
                                } else {
                                    for (k, color) in COLORS.into_iter().enumerate() {
                                        let stripe = egui::Rect::from_min_max(
                                            egui::pos2(
                                                fill.left()
                                                    + fill.width() * k as f32 / COLORS.len() as f32,
                                                fill.top(),
                                            ),
                                            egui::pos2(
                                                fill.left()
                                                    + fill.width() * (k + 1) as f32
                                                        / COLORS.len() as f32,
                                                fill.bottom(),
                                            ),
                                        );
                                        ui.painter().rect_filled(
                                            stripe,
                                            0.,
                                            egui::Color32::from_rgb(color[0], color[1], color[2]),
                                        );
                                    }
                                }
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::SelectableLabel,
                                        ui.is_enabled(),
                                        selected == index,
                                        label,
                                    )
                                });
                                response.on_hover_text(label);
                            }
                        });
                    })
                },
            );
        });
        let mono = settings.rendering == foveonic::render::Rendering::Monochrome;
        let adjustment = if selected == COLORS.len() {
            &mut controls.master
        } else {
            &mut controls.bands[selected]
        };
        let column = LabelColumn::new(ui);
        ui.add_enabled_ui(!mono, |ui| {
            changed |= column.adjustment(
                ui,
                ("Color Range Hue", "Hue"),
                &mut adjustment.hue,
                -1. ..=1.,
                0.,
            );
            changed |= column.saturation_adjustment(
                ui,
                ("Color Range Saturation", "Saturation"),
                &mut adjustment.saturation,
            );
        });
        changed |= column.adjustment(
            ui,
            ("Color Range Lightness", "Lightness"),
            &mut adjustment.lightness,
            -1. ..=1.,
            0.,
        );
        crate::editor::direct_color::button(ui);
        if mono {
            ui.weak("Lightness uses the original subject colors.");
        }
    });
    changed
}

fn modes(ui: &mut egui::Ui, state: &mut crate::editor::advanced_color::State, enabled: bool) {
    ui.add_enabled_ui(enabled, |ui| {
        crate::widgets::button_group(ui, |ui| {
            ui.horizontal(|ui| {
                for (advanced, label) in [(false, "Basic"), (true, "Advanced")] {
                    let label = if advanced {
                        crate::widgets::fitting_button_label(
                            ui,
                            label,
                            "Adv.",
                            ui.available_width()
                                - crate::sections::EDITING_ACTIONS_WIDTH
                                - ui.spacing().item_spacing.x,
                        )
                    } else {
                        label
                    };
                    if crate::widgets::selectable_value(ui, &mut state.advanced, advanced, label)
                        .changed()
                    {
                        crate::editor::direct_color::exit(ui.ctx());
                        state.rename = None;
                    }
                }
            });
        });
    });
}
