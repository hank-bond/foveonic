//! Exercises bounded editing in a large folder of explicit source aliases.

use super::App;
use crate::{preferences::FolderMode, shortcuts::BrowserAction};
use eframe::egui;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::mpsc, thread};

const COUNT: usize = 100;
const LIMIT: usize = 8;

#[derive(Clone, Copy)]
enum Phase {
    Prepare,
    Staging,
    Folder,
    Selected,
    Edited,
    SwitchedAway,
    SwitchedBack,
    OriginalFolder,
    OriginalSelected,
    Done,
}

pub(super) struct CollectionJourney {
    phase: Phase,
    original: Vec<PathBuf>,
    exposures: Vec<f32>,
    directory: PathBuf,
    aliases: Vec<PathBuf>,
    pending: Option<mpsc::Receiver<Result<Vec<PathBuf>, String>>>,
    events: Vec<Value>,
}

impl CollectionJourney {
    pub(super) fn new(original: Vec<PathBuf>, exposures: Vec<f32>, directory: PathBuf) -> Self {
        Self {
            phase: Phase::Prepare,
            original,
            exposures,
            directory,
            aliases: Vec::new(),
            pending: None,
            events: Vec::new(),
        }
    }

    pub(super) fn advance(&mut self, app: &mut App, ctx: &egui::Context) -> Result<bool, String> {
        match self.phase {
            Phase::Prepare => {
                if self.original.len() != LIMIT || self.exposures.len() != LIMIT {
                    return Err(
                        "collection journey requires eight declared manual-exposure inputs".into(),
                    );
                }
                if self
                    .exposures
                    .iter()
                    .any(|v| !v.is_finite() || *v + 0.125 > 4.)
                {
                    return Err("collection starting exposures must accept +0.125".into());
                }
                let original = self.original.clone();
                let directory = self.directory.clone();
                let (send, receive) = mpsc::channel();
                thread::spawn(move || {
                    let _ = send.send(stage(&original, &directory));
                });
                self.pending = Some(receive);
                self.phase = Phase::Staging;
            }
            Phase::Staging => {
                match self.pending.as_ref().unwrap().try_recv() {
                    Ok(result) => self.aliases = result?,
                    Err(mpsc::TryRecvError::Empty) => return Ok(false),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("collection staging worker stopped".into())
                    }
                }
                self.pending = None;
                self.events.push(json!({"action":"stage", "aliases":self.aliases.iter().enumerate().map(|(i,path)| json!({"path":path,"original_source_index":i % LIMIT})).collect::<Vec<_>>(), "independent_originals":LIMIT}));
                app.preferences.multi_edit_limit = LIMIT;
                app.open(self.directory.clone(), FolderMode::Import);
                self.phase = Phase::Folder;
            }
            Phase::Folder => {
                if app.folder_loader.loading()
                    || app.browser.root.as_ref() != Some(&self.directory)
                    || !app.browser.import_mode
                    || app.view_busy()
                    || !app.sidecars.ready()
                {
                    return Ok(false);
                }
                if app.browser.files != self.aliases {
                    return Err("discovered collection does not match the 100 declared aliases in name order".into());
                }
                app.browser
                    .set_actions(vec![(BrowserAction::SelectAll, egui::Modifiers::COMMAND)]);
                self.phase = Phase::Selected;
            }
            Phase::Selected => {
                if app.browser.processing_selection(app.selected.as_deref()) != self.aliases
                    || app.edit_selection.paths() != &self.aliases[..LIMIT]
                    || app.view_busy()
                    || !app.multi_edit.editable_ready()
                {
                    return Ok(false);
                }
                if !frames_ready(app, &self.aliases[..LIMIT], &self.exposures) {
                    return Ok(false);
                }
                // Viewport photos outside the editing targets render too; photos
                // past the viewport's tiles do not.
                let planned = app.planned_photos();
                if let Some((path, frame)) = self.aliases[LIMIT..]
                    .iter()
                    .filter(|path| !planned.contains(path))
                    .find_map(|path| app.photo_layers(path)[0].map(|frame| (path, frame)))
                {
                    return Err(format!(
                        "unexpected frame for a photo the viewport does not draw: {} request {:?} scale {}",
                        path.display(),
                        frame.request.id,
                        frame.artifact.settings.working_scale.divisor()
                    ));
                }
                let progress = app
                    .worker
                    .progress
                    .snapshot()
                    .map(|p| json!({"fraction":p.fraction,"running":p.running()}));
                self.events.push(json!({"step":"bounded-selection", "browser_count":app.browser.processing_selection(app.selected.as_deref()).len(), "edit_count":app.edit_selection.paths().len(), "outside_edit_count":COUNT-LIMIT, "target_frames":LIMIT, "progress":progress}));
                super::scenarios::apply_exposure(app, 0.125);
                self.phase = Phase::Edited;
            }
            Phase::Edited => {
                if app.view_busy() || !app.multi_edit.editable_ready() {
                    return Ok(false);
                }
                let edited: Vec<_> = self.exposures.iter().map(|v| v + 0.125).collect();
                if !frames_ready(app, &self.aliases[..LIMIT], &edited) {
                    return Ok(false);
                }
                self.events
                    .push(json!({"step":"edited", "expected_exposures":edited}));
                app.select(self.aliases[1].clone(), true);
                self.phase = Phase::SwitchedAway;
            }
            Phase::SwitchedAway => {
                if app.view_busy() || app.selected.as_ref() != Some(&self.aliases[1]) {
                    return Ok(false);
                }
                app.select(self.aliases[0].clone(), true);
                self.phase = Phase::SwitchedBack;
            }
            Phase::SwitchedBack => {
                if app.view_busy() || app.selected.as_ref() != Some(&self.aliases[0]) {
                    return Ok(false);
                }
                let edited: Vec<_> = self.exposures.iter().map(|v| v + 0.125).collect();
                if app.edit_selection.paths() != &self.aliases[..LIMIT]
                    || !frames_ready(app, &self.aliases[..LIMIT], &edited)
                {
                    return Ok(false);
                }
                self.events.push(json!({"step":"switch-retained", "browser_count":app.browser.processing_selection(app.selected.as_deref()).len(), "edit_count":app.edit_selection.paths().len()}));
                let original_directory = self.original[0]
                    .parent()
                    .ok_or("original folder missing")?
                    .to_owned();
                app.open(original_directory, FolderMode::Import);
                self.phase = Phase::OriginalFolder;
            }
            Phase::OriginalFolder => {
                if app.folder_loader.loading()
                    || app.browser.files != self.original
                    || app.view_busy()
                    || !app.sidecars.ready()
                {
                    return Ok(false);
                }
                app.browser
                    .set_actions(vec![(BrowserAction::SelectAll, egui::Modifiers::COMMAND)]);
                self.phase = Phase::OriginalSelected;
            }
            Phase::OriginalSelected => {
                if app.edit_selection.paths() != self.original
                    || app.view_busy()
                    || !app.multi_edit.editable_ready()
                {
                    return Ok(false);
                }
                if !frames_ready(app, &self.original, &self.exposures) {
                    return Ok(false);
                }
                self.events.push(
                    json!({"step":"original-selection-restored", "count":self.original.len()}),
                );
                self.phase = Phase::Done;
                return Ok(true);
            }
            Phase::Done => return Ok(true),
        }
        ctx.request_repaint();
        Ok(false)
    }
    pub(super) fn evidence(&self) -> Value {
        json!({"kind":"hard-linked-aliases", "events":self.events})
    }
}

fn stage(original: &[PathBuf], directory: &PathBuf) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir(directory).map_err(|e| format!("create collection: {e}"))?;
    let mut aliases = Vec::with_capacity(COUNT);
    for index in 0..COUNT {
        let source = &original[index % original.len()];
        let alias = directory.join(format!("{index:03}-alias.x3f"));
        std::fs::hard_link(source, &alias).map_err(|e| format!("link collection alias: {e}"))?;
        let source_sidecar = foveonic::sidecar::path(source);
        let target_sidecar = foveonic::sidecar::path(&alias);
        if !source_sidecar.is_file() {
            return Err(format!("missing manual recipe for {}", source.display()));
        }
        std::fs::create_dir_all(target_sidecar.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::copy(source_sidecar, target_sidecar)
            .map_err(|e| format!("copy collection recipe: {e}"))?;
        aliases.push(alias);
    }
    Ok(aliases)
}

fn frames_ready(app: &App, paths: &[PathBuf], exposures: &[f32]) -> bool {
    paths.iter().zip(exposures).all(|(path, expected)| {
        let frame = app
            .active_image()
            .filter(|frame| frame.request.path == *path)
            .or_else(|| app.photo_layers(path)[0]);
        let settings = app
            .multi_edit
            .settings(path)
            .or_else(|| (app.selected.as_ref() == Some(path)).then_some(&app.settings));
        frame.is_some_and(|frame| {
            frame.request.rendition == crate::editor::Rendition::Current
                && app.shows_current_view(frame)
                && frame.request.settings.exposure == *expected
                && !frame.artifact.settings.exposure_auto
                && frame.artifact.settings.exposure == *expected
        }) && settings
            .is_some_and(|settings| !settings.exposure_auto && settings.exposure == *expected)
    })
}
