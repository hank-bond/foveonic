//! Groups pipeline buffers and component views in one inspection selector without scheduling component-only changes.

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
    egui::ComboBox::from_id_salt("artifact")
        .selected_text(selected)
        .width(205.)
        .show_ui(ui, |ui| {
            for image in outputs {
                if image.channels == Channels::Scalar {
                    if ui
                        .selectable_label(image.name == output, image.name)
                        .clicked()
                    {
                        choose(output, channel, image.name, -1);
                        ui.close_menu();
                    }
                } else {
                    ui.menu_button(image.name, |ui| {
                        for &(component, label) in components(image.channels) {
                            if ui
                                .selectable_label(
                                    image.name == output && component == *channel,
                                    label,
                                )
                                .clicked()
                            {
                                choose(output, channel, image.name, component);
                                ui.close_menu();
                            }
                        }
                    });
                }
            }
        });
    before != *output
}

#[cfg(test)]
mod tests {
    use super::*;
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
