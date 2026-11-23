//! Applies queued editor actions through shared recipe scopes and the existing editing session.

use super::{menus::Action, Scope};
use crate::{
    multi_edit_controls::{field, Edit},
    multi_edit_runtime::Change,
    App,
};
use foveonic::render::camera_settings;

impl App {
    pub(crate) fn apply_editor_actions(&mut self, ctx: &eframe::egui::Context) -> bool {
        if self.editor_ui.actions.is_empty() {
            return false;
        }
        if !self.selection_loaded() {
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
                    let rendering = (scope != Scope::All).then_some(settings.rendering);
                    match camera_settings(&metadata, rendering) {
                        Ok(baseline) => {
                            let baseline = if camera {
                                baseline
                            } else {
                                self.preferences.editor.defaults.apply(family, &baseline)
                            };
                            if matches!(
                                scope,
                                Scope::All | Scope::Tool(super::Section::Corrections)
                            ) {
                                super::dehaze::exit(ctx);
                            }
                            Some(Change::Scope {
                                scope,
                                source: Box::new(baseline),
                            })
                        }
                        Err(error) => {
                            self.fail(error.to_string());
                            None
                        }
                    }
                }
                Action::Apply(scope, source) => Some(Change::Scope { scope, source }),
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
                changed |= self.edit_selected(change);
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
}
