//! Applies queued editor actions through shared recipe scopes and the existing editing session.

use super::{menus::Action, Scope};
use crate::App;
use foveonic::render::{camera_settings, ControlCapabilities};

impl App {
    pub(crate) fn apply_editor_actions(&mut self, ctx: &eframe::egui::Context) -> bool {
        if self.editor_ui.actions.is_empty() {
            return false;
        }
        if self.exporting || !self.sidecars.ready() {
            self.editor_ui.actions.clear();
            return false;
        }
        let Some(metadata) = self.metadata.as_deref() else {
            self.editor_ui.actions.clear();
            return false;
        };
        let Ok(capabilities) = ControlCapabilities::from_metadata(metadata) else {
            self.editor_ui.actions.clear();
            return false;
        };
        let family = capabilities.family;
        let before = self.settings.clone();
        let previous_defaults = self.preferences.editor.defaults.clone();
        for action in std::mem::take(&mut self.editor_ui.actions) {
            match action {
                Action::NoiseCalibration => self.noise_calibration.open = true,
                Action::PreventSaturationClipping(enabled) => {
                    self.settings.prevent_color_clipping = enabled
                }
                Action::AdaptiveLimits => crate::application_settings::open_adaptive(ctx),
                Action::ExposureAutoCompensation(choice) => {
                    self.settings.exposure_auto_compensation = choice;
                }
                Action::Reset(scope, camera) => {
                    let rendering = (scope != Scope::All).then_some(self.settings.rendering);
                    match camera_settings(metadata, rendering) {
                        Ok(baseline) => {
                            let baseline = if camera {
                                baseline
                            } else {
                                self.preferences.editor.defaults.apply(family, &baseline)
                            };
                            scope.apply(&baseline, &mut self.settings, family);
                            if matches!(
                                scope,
                                Scope::All | Scope::Tool(super::Section::Corrections)
                            ) {
                                super::dehaze::exit(ctx);
                            }
                        }
                        Err(error) => {
                            self.status = error.to_string();
                            self.status_error = true;
                        }
                    }
                }
                Action::Apply(scope, settings) => {
                    scope.apply(&settings, &mut self.settings, family)
                }
                Action::Save(scope) => self.editor_ui.start_save(family, scope, &self.settings),
                Action::Default(scope) => {
                    self.preferences
                        .editor
                        .defaults
                        .set(family, scope, &self.settings)
                }
                Action::ClearDefaults => self.preferences.editor.defaults.clear(family),
                Action::Delete(index) => {
                    self.preferences.editor.presets.remove(index);
                }
            }
        }
        capabilities.normalize(&mut self.settings);
        crate::inspect::normalize(
            &self.settings,
            &mut self.output,
            &mut self.channel,
            Some(metadata),
            None,
        );
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
        before != self.settings
    }
}
