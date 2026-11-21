//! Drives a foreground exposure edit after a real import batch starts.

use std::path::{Path, PathBuf};

use eframe::egui;
use serde_json::{json, Value};

use super::{journey::ExposureJourney, App};

#[derive(Clone, Copy)]
enum Phase {
    Prepare,
    Scan,
    Batch,
    Done,
}

pub(super) struct BackgroundEdit {
    pub(super) model: ExposureJourney,
    phase: Phase,
    initial_exposures: Vec<f32>,
    at_edit: Option<Value>,
}

impl BackgroundEdit {
    pub(super) fn new(exposures: Vec<f32>) -> Self {
        Self {
            model: ExposureJourney::new(exposures.clone()),
            phase: Phase::Prepare,
            initial_exposures: exposures,
            at_edit: None,
        }
    }

    pub(super) fn advance(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        paths: &[PathBuf],
        destination: &Path,
    ) -> Result<bool, String> {
        match self.phase {
            Phase::Prepare => {
                if paths.is_empty() || paths.len() != self.initial_exposures.len() {
                    return Err(
                        "background edit requires one starting exposure per selected photo".into(),
                    );
                }
                if app.edit_selection.paths() != paths {
                    return Err(
                        "background edit requires all declared photos selected for editing".into(),
                    );
                }
                let mut observed = Vec::with_capacity(paths.len());
                for path in paths {
                    let settings = app
                        .multi_edit
                        .settings(path)
                        .or_else(|| (app.selected.as_ref() == Some(path)).then_some(&app.settings))
                        .ok_or_else(|| {
                            format!("missing starting settings for {}", path.display())
                        })?;
                    if settings.exposure_auto {
                        return Err(format!("manual exposure required for {}", path.display()));
                    }
                    observed.push(settings.exposure);
                }
                self.model.check_exposures(&observed)?;
                if observed
                    .iter()
                    .any(|value| !value.is_finite() || *value < -4.0 || *value + 0.25 > 4.0)
                {
                    return Err("starting exposure cannot accept the +0.25 edit".into());
                }
                if !app.browser.import_mode {
                    return Err("background edit requires import mode".into());
                }
                crate::automation::reveal(Some("/exposure"));
                app.browser.hide_reviewed = false;
                app.browser
                    .marks
                    .insert(paths[0].clone(), crate::browser::Mark::Pick);
                app.browser.refresh_filter();
                app.preferences.processing_destination = Some(destination.to_owned());
                app.preferences.import_processing.copy_only = false;
                app.preferences.import_processing.format = crate::export::ExportFormat::Tiff;
                app.import.rescan(&mut app.browser, Some(destination));
                self.phase = Phase::Scan;
            }
            Phase::Scan => {
                if app.import.ready(&app.browser) {
                    let Some(exposure) = crate::automation::snapshot().and_then(|snapshot| {
                        snapshot
                            .controls
                            .into_iter()
                            .find(|control| control.pointer.as_deref() == Some("/exposure"))
                    }) else {
                        return Ok(false);
                    };
                    if !exposure.visible {
                        return Ok(false);
                    }
                    if !exposure.enabled {
                        return Err(
                            "Exposure control is disabled before background processing".into()
                        );
                    }
                    app.process();
                    let progress = app.import.smoke_batch_progress();
                    if !progress.running || progress.total != 1 || !progress.errors.is_empty() {
                        return Err("could not submit the single-photo background import".into());
                    }
                    self.phase = Phase::Batch;
                }
            }
            Phase::Batch => {
                let progress = app.import.smoke_batch_progress();
                if !progress.running || !progress.errors.is_empty() {
                    return Err(format!(
                        "background import finished or failed before the edit: {:?}",
                        progress.errors
                    ));
                }
                if progress.decisions_saved {
                    let observation = crate::automation::snapshot()
                        .ok_or("background edit control observation is missing")?;
                    let exposure = observation
                        .controls
                        .iter()
                        .find(|control| control.pointer.as_deref() == Some("/exposure"))
                        .ok_or("Exposure control is missing during background processing")?;
                    if !exposure.enabled || !exposure.visible {
                        return Err(
                            "Exposure control is unavailable during background processing".into(),
                        );
                    }
                    self.at_edit = Some(json!({
                        "running": progress.running,
                        "decisions_saved": progress.decisions_saved,
                        "total": progress.total,
                        "completed": progress.completed,
                        "errors": progress.errors,
                        "destination": progress.destination,
                        "processing":app.preferences.import_processing,
                        "exposure_control":exposure,
                        "expected_export_extent":app.photo_layers(&paths[0])[0]
                            .or_else(||app.active_image().filter(|frame|frame.request.path == paths[0]))
                            .map(|frame|frame.display_extent(false)),
                    }));
                    self.model.adjust_exposure(0.25);
                    foveonic::diagnostics::event(
                        foveonic::diagnostics::Category::Scenario,
                        "scenario.background_edit",
                        |fields| {
                            fields.bool("batch_started", true);
                        },
                    );
                    super::scenarios::apply_exposure(app, 0.25);
                    crate::automation::reveal(None);
                    self.phase = Phase::Done;
                    return Ok(true);
                }
            }
            Phase::Done => return Ok(true),
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(20));
        Ok(false)
    }

    pub(super) fn evidence(&self) -> Value {
        json!({"initial_exposures": self.initial_exposures,"expected":self.model,
               "command": {"exposure_delta": 0.25},
               "background_at_edit": self.at_edit})
    }
}
