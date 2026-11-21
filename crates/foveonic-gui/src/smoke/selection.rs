//! Drives and observes a bounded viewport selection journey during held delivery.

use crossbeam_channel::TryRecvError;
use eframe::egui;
use std::path::PathBuf;

use crate::App;

pub(super) struct SelectionJourney {
    pub(super) model: super::journey::ExposureJourney,
    stage: usize,
    hold: Option<crate::rendering::worker::DeliveryHold>,
    events: Vec<serde_json::Value>,
}

impl SelectionJourney {
    pub(super) fn new(exposures: Vec<f32>) -> Self {
        Self {
            model: super::journey::ExposureJourney::new(exposures),
            stage: 0,
            hold: None,
            events: Vec::new(),
        }
    }

    pub(super) fn advance(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        smoke: &mut super::Smoke,
        paths: &[PathBuf],
    ) -> Result<bool, String> {
        let result = self.advance_inner(app, ctx, smoke, paths);
        if let Err(error) = &result {
            self.events.push(serde_json::json!({"failure": error}));
            self.release();
        }
        result
    }

    pub(super) fn release(&mut self) {
        self.hold.take();
    }

    pub(super) fn evidence(&self) -> serde_json::Value {
        serde_json::json!({"expected": self.model, "events": self.events})
    }

    fn observe(&mut self, app: &App, paths: &[PathBuf]) -> Result<(), String> {
        let selection: Result<Vec<_>, _> = app
            .edit_selection
            .paths()
            .iter()
            .map(|path| {
                paths
                    .iter()
                    .position(|candidate| candidate == path)
                    .ok_or_else(|| {
                        format!("selected photo is outside the journey: {}", path.display())
                    })
            })
            .collect();
        let selection = selection?;
        let exposures: Result<Vec<_>, _> = paths
            .iter()
            .map(|path| {
                let settings = if app.selected.as_ref() == Some(path) {
                    Some(&app.settings)
                } else {
                    app.multi_edit.settings(path)
                }
                .ok_or_else(|| format!("missing settings for {}", path.display()))?;
                if settings.exposure_auto {
                    return Err(format!("automatic Exposure enabled for {}", path.display()));
                }
                Ok(settings.exposure)
            })
            .collect();
        let exposures = exposures?;
        self.events.push(serde_json::json!({
            "stage": self.stage, "observation": {"selection": selection, "exposures": exposures},
            "expected": self.model,
        }));
        self.model.check_selection(&selection)?;
        for path in paths {
            if app.displayed_paths.contains(path)
                && app.multi_edit.settings(path).is_some()
                && !app.multi_edit.rendering(path)
            {
                return Err(
                    "a displayed retained photo left render demand when editing selection changed"
                        .into(),
                );
            }
        }
        self.model.check_exposures(&exposures)
    }

    fn edit(&mut self, app: &mut App, delta: f32) {
        self.model.adjust_exposure(delta);
        self.events
            .push(serde_json::json!({"action": "exposure", "delta": delta}));
        super::scenarios::apply_exposure(app, f64::from(delta));
    }

    fn click(
        &mut self,
        app: &App,
        ctx: &egui::Context,
        smoke: &mut super::Smoke,
        index: usize,
        modifiers: egui::Modifiers,
        selection: Vec<usize>,
    ) {
        self.events.push(serde_json::json!({
            "action": "click", "photo": index, "alt": modifiers.alt, "shift": modifiers.shift,
        }));
        self.model.select(selection);
        app.smoke_multi_click(ctx, smoke, index, modifiers);
        // Smoke advances before drawing. Observe selection on the frame after release is applied.
        smoke.direct_events.push_back(vec![]);
    }

    fn advance_inner(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        smoke: &mut super::Smoke,
        paths: &[PathBuf],
    ) -> Result<bool, String> {
        if paths.len() < 2 || paths.len() != self.model.exposures().len() {
            return Err("journey requires matching paths and at least two exposures".into());
        }
        if !smoke.direct_events.is_empty() {
            return Ok(false);
        }
        match self.stage {
            0 => {
                self.observe(app, paths)?;
                let revision = app.last_request_id.next().0;
                self.hold = Some(app.worker.hold_delivery(paths[0].clone(), revision));
                self.events.push(serde_json::json!({"action": "hold_delivery", "photo": 0, "minimum_revision": revision}));
                self.edit(app, 0.25);
            }
            1 => {
                match self
                    .hold
                    .as_ref()
                    .expect("delivery hold exists")
                    .reached
                    .try_recv()
                {
                    Ok(()) => self
                        .events
                        .push(serde_json::json!({"observation": "delivery_held"})),
                    Err(TryRecvError::Empty) => return Ok(false),
                    Err(TryRecvError::Disconnected) => {
                        return Err("delivery hold disconnected before reaching an image".into())
                    }
                }
                self.observe(app, paths)?;
                self.click(app, ctx, smoke, 0, egui::Modifiers::NONE, vec![0]);
            }
            2 => {
                self.observe(app, paths)?;
                self.edit(app, -0.5);
                self.click(app, ctx, smoke, 1, egui::Modifiers::ALT, vec![0, 1]);
            }
            3 => {
                self.observe(app, paths)?;
                self.edit(app, 0.125);
                self.click(app, ctx, smoke, 0, egui::Modifiers::ALT, vec![1]);
            }
            4 => {
                self.observe(app, paths)?;
                self.click(app, ctx, smoke, 0, egui::Modifiers::ALT, vec![0, 1]);
            }
            5 => {
                self.observe(app, paths)?;
                self.edit(app, -0.125);
                self.click(app, ctx, smoke, 0, egui::Modifiers::NONE, vec![0]);
            }
            6 => {
                self.observe(app, paths)?;
                let (first, last) = app.smoke_visual_range(paths);
                // Stage five already clicked photo zero. Repeating that click
                // quickly would deliberately request double-click zoom.
                if first != 0 {
                    self.click(app, ctx, smoke, first, egui::Modifiers::NONE, vec![first]);
                }
                self.click(
                    app,
                    ctx,
                    smoke,
                    last,
                    egui::Modifiers::SHIFT,
                    (0..paths.len()).collect(),
                );
            }
            7 => {
                self.observe(app, paths)?;
                self.release();
                self.events
                    .push(serde_json::json!({"action": "release_delivery"}));
                self.stage += 1;
                return Ok(true);
            }
            _ => return Ok(true),
        }
        self.stage += 1;
        Ok(false)
    }
}
