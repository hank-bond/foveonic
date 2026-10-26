//! Applies workspace commands through browser selection and the existing per-photo editing session.

use crate::{
    controls, inspect,
    shortcuts::{self, Action},
    App,
};
use eframe::egui;
use foveonic::{render::Settings, sidecar};

pub enum Edit {
    CopySettings,
    PasteSettings(String),
}

impl App {
    pub fn capture_actions(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.actions.clear();
        let mut browser = Vec::new();
        for invocation in shortcuts::capture(
            ctx,
            input,
            shortcuts::Availability {
                enabled: !self.exporting,
                browser: !self.browser.files.is_empty(),
                import: self.browser.import_mode,
                settings: self.selected.is_some() && self.sidecars.ready(),
            },
        ) {
            match invocation.action {
                Action::Browser(action) => browser.push((action, invocation.modifiers)),
                Action::CopySettings => self.actions.push(Edit::CopySettings),
                Action::PasteSettings => {
                    if let Some(text) = invocation.text {
                        self.actions.push(Edit::PasteSettings(text));
                    }
                }
            }
        }
        self.browser.set_actions(browser);
    }

    /// Browser actions run first, so Select All and Paste in one input frame
    /// target the new highlights. Recipe edits then join the normal edit/render path.
    pub fn dispatch_actions(&mut self, ctx: &egui::Context) -> bool {
        if self.actions.is_empty() {
            return false;
        }
        // A folder picker or export button can change readiness after capture.
        if self.exporting || !self.sidecars.ready() || self.selected.is_none() {
            self.actions.clear();
            return false;
        }
        let before = self.settings.clone();
        for edit in std::mem::take(&mut self.actions) {
            match edit {
                Edit::CopySettings => {
                    match sidecar::encode(&self.settings)
                        .and_then(|bytes| Ok(String::from_utf8(bytes)?))
                    {
                        Ok(recipe) => {
                            ctx.copy_text(recipe);
                            self.status = "Copied image settings".into();
                            self.status_error = false;
                        }
                        Err(error) => {
                            self.status = format!("Cannot copy settings: {error}");
                            self.status_error = true;
                        }
                    }
                }
                Edit::PasteSettings(text) => {
                    match clipboard_settings(&text) {
                        Ok(copied) => {
                            let paths = self.browser.processing_selection(self.selected.as_deref());
                            let count = paths.len();
                            for path in paths {
                                let mut settings = copied.clone();
                                if self.selected.as_ref() == Some(&path) {
                                    // Preview policy and inspection belong to the workspace.
                                    settings.preview_quality = self.settings.preview_quality;
                                    settings.preview_region = self.settings.preview_region;
                                    controls::normalize_with_capture(
                                        &mut settings,
                                        self.metadata.as_deref(),
                                        self.capture.properties.as_ref(),
                                    );
                                    self.settings = settings.clone();
                                    inspect::normalize(
                                        &self.settings,
                                        &mut self.output,
                                        &mut self.channel,
                                        self.metadata.as_deref(),
                                        None,
                                    );
                                }
                                self.sidecars.apply(path, settings);
                            }
                            self.status = format!("Applied settings to {count} photos");
                            self.status_error = false;
                        }
                        Err(error) => {
                            self.status = format!("Cannot paste settings: {error}");
                            self.status_error = true;
                        }
                    }
                }
            }
        }
        self.settings != before
    }
}

fn clipboard_settings(text: &str) -> foveonic::source::Result<Settings> {
    sidecar::decode(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_recipes_copy_requested_edits_without_transient_preview_state() {
        let mut source = Settings::new("Shade", "Vivid");
        source.exposure = 0.7;
        source.tone_enabled = false;
        source.preview_quality = foveonic::render::PreviewQuality::Draft;
        source.preview_region = Some([10, 20, 100, 200]);
        let encoded = String::from_utf8(sidecar::encode(&source).unwrap()).unwrap();
        let copied = clipboard_settings(&encoded).unwrap();
        source.preview_quality = foveonic::render::PreviewQuality::Full;
        source.preview_region = None;
        assert_eq!(copied, source);
        assert!(clipboard_settings("ordinary clipboard text").is_err());
        assert!(
            clipboard_settings(&encoded.replace("\"version\": 1", "\"version\": 999")).is_err()
        );
    }
}
