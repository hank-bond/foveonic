//! Proves a ready photo can be edited and rendered while another source load is held.

use super::{journey::ExposureJourney, scenarios::apply_exposure, App, Smoke};
use crate::{editor::Rendition, rendering::request::Request};
use eframe::egui;
use foveonic::render::ProcessingQuality;
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
                if app.worker.busy() {
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
                self.hold = Some(app.worker.hold_source(paths[0].clone()));
                app.revision += 1;
                // Native Camera demand isolates this diagnostic source request
                // from ordinary Current snapshots. This is not a Before UI test.
                // B keeps its reduced session and active controls.
                app.worker.submit(Request {
                    auxiliary_preview: false,

                    detail: false,
                    path: paths[0].clone(),
                    revision: app.revision,
                    settings: app
                        .multi_edit
                        .settings(&paths[0])
                        .ok_or("missing photo A recipe")?
                        .clone(),
                    rendition: Rendition::Camera,
                    output: "final".into(),
                    comparison: false,
                    framing_preview: false,
                    calibration: app
                        .noise_calibration
                        .selected(app.preferences.noise_profile),
                    preview_scale: foveonic::render::Scale::NATIVE,
                    processing_quality: ProcessingQuality::Quality,
                    export: None,
                });
                self.events
                    .push(serde_json::json!({"action":"request_native_source","photo":0}));
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
                    .frame
                    .as_ref()
                    .filter(|frame| frame.path == paths[1])
                    .or_else(|| app.multi_edit_frame(&paths[1]))
                    .ok_or("missing ready photo before edit")?
                    .diagnostic_task
                    .revision;
                self.model.adjust_exposure(0.25);
                apply_exposure(app, 0.25);
                self.events.push(serde_json::json!({"observation":"source_held","exposure_control_enabled":control.enabled,"edit_photo":1,"previous_revision":self.previous_revision}));
            }
            3 => {
                let frame = app
                    .frame
                    .as_ref()
                    .filter(|frame| frame.path == paths[1])
                    .or_else(|| app.multi_edit_frame(&paths[1]));
                let Some(frame) = frame else {
                    return Ok(false);
                };
                if frame.diagnostic_task.revision <= self.previous_revision
                    || frame.requested.exposure != self.model.exposures()[1]
                    || !app.worker.is_settled_and_admitted(frame)
                {
                    return Ok(false);
                }
                if !app.worker.busy() {
                    return Err("source A stopped being requested before B completed".into());
                }
                self.events.push(serde_json::json!({"observation":"rendered_while_source_held","photo":1,"task":frame.diagnostic_task.id,"revision":frame.diagnostic_task.revision,"exposure":frame.requested.exposure}));
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
