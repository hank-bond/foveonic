//! Drives and records a real multi-photo slider gesture in a native scenario.

use crate::{automation, App};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Pattern {
    #[default]
    Forward,
    Scrub,
}

#[cfg(test)]
#[test]
fn maintained_slider_recipes_use_the_application_sidecar_contract() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../scripts/profiling/slider_cases.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        for recipe in case["recipes"].as_array().unwrap() {
            let bytes = serde_json::to_vec(&serde_json::json!({
                "format":"foveonic-editing", "version":1, "settings":recipe
            }))
            .unwrap();
            assert!(
                foveonic::sidecar::decode(&bytes).is_ok(),
                "invalid slider fixture: {}",
                case["name"]
            );
        }
    }
}

#[derive(Serialize)]
struct Frame {
    time_seconds: f64,
    revision: u64,
    worker_busy: bool,
    panel: Option<[f32; 4]>,
    control: Option<automation::Control>,
    values: Vec<Option<f64>>,
    rendered: Vec<Option<Rendered>>,
}

#[derive(Serialize)]
struct Rendered {
    task: u64,
    revision: u64,
    requested_value: Option<f64>,
    scale_divisor: u32,
}

#[derive(Serialize)]
pub(super) struct Drag {
    pointer: String,
    pattern: Pattern,
    paths: Vec<PathBuf>,
    initial: Vec<f64>,
    inventory: Vec<automation::Control>,
    events: Vec<[f32; 2]>,
    frames: Vec<Frame>,
    errors: Vec<String>,
}

impl Drag {
    pub(super) fn start(
        app: &App,
        pointer: &str,
        steps: usize,
        fraction: f32,
        expected_mixed: bool,
        pattern: Pattern,
        queue: &mut VecDeque<Vec<egui::Event>>,
    ) -> Result<Self, String> {
        let snapshot = automation::snapshot().ok_or("control observation is not enabled")?;
        let controls: Vec<_> = snapshot
            .controls
            .iter()
            .filter(|c| c.pointer.as_deref() == Some(pointer))
            .collect();
        if controls.len() != 1 {
            return Err(format!(
                "expected one visible control for {pointer}, found {}; observed: {}",
                controls.len(),
                snapshot
                    .controls
                    .iter()
                    .map(|c| format!("{} ({:?})", c.label, c.pointer))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let control = controls[0];
        if !control.enabled {
            return Err(format!("{pointer} is disabled"));
        }
        if !control.visible {
            return Err(format!("{pointer} is outside the visible editing panel"));
        }
        let paths = app.edit_selection.paths().to_vec();
        let initial: Vec<f64> = values(app, &paths, pointer)
            .into_iter()
            .collect::<Option<_>>()
            .ok_or_else(|| {
                format!("{pointer} is not a numeric setting for every selected photo")
            })?;
        if initial.len() < 2 {
            return Err(format!(
                "{pointer} requires at least two selected photos; got {initial:?}"
            ));
        }
        let mixed = initial.iter().any(|v| (*v - initial[0]).abs() > 1e-9);
        if mixed != expected_mixed {
            return Err(format!(
                "{pointer} requires at least two photos with {} starting values; got {initial:?}",
                if expected_mixed { "different" } else { "equal" }
            ));
        }
        if control.mixed != expected_mixed {
            return Err(format!(
                "{pointer} has {} values but its mixed readout is {}",
                if mixed { "different" } else { "equal" },
                control.mixed
            ));
        }
        let [left, top, right, bottom] = control.slider;
        let travel = f64::from((right - left) * fraction / ((right - left) - 12.).max(1.))
            * (control.range[1] - control.range[0]);
        if initial
            .iter()
            .any(|value| control.mapping.control(*value) + travel > control.range[1] + 1e-6)
        {
            return Err(format!(
                "{pointer} starts too near its upper limit for this gesture: {initial:?}"
            ));
        }
        let start = egui::pos2((left + right) * 0.5, (top + bottom) * 0.5);
        let end = start + egui::vec2((right - left) * fraction, 0.);
        if end.x >= right - 4. {
            return Err("drag extends beyond the slider".into());
        }
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        queue.push_back(vec![egui::Event::PointerMoved(start), button(start, true)]);
        let mut events = vec![[start.x, start.y]];
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let distance = match pattern {
                Pattern::Forward => t,
                Pattern::Scrub if t < 0.4 => t / 0.4 * 0.7,
                Pattern::Scrub if t < 0.7 => 0.7 - (t - 0.4) / 0.3 * 0.5,
                Pattern::Scrub => 0.2 + (t - 0.7) / 0.3 * 0.8,
            };
            let pos = start.lerp(end, distance);
            events.push([pos.x, pos.y]);
            queue.push_back(vec![egui::Event::PointerMoved(pos)]);
        }
        queue.push_back(vec![button(end, false)]);
        queue.push_back(vec![]);
        let mut drag = Self {
            pointer: pointer.into(),
            pattern,
            paths,
            initial,
            inventory: snapshot.controls,
            events,
            frames: Vec::new(),
            errors: Vec::new(),
        };
        drag.observe(app);
        Ok(drag)
    }

    pub(super) fn observe(&mut self, app: &App) {
        if let Some(snapshot) = automation::snapshot() {
            self.frames.push(Frame {
                time_seconds: snapshot.time_seconds,
                revision: app.revision,
                worker_busy: app.worker.busy(),
                panel: snapshot.panel,
                control: snapshot
                    .controls
                    .into_iter()
                    .find(|c| c.pointer.as_deref() == Some(self.pointer.as_str())),
                values: values(app, &self.paths, &self.pointer),
                rendered: self
                    .paths
                    .iter()
                    .map(|path| {
                        app.frame
                            .as_ref()
                            .filter(|frame| frame.path == *path)
                            .or_else(|| app.multi_edit_frame(path))
                            .map(|frame| Rendered {
                                task: frame.diagnostic_task.id,
                                revision: frame.diagnostic_task.revision,
                                requested_value: serde_json::to_value(&frame.requested)
                                    .ok()
                                    .and_then(|settings| {
                                        settings
                                            .pointer(&self.pointer)
                                            .and_then(|value| value.as_f64())
                                    }),
                                scale_divisor: frame.artifact.settings.working_scale.divisor(),
                            })
                    })
                    .collect(),
            });
        }
    }

    pub(super) fn check(&mut self) -> Result<(), String> {
        self.errors.clear();
        let Some(first) = self.frames.first().and_then(|f| f.control.as_ref()) else {
            return Err("missing starting control observation".into());
        };
        let start_slider = first.slider;
        let start_readout = first.readout;
        let start_panel = self.frames[0].panel;
        let min_changes = first.step.map_or(3, |step| {
            let travel = f64::from(
                (self.events.last().unwrap()[0] - self.events[0][0])
                    / (first.slider[2] - first.slider[0] - 12.).max(1.),
            ) * (first.range[1] - first.range[0]);
            ((travel / step).floor() as usize).clamp(1, 3)
        });
        let mut changes = vec![0usize; self.initial.len()];
        let mut previous = self.initial.clone();
        let mut previous_pointer = None;
        let mut drag_started = false;
        let mut expected_travel = 0.;
        for (index, frame) in self.frames.iter().enumerate() {
            let Some(control) = &frame.control else {
                self.errors
                    .push(format!("frame {index}: control disappeared"));
                continue;
            };
            if !same_rect(start_slider, control.slider)
                || !same_rect(start_readout, control.readout)
                || frame
                    .panel
                    .zip(start_panel)
                    .is_some_and(|(a, b)| !same_rect(a, b))
            {
                self.errors
                    .push(format!("frame {index}: control or panel geometry moved"));
            }
            if control.text.is_empty() {
                self.errors
                    .push(format!("frame {index}: readout text was not observed"));
            }
            let mut pointer_movement = 0.;
            if let Some(position) = control.pointer_position {
                if let Some(previous) = previous_pointer {
                    let movement = position[0] - previous;
                    if control.dragging {
                        drag_started = true;
                        pointer_movement = movement;
                        expected_travel +=
                            f64::from(movement / (start_slider[2] - start_slider[0] - 12.).max(1.))
                                * (first.range[1] - first.range[0]);
                    } else if drag_started && control.pointer_down && movement.abs() > 0.01 {
                        self.errors
                            .push(format!("frame {index}: slider lost the active drag"));
                    }
                }
                previous_pointer = Some(position[0]);
            }
            for text in &control.text {
                if text.value != "Multi" && !valid_readout(&text.value) {
                    self.errors
                        .push(format!("frame {index}: invalid readout {:?}", text.value));
                }
                if text.rect[0] < control.readout[0] - 2. || text.rect[2] > control.readout[2] + 2.
                {
                    self.errors.push(format!(
                        "frame {index}: readout {:?} overflows its field",
                        text.value
                    ));
                }
            }
            for (photo, value) in frame.values.iter().enumerate() {
                match value {
                    Some(value) => {
                        let change = *value - previous[photo];
                        if change.abs() > 1e-7 {
                            changes[photo] += 1;
                        }
                        if (pointer_movement > 0.01 && change < -1e-7)
                            || (pointer_movement < -0.01 && change > 1e-7)
                        {
                            self.errors.push(format!(
                                "frame {index}: photo {photo} moved against the drag"
                            ));
                        }
                        previous[photo] = *value;
                    }
                    None => self.errors.push(format!(
                        "frame {index}: photo {photo} lost its numeric value"
                    )),
                }
            }
        }
        for (photo, count) in changes.iter().enumerate() {
            if *count < min_changes {
                self.errors.push(format!(
                    "photo {photo}: only {count} value changes during sustained drag"
                ));
            }
        }
        if previous
            .iter()
            .zip(&self.initial)
            .any(|(end, start)| end <= &(start + 1e-6))
        {
            self.errors.push("not every selected photo advanced".into());
        }
        let shifts: Vec<_> = previous
            .iter()
            .zip(&self.initial)
            .map(|(end, start)| first.mapping.control(*end) - first.mapping.control(*start))
            .collect();
        if shifts.iter().any(|shift| (shift - shifts[0]).abs() > 1e-4) {
            self.errors.push(format!(
                "relative changes disagree across selected photos: {shifts:?}"
            ));
        }
        let tolerance = first.step.map_or(1e-4, |step| step * 0.5 + 1e-4);
        if shifts
            .iter()
            .any(|shift| (shift - expected_travel).abs() > tolerance)
        {
            self.errors.push(format!("pointer movement implies {expected_travel:.6} in control units, but photo changes were {shifts:?}"));
        }
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.join("; "))
        }
    }
}

fn values(app: &App, paths: &[PathBuf], pointer: &str) -> Vec<Option<f64>> {
    paths
        .iter()
        .map(|path| {
            app.multi_edit
                .settings(path)
                .and_then(|settings| serde_json::to_value(settings).ok())
                .and_then(|settings| settings.pointer(pointer).and_then(|value| value.as_f64()))
        })
        .collect()
}

fn same_rect(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() <= 1.)
}

// Validate the drawn text independently of the application formatter.
fn valid_readout(text: &str) -> bool {
    let Ok(value) = text.parse::<f64>() else {
        return false;
    };
    if !value.is_finite() || (value != 0. && value.abs() < 0.005) {
        return false;
    }
    let (mantissa, exponent) = match text.split_once('e') {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (text, None),
    };
    if let Some(exponent) = exponent {
        if exponent.parse::<i32>().is_err() {
            return false;
        }
    }
    let number = mantissa.trim_start_matches(['-', '+']);
    let mut parts = number.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next().unwrap_or("");
    if whole.is_empty()
        || parts.next().is_some()
        || fraction.len() > 2
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|byte| byte.is_ascii_digit())
    {
        return false;
    }
    let digits = format!("{whole}{fraction}");
    let significant = digits.trim_start_matches('0');
    significant.len() <= 3
}

#[cfg(test)]
mod tests {
    use super::{valid_readout, Drag, Frame, Pattern};
    use crate::{automation, multi_edit_controls::Mapping};

    fn control(x: f32, dragging: bool) -> automation::Control {
        automation::Control {
            pointer: Some("/exposure".into()),
            mapping: Mapping::Identity,
            label: "Exposure".into(),
            slider: [0., 0., 112., 20.],
            readout: [120., 0., 180., 20.],
            text: vec![automation::Text {
                value: "Multi".into(),
                rect: [125., 2., 160., 18.],
            }],
            value: 0.,
            range: [0., 1.],
            step: None,
            mixed: true,
            enabled: true,
            visible: true,
            dragging,
            pointer_position: Some([x, 10.]),
            pointer_down: true,
        }
    }

    fn drag() -> Drag {
        let frames = (0..=3)
            .map(|i| Frame {
                time_seconds: i as f64,
                revision: i,
                worker_busy: true,
                panel: Some([0., 0., 200., 200.]),
                control: Some(control(i as f32 * 10., i > 0)),
                values: vec![Some(i as f64 / 10.), Some(0.2 + i as f64 / 10.)],
                rendered: vec![],
            })
            .collect();
        Drag {
            pointer: "/exposure".into(),
            pattern: Pattern::Forward,
            paths: vec![],
            initial: vec![0., 0.2],
            inventory: vec![],
            events: vec![[0., 10.], [30., 10.]],
            frames,
            errors: vec![],
        }
    }

    #[test]
    fn gesture_check_detects_dropped_motion_and_display_regressions() {
        let mut good = drag();
        assert!(good.check().is_ok(), "{:?}", good.errors);
        let mut dropped = drag();
        for frame in &mut dropped.frames[2..] {
            frame.values = vec![Some(0.1), Some(0.3)];
        }
        assert!(dropped
            .check()
            .unwrap_err()
            .contains("pointer movement implies"));
        let mut jitter = drag();
        let control = jitter.frames[2].control.as_mut().unwrap();
        control.readout[2] += 10.;
        control.text[0].value = "-7.85e-4".into();
        let error = jitter.check().unwrap_err();
        assert!(
            error.contains("geometry moved") && error.contains("invalid readout"),
            "{error}"
        );
    }

    #[test]
    fn gesture_check_accepts_a_reversal_and_catches_an_opposite_edit() {
        let mut scrub = drag();
        scrub.pattern = Pattern::Scrub;
        let reversed = Frame {
            time_seconds: 2.5,
            revision: 3,
            worker_busy: true,
            panel: Some([0., 0., 200., 200.]),
            control: Some(control(10., true)),
            values: vec![Some(0.1), Some(0.3)],
            rendered: vec![],
        };
        scrub.frames.insert(3, reversed);
        assert!(scrub.check().is_ok(), "{:?}", scrub.errors);
        scrub.frames[3].values = vec![Some(0.3), Some(0.5)];
        assert!(scrub
            .check()
            .unwrap_err()
            .contains("moved against the drag"));
    }
    #[test]
    fn validates_the_display_contract_without_using_the_formatter() {
        for good in ["Multi", "0.00", "-0.01", "1.23", "123", "1e3", "-1.23e4"] {
            assert!(good == "Multi" || valid_readout(good), "{good}");
        }
        for bad in [
            "-7.85e-4", "0.000785", "1.234", "1234", "abc", "", ".2", "1e", "1.2.3",
        ] {
            assert!(!valid_readout(bad), "{bad}");
        }
    }
}
