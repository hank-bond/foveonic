//! Proves a ready photo can be edited and rendered while another source load is held.

use super::{journey::ExposureJourney, scenarios::apply_exposure, App, Smoke};
use eframe::egui;
use std::path::PathBuf;

pub(super) struct SourceLoading {
    pub model: ExposureJourney,
    stage: usize,
    hold: Option<crate::rendering::worker::LoadHold>,
    previous_revision: u64,
    events: Vec<serde_json::Value>,
}

impl SourceLoading {
    pub fn new(exposures: Vec<f32>) -> Self {
        Self {
            model: ExposureJourney::new(exposures),
            stage: 0,
            hold: None,
            previous_revision: 0,
            events: Vec::new(),
        }
    }

    pub fn evidence(&self) -> serde_json::Value {
        serde_json::json!({"expected":self.model,"events":self.events})
    }

    pub fn release(&mut self) {
        self.hold.take();
    }

    pub fn advance(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        smoke: &mut Smoke,
        paths: &[PathBuf],
    ) -> Result<bool, String> {
        if !smoke.direct_events.is_empty() {
            return Ok(false);
        }
        match self.stage {
            0 => {
                self.model.select(vec![1]);
                app.smoke_multi_click(ctx, smoke, 1, egui::Modifiers::NONE);
                smoke.direct_events.push_back(vec![]);
                crate::automation::reveal(Some("Exposure"));
            }
            1 => {
                if app.view_busy() {
                    return Ok(false);
                }
                if app.selected.as_ref() != Some(&paths[1])
                    || app.edit_selection.paths() != &paths[1..2]
                {
                    return Err("ready photo was not selected".into());
                }
                let Some(control) = crate::automation::snapshot().and_then(|snapshot| {
                    snapshot.controls.into_iter().find(|control| {
                        control.pointer.as_deref() == Some("/exposure")
                            || control.label == "Exposure"
                    })
                }) else {
                    return Ok(false);
                };
                if !control.visible {
                    return Ok(false);
                }
                if !control.enabled {
                    return Err("ready photo's Exposure control is disabled before loading".into());
                }
                if !app.displayed_paths.contains(&paths[0]) || !app.multi_edit.rendering(&paths[0])
                {
                    return Err("loading photo A must remain visible and admitted".into());
                }
                self.hold = Some(app.worker.hold_source(paths[0].clone()));
                // Native viewing keeps A wanted independently of B's editing selection.
                app.viewports.view_mut(Some(&paths[0])).one_to_one = true;
                app.submit_photos();
                self.events.push(serde_json::json!({"action":"request_native_source","photo":0,"consumer":"visible-current","editing_target":false,"zoom_percent":100.}));
            }
            2 => {
                match self.hold.as_ref().unwrap().reached.try_recv() {
                    Ok(()) => {}
                    Err(crossbeam_channel::TryRecvError::Empty) => return Ok(false),
                    Err(_) => return Err("source hold disconnected".into()),
                }
                let control = crate::automation::snapshot()
                    .and_then(|snapshot| {
                        snapshot.controls.into_iter().find(|control| {
                            control.pointer.as_deref() == Some("/exposure")
                                || control.label == "Exposure"
                        })
                    })
                    .ok_or("Exposure control was not observed during loading")?;
                if !control.enabled || !control.visible {
                    return Err(
                        "ready photo's Exposure control is unavailable during loading".into(),
                    );
                }
                self.previous_revision = app
                    .active_image()
                    .filter(|frame| frame.request.path == paths[1])
                    .or_else(|| app.photo_layers(&paths[1])[0])
                    .ok_or("missing ready photo before edit")?
                    .diagnostic_task
                    .revision;
                self.model.adjust_exposure(0.25);
                apply_exposure(app, 0.25);
                self.events.push(serde_json::json!({"observation":"source_held","exposure_control_enabled":control.enabled,"edit_photo":1,"previous_revision":self.previous_revision}));
            }
            3 => {
                let frame = app
                    .active_image()
                    .filter(|frame| frame.request.path == paths[1])
                    .or_else(|| app.photo_layers(&paths[1])[0]);
                let Some(frame) = frame else {
                    return Ok(false);
                };
                if frame.diagnostic_task.revision <= self.previous_revision
                    || frame.request.settings.exposure != self.model.exposures()[1]
                    || !app.worker.is_settled_and_admitted(frame)
                {
                    return Ok(false);
                }
                if !app.view_busy() {
                    return Err("source A stopped being requested before B completed".into());
                }
                self.events.push(serde_json::json!({"observation":"rendered_while_source_held","photo":1,"task":frame.diagnostic_task.id,"revision":frame.diagnostic_task.revision,"exposure":frame.request.settings.exposure}));
                self.release();
                crate::automation::reveal(None);
                self.stage += 1;
                return Ok(true);
            }
            _ => return Ok(true),
        }
        self.stage += 1;
        Ok(false)
    }
}
