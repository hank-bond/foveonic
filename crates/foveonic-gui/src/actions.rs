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
    Rotate,
    CycleRotate,
    PasteSettings(String),
    Before(crate::editor::Rendition),
}

impl App {
    pub fn capture_actions(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.actions.clear();
        if crate::photo_tools::menu_open(ctx) {
            self.browser.set_actions(Vec::new());
            return;
        }
        let mut browser = Vec::new();
        for invocation in shortcuts::capture(
            ctx,
            input,
            shortcuts::Availability {
                enabled: !self.exporting,
                browser: !self.browser.files.is_empty(),
                import: self.browser.import_mode,
                settings: self.selected.is_some() && self.sidecars.ready(),
                photo_tool_active: crate::photo_tools::active(ctx),
                framing_active: crate::crop_tool::active(ctx) || crate::rotate_tool::active(ctx),
            },
        ) {
            match invocation.action {
                Action::Browser(action) => browser.push((action, invocation.modifiers)),
                Action::DirectColor => {
                    self.exit_before();
                    if crate::editor::advanced_color::state(ctx).advanced {
                        crate::editor::direct_color::toggle_advanced(ctx);
                    } else {
                        crate::editor::direct_color::toggle(ctx);
                    }
                }
                Action::ExitTool => {
                    crate::photo_tools::escape(ctx);
                }
                Action::Rotate => self.actions.push(Edit::Rotate),
                Action::CycleRotate => self.actions.push(Edit::CycleRotate),
                Action::Crop => {
                    self.exit_before();
                    self.output = "final".into();
                    self.channel = -1;
                    self.comparison = Default::default();
                    crate::crop_tool::toggle(ctx);
                }
                Action::Hand | Action::FinishTool => {
                    crate::photo_tools::hand(ctx);
                }
                Action::CopySettings => self.actions.push(Edit::CopySettings),
                Action::PasteSettings => {
                    if let Some(text) = invocation.text {
                        self.actions.push(Edit::PasteSettings(text));
                    }
                }
                Action::BeforeCamera => self
                    .actions
                    .push(Edit::Before(crate::editor::Rendition::Camera)),
                Action::BeforeDefaults => self
                    .actions
                    .push(Edit::Before(crate::editor::Rendition::Defaults)),
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
        let before_output = self.output.clone();
        for edit in std::mem::take(&mut self.actions) {
            match edit {
                edit @ (Edit::Rotate | Edit::CycleRotate) => {
                    self.exit_before();
                    self.output = "final".into();
                    self.channel = -1;
                    self.comparison = Default::default();
                    if matches!(edit, Edit::CycleRotate) {
                        crate::rotate_tool::cycle(ctx);
                    } else {
                        crate::rotate_tool::invoke(
                            ctx,
                            &mut self.settings.rotation,
                            &mut self.settings.crop,
                        );
                    }
                }
                Edit::Before(target) => self.toggle_before(target),
                Edit::CopySettings => {
                    match sidecar::encode(&self.settings)
                        .and_then(|bytes| Ok(String::from_utf8(bytes)?))
                    {
                        Ok(recipe) => {
                            ctx.copy_text(recipe);
                            self.activity.operation("Copied image settings");
                        }
                        Err(error) => {
                            self.fail(format!("Cannot copy settings: {error}"));
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
                            self.activity
                                .operation(format!("Applied settings to {count} photos"));
                        }
                        Err(error) => {
                            self.fail(format!("Cannot paste settings: {error}"));
                        }
                    }
                }
            }
        }
        self.settings != before || self.output != before_output
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
        source.preview_quality = foveonic::render::PreviewQuality::Fast;
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
