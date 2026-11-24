//! Applies queued editor actions through shared recipe scopes and the existing editing session.

use super::{menus::Action, recipes::Defaults, Scope};
use crate::{
    multi_edit_controls::{field, Edit},
    multi_edit_runtime::{Change, Runtime},
    App,
};
use foveonic::{
    render::{camera_settings, Settings},
    source::ProcessingFamily,
};
use std::path::PathBuf;

impl App {
    pub(crate) fn apply_editor_actions(&mut self, ctx: &eframe::egui::Context) -> bool {
        if self.editor_ui.actions.is_empty() {
            return false;
        }
        // The menu edits the editing selection under the panel's rule: every photo is ready.
        if !self.selection_loaded() || !self.multi_edit.editable_ready() {
            self.editor_ui.actions.clear();
            return false;
        }
        let (Some(metadata), Some(family)) = (
            self.selected_metadata(),
            self.selected
                .as_deref()
                .and_then(|path| self.multi_edit.family(path)),
        ) else {
            self.editor_ui.actions.clear();
            return false;
        };
        let previous_defaults = self.preferences.editor.defaults.clone();
        let mut changed = false;
        for action in std::mem::take(&mut self.editor_ui.actions) {
            let settings = self
                .selected_settings()
                .expect("editor actions wait for loaded settings");
            let change = match action {
                Action::NoiseCalibration => {
                    self.noise_calibration.open = true;
                    None
                }
                Action::PreventSaturationClipping(enabled) => {
                    Some(Change::Fields(vec![Edit::set(
                        field::PREVENT_COLOR_CLIPPING,
                        &enabled,
                    )]))
                }
                Action::AdaptiveLimits => {
                    crate::application_settings::open_adaptive(ctx);
                    None
                }
                Action::ExposureAutoCompensation(choice) => Some(Change::Fields(vec![Edit::set(
                    field::EXPOSURE_AUTO_COMPENSATION,
                    &choice,
                )])),
                Action::Reset(scope, camera) => {
                    if matches!(scope, Scope::All | Scope::Tool(super::Section::Corrections)) {
                        super::dehaze::exit(ctx);
                    }
                    changed |= self.reset_editing(scope, camera);
                    None
                }
                Action::Apply(scope, source) => {
                    // The menu lists presets for the selected photo's camera family.
                    let paths =
                        preset_targets(&self.multi_edit, self.edit_selection.paths(), family);
                    changed |= self.edit_paths(&paths, &Change::Scope { scope, source });
                    None
                }
                Action::Save(scope) => {
                    let settings = settings.clone();
                    self.editor_ui.start_save(family, scope, &settings);
                    None
                }
                Action::Default(scope) => {
                    let settings = settings.clone();
                    self.preferences
                        .editor
                        .defaults
                        .set(family, scope, &settings);
                    None
                }
                Action::ClearDefaults => {
                    self.preferences.editor.defaults.clear(family);
                    None
                }
                Action::Delete(index) => {
                    self.preferences.editor.presets.remove(index);
                    None
                }
            };
            if let Some(change) = change {
                changed |= self.edit_editing(&change);
            }
        }
        if let Some(settings) = self.selected_settings().cloned() {
            crate::inspect::normalize(
                &settings,
                &mut self.output,
                &mut self.channel,
                Some(&metadata),
                Some(family),
            );
        }
        if previous_defaults != self.preferences.editor.defaults {
            let defaults = &self.preferences.editor.defaults;
            self.sidecars.set_defaults(defaults);
            self.worker.cache.configure_photo_defaults(defaults);
            self.import.set_defaults(
                defaults,
                &mut self.browser,
                self.preferences.processing_destination.as_deref(),
            );
            self.browser.invalidate_previews();
            self.previews.invalidate();
        }
        changed
    }

    /// Reset every editing photo to its own camera values or editor defaults.
    fn reset_editing(&mut self, scope: Scope, camera: bool) -> bool {
        let paths = self.edit_selection.paths().to_vec();
        let defaults = (!camera).then_some(&self.preferences.editor.defaults);
        let baselines = match reset_baselines(&self.multi_edit, defaults, &paths, scope) {
            Ok(baselines) => baselines,
            Err(error) => {
                self.fail(error.to_string());
                return false;
            }
        };
        let mut changed = false;
        for (path, baseline) in paths.iter().zip(baselines) {
            let change = Change::Scope {
                scope,
                source: Box::new(baseline),
            };
            changed |= self.edit_paths(std::slice::from_ref(path), &change);
        }
        changed
    }
}

/// Each photo's reset baseline: its own camera values, under the editor
/// `defaults` when given. A scoped reset keeps the photo's rendering choice.
/// Empty unless every photo has been rendered.
fn reset_baselines(
    photos: &Runtime,
    defaults: Option<&Defaults>,
    paths: &[PathBuf],
    scope: Scope,
) -> foveonic::source::Result<Vec<Settings>> {
    let mut baselines = Vec::with_capacity(paths.len());
    for path in paths {
        let (Some(metadata), Some(family), Some(settings)) = (
            photos.metadata(path),
            photos.family(path),
            photos.settings(path),
        ) else {
            return Ok(Vec::new());
        };
        let rendering = (scope != Scope::All).then_some(settings.rendering);
        let camera = camera_settings(&metadata, rendering)?;
        baselines.push(match defaults {
            Some(defaults) => defaults.apply(family, &camera),
            None => camera,
        });
    }
    Ok(baselines)
}

/// The photos a preset made for `family` applies to.
fn preset_targets(photos: &Runtime, paths: &[PathBuf], family: ProcessingFamily) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|path| photos.family(path) == Some(family))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "actions_test.rs"]
mod tests;
