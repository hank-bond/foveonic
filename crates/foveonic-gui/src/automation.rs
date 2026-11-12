//! Observes real controls for opt-in scripted application checks.

use eframe::egui;
use serde::Serialize;
use std::cell::RefCell;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Text {
    pub value: String,
    pub rect: [f32; 4],
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Control {
    pub pointer: Option<String>,
    pub mapping: crate::multi_edit_controls::Mapping,
    pub label: String,
    pub slider: [f32; 4],
    pub readout: [f32; 4],
    pub text: Vec<Text>,
    pub value: f64,
    pub range: [f64; 2],
    pub step: Option<f64>,
    pub mixed: bool,
    pub enabled: bool,
    pub visible: bool,
    pub dragging: bool,
    pub pointer_position: Option<[f32; 2]>,
    pub pointer_down: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Button {
    pub label: String,
    pub rect: [f32; 4],
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Choice {
    pub kind: &'static str,
    pub pointer: Option<String>,
    pub label: String,
    pub rect: [f32; 4],
    pub text: Vec<Text>,
    pub checked: Option<bool>,
    pub painted_checked: Option<bool>,
    pub enabled: bool,
    pub visible: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct Snapshot {
    pub time_seconds: f64,
    pub panel: Option<[f32; 4]>,
    pub controls: Vec<Control>,
    pub buttons: Vec<Button>,
    pub choices: Vec<Choice>,
}

thread_local! {
    static OBSERVATION: RefCell<Option<Snapshot>> = const { RefCell::new(None) };
    static REVEAL: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub(crate) fn enable() {
    OBSERVATION.with(|slot| *slot.borrow_mut() = Some(Snapshot::default()));
}

pub(crate) fn enabled() -> bool {
    OBSERVATION.with(|slot| slot.borrow().is_some())
}

pub(crate) fn begin_frame(ctx: &egui::Context) {
    OBSERVATION.with(|slot| {
        if let Some(snapshot) = slot.borrow_mut().as_mut() {
            *snapshot = Snapshot::default();
            snapshot.time_seconds = ctx.input(|input| input.time);
        }
    });
}

pub(crate) fn panel(rect: egui::Rect) {
    OBSERVATION.with(|slot| {
        if let Some(snapshot) = slot.borrow_mut().as_mut() {
            snapshot.panel = Some(rectangle(rect));
        }
    });
}

pub(crate) fn snapshot() -> Option<Snapshot> {
    OBSERVATION.with(|slot| slot.borrow().clone())
}

pub(crate) fn button(label: &str, response: &egui::Response) {
    OBSERVATION.with(|slot| {
        if let Some(snapshot) = slot.borrow_mut().as_mut() {
            snapshot.buttons.push(Button {
                label: label.to_owned(),
                rect: rectangle(response.rect),
                enabled: response.enabled(),
            });
        }
    });
}

pub(crate) fn reveal(pointer: Option<&str>) {
    REVEAL.with(|target| *target.borrow_mut() = pointer.map(str::to_owned));
}

pub(crate) fn revealing() -> bool {
    REVEAL.with(|target| target.borrow().is_some())
}

fn rectangle(rect: egui::Rect) -> [f32; 4] {
    [rect.min.x, rect.min.y, rect.max.x, rect.max.y]
}

fn text_in(shape: &egui::Shape, target: egui::Rect, output: &mut Vec<Text>) {
    match shape {
        egui::Shape::Text(text) => {
            let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
            if rect.intersects(target) {
                output.push(Text {
                    value: text.galley.job.text.clone(),
                    rect: rectangle(rect),
                });
            }
        }
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                text_in(shape, target, output);
            }
        }
        _ => {}
    }
}

pub(crate) fn choice(
    ui: &egui::Ui,
    kind: &'static str,
    label: &str,
    checked: Option<bool>,
    icon: Option<egui::Rect>,
    response: &egui::Response,
) {
    if !enabled() {
        return;
    }
    let pointer =
        crate::multi_edit_controls::observed_property(ui.ctx()).map(|property| property.0);
    let visible = ui.clip_rect().contains(response.rect.center());
    if !visible
        && REVEAL.with(|target| {
            let target = target.borrow();
            target
                .as_deref()
                .is_some_and(|target| Some(target) == pointer.as_deref() || target == label)
        })
    {
        ui.scroll_to_rect_animation(
            response.rect,
            Some(egui::Align::Center),
            egui::style::ScrollAnimation::none(),
        );
        ui.ctx().request_repaint();
    }
    let mut text = Vec::new();
    let mut painted_checked = false;
    ui.ctx().graphics(|layers| {
        if let Some(shapes) = layers.get(ui.layer_id()) {
            for shape in shapes.all_entries() {
                text_in(&shape.shape, response.rect, &mut text);
                if let (Some(icon), egui::Shape::Path(path)) = (icon, &shape.shape) {
                    if path.points.len() == 3
                        && path
                            .points
                            .iter()
                            .all(|point| icon.expand(2.).contains(*point))
                    {
                        painted_checked = true;
                    }
                }
            }
        }
    });
    OBSERVATION.with(|slot| {
        if let Some(snapshot) = slot.borrow_mut().as_mut() {
            snapshot.choices.push(Choice {
                kind,
                pointer,
                label: label.to_owned(),
                rect: rectangle(response.rect),
                text,
                checked,
                painted_checked: icon.map(|_| painted_checked),
                enabled: response.enabled(),
                visible,
            });
        }
    });
}

pub(crate) struct ControlInput<'a> {
    pub label: &'a str,
    pub pointer: Option<String>,
    pub mapping: crate::multi_edit_controls::Mapping,
    pub slider: &'a egui::Response,
    pub readout: &'a egui::Response,
    pub value: f64,
    pub range: [f64; 2],
    pub step: Option<f64>,
    pub mixed: bool,
}

pub(crate) fn control(ui: &egui::Ui, input: ControlInput<'_>) {
    if !enabled() {
        return;
    }
    let visible = ui.clip_rect().contains(input.slider.rect.center());
    if !visible
        && REVEAL.with(|target| {
            target.borrow().as_deref() == input.pointer.as_deref() && input.pointer.is_some()
        })
    {
        ui.scroll_to_rect_animation(
            input.slider.rect,
            Some(egui::Align::Center),
            egui::style::ScrollAnimation::none(),
        );
        ui.ctx().request_repaint();
    }
    let mut text = Vec::new();
    ui.ctx().graphics(|layers| {
        if let Some(shapes) = layers.get(ui.layer_id()) {
            for shape in shapes.all_entries() {
                text_in(&shape.shape, input.readout.rect, &mut text);
            }
        }
    });
    OBSERVATION.with(|slot| {
        if let Some(snapshot) = slot.borrow_mut().as_mut() {
            snapshot.controls.push(Control {
                pointer: input.pointer,
                mapping: input.mapping,
                label: input.label.to_owned(),
                slider: rectangle(input.slider.rect),
                readout: rectangle(input.readout.rect),
                text,
                value: input.value,
                range: input.range,
                step: input.step,
                mixed: input.mixed,
                enabled: input.slider.enabled(),
                visible,
                dragging: input.slider.dragged(),
                pointer_position: ui
                    .input(|input| input.pointer.interact_pos())
                    .map(|position| [position.x, position.y]),
                pointer_down: ui.input(|input| input.pointer.primary_down()),
            });
        }
    });
}

#[cfg(test)]
mod choice_tests {
    use super::*;
    #[test]
    fn records_checkbox_paint_and_state() {
        let ctx = egui::Context::default();
        enable();
        let mut value = true;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            begin_frame(ctx);
            egui::CentralPanel::default().show(ctx, |ui| {
                crate::widgets::checkbox(ui, &mut value, "Tone");
            });
        });
        let first = snapshot().unwrap();
        let check = first
            .choices
            .iter()
            .find(|c| c.kind == "checkbox" && c.label == "Tone")
            .unwrap();
        assert!(check.text.iter().any(|text| text.value == "Tone"));
        assert_eq!(check.checked, Some(true));
        assert_eq!(check.painted_checked, Some(true));
        value = false;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            begin_frame(ctx);
            egui::CentralPanel::default().show(ctx, |ui| {
                crate::widgets::checkbox(ui, &mut value, "Tone");
            });
        });
        let second = snapshot().unwrap();
        let check = second
            .choices
            .iter()
            .find(|c| c.kind == "checkbox" && c.label == "Tone")
            .unwrap();
        assert_eq!(check.checked, Some(false));
        assert_eq!(check.painted_checked, Some(false));
    }
}
