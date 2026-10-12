//! Groups pipeline buffers and component views in one inspection selector without scheduling component-only changes.

use crate::help;
use eframe::egui;
use foveonic::{
    inspection::{self, Channels},
    render::Settings,
};

pub fn components(channels: Channels) -> &'static [(i32, &'static str)] {
    match channels {
        Channels::Layers => &[(0, "Layer 1"), (1, "Layer 2"), (2, "Layer 3")],
        Channels::Rgb => &[(-1, "RGB"), (0, "Red"), (1, "Green"), (2, "Blue")],
        Channels::Scalar => &[(-1, "Scalar")],
    }
}
fn choose(output: &mut String, channel: &mut i32, name: &str, component: i32) {
    *output = name.into();
    *channel = component;
}

/// A full-width disclosure row. Opening it never selects a processing output.
fn group_row(ui: &mut egui::Ui, name: &str, selected: bool) -> bool {
    let id = ui.make_persistent_id(("inspect group", name));
    let mut open = ui.data_mut(|d| d.get_persisted::<bool>(id).unwrap_or(false));
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
        egui::Sense::click(),
    );
    if response.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    let visuals = ui.style().interact_selectable(&response, selected);
    ui.painter()
        .rect_filled(rect, visuals.corner_radius, visuals.bg_fill);
    let padding = ui.spacing().button_padding.x;
    ui.painter().text(
        rect.left_center() + egui::vec2(padding, 0.),
        egui::Align2::LEFT_CENTER,
        name,
        egui::TextStyle::Button.resolve(ui.style()),
        visuals.text_color(),
    );
    ui.painter().text(
        rect.right_center() - egui::vec2(padding, 0.),
        egui::Align2::RIGHT_CENTER,
        if open { "▾" } else { "▸" },
        egui::TextStyle::Button.resolve(ui.style()),
        visuals.text_color(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, ui.is_enabled(), name)
    });
    response.on_hover_text(help::inspection(name).unwrap());
    open
}

/// Returns true only when processing must produce a different buffer.
pub fn show(
    ui: &mut egui::Ui,
    settings: &Settings,
    output: &mut String,
    channel: &mut i32,
) -> bool {
    let before = output.clone();
    let outputs = inspection::image_outputs(settings);
    if !outputs.iter().any(|o| o.name == output) {
        choose(output, channel, "final", -1);
    }
    let current = outputs.iter().find(|o| o.name == output).unwrap();
    let choices = components(current.channels);
    if !choices.iter().any(|(c, _)| c == channel) {
        *channel = choices[0].0;
    }
    let label = choices.iter().find(|(c, _)| c == channel).unwrap().1;
    let selected = if current.channels == Channels::Scalar {
        output.clone()
    } else {
        format!("{output} · {label}")
    };
    let current_help = help::inspection(current.name).unwrap();
    egui::ComboBox::from_id_salt("artifact")
        .selected_text(selected)
        .width(205.)
        .show_ui(ui, |ui| {
            for image in outputs {
                if image.channels == Channels::Scalar {
                    if ui
                        .selectable_label(image.name == output, image.name)
                        .on_hover_text(help::inspection(image.name).unwrap())
                        .clicked()
                    {
                        choose(output, channel, image.name, -1);
                        ui.close_menu();
                    }
                } else {
                    if group_row(ui, image.name, image.name == output) {
                        ui.indent(image.name, |ui| {
                        for &(component, label) in components(image.channels) {
                            if ui
                                .selectable_label(
                                    image.name == output && component == *channel,
                                    label,
                                )
                                .on_hover_text(if image.channels == Channels::Layers {
                                    "Shows one of the camera's three color-sensitive sensor layers in gray. Each responds to overlapping colors; it is not a finished red, green, or blue channel."
                                } else if component == -1 {
                                    "Shows the normal color view of this processing stage."
                                } else {
                                    "Shows this color channel in gray: brighter means more of that channel. This changes the display only; it does not remove colors from the photo or change the scopes."
                                })
                                .clicked()
                            {
                                choose(output, channel, image.name, component);
                                ui.close_menu();
                            }
                        }
                        });
                    }
                }
            }
        })
        .response
        .on_hover_text(format!(
            "{}\n\nInspect shows steps used to make the photo. Earlier steps do not include later edits. Changing this view does not change your export: export always uses the finished photo.",
            current_help,
        ));
    before != *output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disclosure_opens_and_closes_from_both_edges_of_the_row() {
        let ctx = egui::Context::default();
        let draw = |events: Vec<egui::Event>| {
            let mut open = false;
            let mut rect = egui::Rect::NOTHING;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(300., 300.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let origin = ui.cursor().min;
                        let width = ui.available_width();
                        let height = ui.spacing().interact_size.y;
                        open = group_row(ui, "repair.layers", false);
                        rect = egui::Rect::from_min_size(origin, egui::vec2(width, height));
                    });
                },
            );
            (open, rect)
        };
        let (open, rect) = draw(vec![]);
        assert!(!open);
        let click = |point| {
            draw(vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ]);
            draw(vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }])
            .0
        };
        assert!(click(rect.left_center() + egui::vec2(5., 0.)));
        assert!(draw(vec![]).0); // state survives subsequent frames
        assert!(!click(rect.right_center() - egui::vec2(5., 0.)));
    }

    #[test]
    fn component_selection_reuses_the_root_and_scalar_views_have_no_layers() {
        let mut output = "final".to_owned();
        let mut channel = -1;
        choose(&mut output, &mut channel, "final", 2);
        assert_eq!((&*output, channel), ("final", 2));
        choose(&mut output, &mut channel, "sensor.layers", 1);
        assert_eq!((&*output, channel), ("sensor.layers", 1));
        assert_eq!(components(Channels::Scalar), &[(-1, "Scalar")]);
        assert_eq!(components(Channels::Layers).len(), 3);
    }
}
