//! Displays preset-linked reconstruction parameters and captures explicit overrides without changing the baseline.

use eframe::egui;
use foveonic::{
    noise::{self, Controls, Parameter},
    source::Metadata,
};

pub fn link_button(ui: &mut egui::Ui, linked: bool, parent: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22., 20.), egui::Sense::click());
    let stroke = egui::Stroke::new(1.3_f32, ui.visuals().text_color());
    let offset = if linked { 2.5 } else { 5. };
    for sign in [-1., 1.] {
        ui.painter().circle_stroke(
            rect.center() + egui::vec2(sign * offset, sign * 1.5),
            3.5,
            stroke,
        );
    }
    if linked {
        ui.painter().line_segment(
            [
                rect.center() - egui::vec2(3., 2.),
                rect.center() + egui::vec2(3., 2.),
            ],
            stroke,
        );
    }
    response.on_hover_text(if linked {
        format!("Linked to {parent}. Click to capture this value as a manual override.")
    } else {
        format!("Manual override. Click to follow the latest {parent} value.")
    })
}

pub fn show(
    ui: &mut egui::Ui,
    controls: &mut Controls,
    metadata: Option<&Metadata>,
    wb: &str,
    gain: f32,
) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("Advanced reconstruction")
        .id_salt("advanced-reconstruction")
        .default_open(crate::smoke::editor_review_active())
        .show(ui, |ui| {
            let calculated = metadata.ok_or_else(|| "Waiting for camera metadata".to_owned())
                .and_then(|m| noise::calculated_values(m, wb, gain, *controls).map_err(|e| e.to_string()));
            let values = match calculated {
                Ok(values) => Some(values),
                Err(error) => { ui.weak(error); None }
            };
            for parameter in Parameter::ALL {
                changed |= row(ui, &mut controls.overrides, parameter, values.map(|v| v[parameter as usize]));
            }
            ui.small("Links follow reference presets; overrides replace individual parameters. Detail remains a separate multiplier.");
        });
    changed
}

fn row(
    ui: &mut egui::Ui,
    overrides: &mut noise::Overrides,
    parameter: Parameter,
    automatic: Option<f32>,
) -> bool {
    let before = overrides.get(parameter);
    let mut value = before.or(automatic).unwrap_or(0.);
    ui.push_id(parameter.key(), |ui| {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(before.is_some() || automatic.is_some(), |ui| {
                if link_button(ui, before.is_none(), parameter.parent()).clicked() {
                    overrides.set(parameter, if before.is_none() { Some(value) } else { None });
                }
            });
            let manual = overrides.get(parameter);
            value = manual.or(automatic).unwrap_or(0.);
            let bounds = parameter.bounds();
            let range = bounds.start().min(value)..=bounds.end().max(value);
            let default = automatic.unwrap_or(value);
            let editing = ui.add_enabled_ui(manual.is_some(), |ui| {
                crate::controls::adjustment_with_nudge(
                    ui,
                    parameter.label(),
                    &mut value,
                    range,
                    default,
                    if parameter == Parameter::LumaNoiseScale {
                        0.1
                    } else {
                        0.01
                    },
                )
            });
            editing.response.on_hover_text(format!(
                "{}\nParent: {}",
                parameter.help(),
                parameter.parent()
            ));
            if manual.is_some() && editing.inner {
                overrides.set(parameter, Some(value));
            }
        });
    });
    before != overrides.get(parameter)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_link_captures_value_keeps_override_and_relinks_to_latest_parent() {
        let ctx = egui::Context::default();
        let mut overrides = noise::Overrides::default();
        let parameter = Parameter::MeanDecay;
        let mut frame = |time, automatic, events| {
            let mut changed = false;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(450., 100.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        changed = row(ui, &mut overrides, parameter, Some(automatic));
                    });
                },
            );
            (changed, output, overrides.get(parameter))
        };
        let (_, output, value) = frame(0., 0.95, vec![]);
        assert!(value.is_none());
        let point = output
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Circle(c) = &s.shape {
                    Some(c.center)
                } else {
                    None
                }
            })
            .unwrap();
        let click = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(
            1.,
            0.95,
            vec![egui::Event::PointerMoved(point), click(true)],
        );
        let (changed, _, value) = frame(1.1, 0.95, vec![click(false)]);
        assert!(changed);
        assert_eq!(value, Some(0.95));
        assert_eq!(frame(2., 0.4, vec![]).2, Some(0.95));
        frame(3., 0.4, vec![click(true)]);
        let (changed, _, value) = frame(3.1, 0.4, vec![click(false)]);
        assert!(changed);
        assert!(value.is_none());
    }
}
