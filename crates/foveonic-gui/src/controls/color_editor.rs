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
    let mut changed = false;
    changed |= super::common::section_header(
        ui,
        EditingSection::ColorEditor,
        "Color Editor",
        Some(&mut settings.color_editor_enabled),
    );
    ui.add_enabled_ui(settings.color_editor_enabled, |ui| {
        let selection = ui.make_persistent_id("color-editor-band");
        let mut selected = ui
            .data_mut(|data| data.get_temp::<usize>(selection))
            .unwrap_or(0)
            .min(COLORS.len());
        let controls = &mut settings.photographic.color_editor;
        ui.horizontal(|ui| {
            let count = ColorEditorControls::LABELS.len() as f32;
            let gaps = (count - 1.) * ui.spacing().item_spacing.x;
            let width = ui.available_width();
            let side = ((width - gaps) / count)
                .min(ui.spacing().interact_size.y)
                .max(12.);
            ui.add_space(((width - count * side - gaps) * 0.5).max(0.));
            for (index, label) in ColorEditorControls::LABELS.into_iter().enumerate() {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
                if response.clicked() {
                    selected = index;
                }
                let visuals = ui.style().interact_selectable(&response, selected == index);
                let border = if selected == index {
                    egui::Stroke::new(2_f32, egui::Color32::WHITE)
                } else {
                    visuals.bg_stroke
                };
                ui.painter()
                    .rect(rect, 3., visuals.bg_fill, border, egui::StrokeKind::Inside);
                let fill = rect.shrink(3.);
                if index < COLORS.len() {
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
                                fill.left() + fill.width() * k as f32 / COLORS.len() as f32,
                                fill.top(),
                            ),
                            egui::pos2(
                                fill.left() + fill.width() * (k + 1) as f32 / COLORS.len() as f32,
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
        ui.data_mut(|data| data.insert_temp(selection, selected));
        let mono = settings.rendering == foveonic::render::Rendering::Monochrome;
        let adjustment = if selected == COLORS.len() {
            &mut controls.master
        } else {
            &mut controls.bands[selected]
        };
        let column = LabelColumn::new(ui, &["Hue", "Saturation", "Lightness"], &[]);
        ui.add_enabled_ui(!mono, |ui| {
            changed |= column.adjustment(
                ui,
                ("Color Editor Hue", "Hue"),
                &mut adjustment.hue,
                -1. ..=1.,
                0.,
            );
            changed |= column.adjustment(
                ui,
                ("Color Editor Saturation", "Saturation"),
                &mut adjustment.saturation,
                -1. ..=1.,
                0.,
            );
        });
        changed |= column.adjustment(
            ui,
            ("Color Editor Lightness", "Lightness"),
            &mut adjustment.lightness,
            -1. ..=1.,
            0.,
        );
        if mono {
            ui.weak("Lightness uses the original subject colors.");
        }
    });
    changed
}
