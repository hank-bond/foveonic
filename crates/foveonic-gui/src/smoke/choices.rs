//! Drives the shared white-balance menu and Tone bypass through egui input.

use crate::{automation, preferences::EditingSection, App};
use eframe::egui;
use foveonic::render::Settings;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Copy)]
enum Phase {
    OpenWhiteBalance,
    ChooseSunlight,
    AwaitWhiteBalance,
    DisableTone,
    AwaitTone,
    Done,
}

pub(super) struct ChoiceJourney {
    initial: Vec<(String, f64)>,
    phase: Phase,
    frames_waited: usize,
    events: Vec<serde_json::Value>,
}
impl ChoiceJourney {
    pub(super) fn new(inputs: &[serde_json::Value]) -> Result<Self, String> {
        let initial: Result<Vec<_>, String> = inputs
            .iter()
            .map(|input| {
                let settings = &input["settings"];
                let wb = settings["white_balance"]
                    .as_str()
                    .ok_or("fixture needs explicit white_balance")?
                    .to_owned();
                let contrast = settings["tone"]["contrast"]
                    .as_f64()
                    .ok_or("fixture needs explicit tone.contrast")?;
                if settings["tone_enabled"] != true || contrast != 0.25 {
                    return Err(
                        "fixture needs tone_enabled=true and tone.contrast=0.25 for every photo"
                            .into(),
                    );
                }
                Ok((wb, contrast))
            })
            .collect();
        let initial = initial?;
        if initial.len() < 2
            || !initial.iter().any(|(wb, _)| wb == "Sunlight")
            || !initial.iter().any(|(wb, _)| wb == "Incandescent")
            || initial
                .iter()
                .any(|(wb, _)| wb != "Sunlight" && wb != "Incandescent")
        {
            return Err("fixture needs at least two photos with explicit Sunlight and Incandescent white balance".into());
        }
        Ok(Self {
            initial,
            phase: Phase::OpenWhiteBalance,
            frames_waited: 0,
            events: Vec::new(),
        })
    }
    pub(super) fn evidence(&self) -> serde_json::Value {
        serde_json::json!({"initial":self.initial,"events":self.events})
    }
    fn wait(&mut self, message: &str) -> Result<bool, String> {
        self.frames_waited += 1;
        if self.frames_waited > 120 {
            Err(format!(
                "{message}; observations: {:?}",
                automation::snapshot()
            ))
        } else {
            Ok(false)
        }
    }
    fn next(&mut self, phase: Phase) {
        self.phase = phase;
        self.frames_waited = 0;
    }
    fn find<'a>(
        snapshot: &'a automation::Snapshot,
        kind: &str,
        pointer: Option<&str>,
        label: &str,
    ) -> Option<&'a automation::Choice> {
        snapshot.choices.iter().find(|choice| {
            choice.kind == kind
                && choice.pointer.as_deref() == pointer
                && choice.label == label
                && choice.visible
                && choice.enabled
        })
    }
    fn click(smoke: &mut super::Smoke, rect: [f32; 4]) {
        let pos = egui::pos2((rect[0] + rect[2]) * 0.5, (rect[1] + rect[3]) * 0.5);
        for pressed in [true, false] {
            smoke.direct_events.push_back(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        smoke.direct_events.push_back(vec![]);
    }
    fn frame<'a>(
        app: &'a App,
        path: &PathBuf,
    ) -> Option<&'a crate::rendering::worker::RenderResult> {
        app.active_image()
            .filter(|frame| frame.request.path == *path)
            .or_else(|| app.photo_layers(path)[0])
    }
    pub(super) fn advance(
        &mut self,
        app: &mut App,
        smoke: &mut super::Smoke,
        paths: &[PathBuf],
        expected: &mut BTreeMap<PathBuf, Settings>,
    ) -> Result<bool, String> {
        if !smoke.direct_events.is_empty() {
            return Ok(false);
        }
        if app.edit_selection.paths() != paths {
            return Err("choice journey lost all-photo editing selection".into());
        }
        let snapshot = automation::snapshot().ok_or("choice observation is not enabled")?;
        match self.phase {
            Phase::OpenWhiteBalance => {
                if app.view_busy() || !app.multi_edit.editable_ready() {
                    return Ok(false);
                }
                for (index, path) in paths.iter().enumerate() {
                    let settings = app
                        .multi_edit
                        .settings(path)
                        .ok_or("missing starting settings")?;
                    if settings.white_balance != self.initial[index].0
                        || !settings.tone_enabled
                        || settings.tone.contrast != self.initial[index].1
                        || !settings.white_balance_bias.is_neutral()
                    {
                        return Err(format!("fixture settings disagree for {}: require declared white balance, enabled Tone, contrast 0.25 and neutral bias", path.display()));
                    }
                    let frame = Self::frame(app, path).ok_or("missing initial frame")?;
                    let presets = frame.artifact.metadata.white_balance_presets();
                    if !presets.contains(&"Sunlight") || !presets.contains(&"Incandescent") {
                        return Err(format!("fixture {} lacks Sunlight or Incandescent white balance preset: {presets:?}", path.display()));
                    }
                }
                automation::reveal(Some("/white_balance"));
                let Some(dropdown) = Self::find(&snapshot, "dropdown", Some("/white_balance"), "")
                else {
                    return self.wait("White Balance dropdown unavailable");
                };
                if !dropdown.text.iter().any(|text| text.value == "Multi") {
                    return Err(format!(
                        "mixed White Balance must paint Multi: {dropdown:?}"
                    ));
                }
                self.events.push(
                    serde_json::json!({"observation":"mixed White Balance","control":dropdown}),
                );
                Self::click(smoke, dropdown.rect);
                self.next(Phase::ChooseSunlight);
            }
            Phase::ChooseSunlight => {
                let Some(item) = Self::find(&snapshot, "menu", Some("/white_balance"), "Sunlight")
                else {
                    return self.wait("Sunlight menu item unavailable for all selected cameras");
                };
                if !item.text.iter().any(|text| text.value == "Sunlight") {
                    return Err(format!("Sunlight menu label not painted: {item:?}"));
                }
                for path in paths {
                    let settings = expected.get_mut(path).ok_or("missing expected settings")?;
                    settings.white_balance = "Sunlight".into();
                    settings.white_balance_bias = Default::default();
                }
                self.events.push(serde_json::json!({"action":"choose Sunlight","coverage":"egui-input","control":item}));
                Self::click(smoke, item.rect);
                automation::reveal(None);
                self.next(Phase::AwaitWhiteBalance);
            }
            Phase::AwaitWhiteBalance => {
                if app.view_busy() {
                    return Ok(false);
                }
                for path in paths {
                    let settings = app
                        .multi_edit
                        .settings(path)
                        .ok_or("missing White Balance settings")?;
                    if settings.white_balance != "Sunlight" {
                        return self.wait("selected Sunlight did not reach every photo");
                    }
                    let Some(frame) = Self::frame(app, path) else {
                        return Ok(false);
                    };
                    if frame.request.rendition != crate::editor::Rendition::Current
                        || frame.request.settings.white_balance != "Sunlight"
                        || frame.artifact.settings.white_balance != "Sunlight"
                    {
                        return Ok(false);
                    }
                }
                app.preferences
                    .editing_sections
                    .retain(|section| *section != EditingSection::Tone);
                app.preferences
                    .editing_sections
                    .insert(0, EditingSection::Tone);
                automation::reveal(Some("Tone"));
                self.next(Phase::DisableTone);
            }
            Phase::DisableTone => {
                let Some(check) = Self::find(&snapshot, "checkbox", None, "Tone") else {
                    return self.wait("Tone checkbox unavailable");
                };
                if check.checked != Some(true)
                    || check.painted_checked != Some(true)
                    || !check.text.iter().any(|text| text.value == "Tone")
                {
                    return Err(format!(
                        "Tone must paint a checked checkbox and label: {check:?}"
                    ));
                }
                for path in paths {
                    expected
                        .get_mut(path)
                        .ok_or("missing expected settings")?
                        .tone_enabled = false;
                }
                self.events.push(serde_json::json!({"action":"disable Tone","coverage":"egui-input","control":check}));
                Self::click(smoke, check.rect);
                automation::reveal(None);
                self.next(Phase::AwaitTone);
            }
            Phase::AwaitTone => {
                let Some(check) = Self::find(&snapshot, "checkbox", None, "Tone") else {
                    return self.wait("Tone checkbox disappeared");
                };
                if check.checked != Some(false)
                    || check.painted_checked != Some(false)
                    || !check.text.iter().any(|text| text.value == "Tone")
                {
                    return self.wait("Tone checkbox did not paint unchecked");
                }
                if app.view_busy() {
                    return Ok(false);
                }
                for path in paths {
                    let settings = app
                        .multi_edit
                        .settings(path)
                        .ok_or("missing final settings")?;
                    if settings.tone_enabled
                        || settings.tone.contrast != 0.25
                        || settings.white_balance != "Sunlight"
                    {
                        return self.wait("final choice settings did not reach every photo");
                    }
                    let Some(frame) = Self::frame(app, path) else {
                        return Ok(false);
                    };
                    if frame.request.rendition != crate::editor::Rendition::Current
                        || frame.request.settings.tone_enabled
                        || frame.request.settings.tone.contrast != 0.25
                        || frame.artifact.settings.tone_enabled
                        || frame.artifact.settings.white_balance != "Sunlight"
                    {
                        return Ok(false);
                    }
                }
                self.events.push(serde_json::json!({"observation":"Tone unchecked and final images current","control":check}));
                self.next(Phase::Done);
            }
            Phase::Done => return Ok(true),
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(wb: &str, contrast: f64) -> serde_json::Value {
        serde_json::json!({"settings":{"white_balance":wb,"tone_enabled":true,"tone":{"contrast":contrast}}})
    }
    #[test]
    fn choice_fixtures_require_mixed_explicit_values_and_visible_tone_effect() {
        assert!(
            ChoiceJourney::new(&[fixture("Sunlight", 0.25), fixture("Incandescent", 0.25)]).is_ok()
        );
        assert!(
            ChoiceJourney::new(&[fixture("Sunlight", 0.25), fixture("Sunlight", 0.25)]).is_err()
        );
        assert!(
            ChoiceJourney::new(&[fixture("Sunlight", 0.25), fixture("Incandescent", 0.)]).is_err()
        );
    }
}
