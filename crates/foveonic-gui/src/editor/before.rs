//! Keeps transient camera/default renditions separate from authored edits and photographic preview publication.

use crate::{worker, App};
use eframe::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rendition {
    #[default]
    Current,
    Camera,
    Defaults,
}
#[derive(Default)]
pub struct Before {
    pub active: Rendition,
    saved: Option<(Option<worker::Frame>, Option<worker::Frame>)>,
}
impl Before {
    pub fn retain_current(
        &mut self,
        frame: &mut Option<worker::Frame>,
        detail: &mut Option<worker::Frame>,
    ) {
        if self.active != Rendition::Current && self.saved.is_none() {
            self.saved = Some((frame.take(), detail.take()));
        }
    }
    pub fn settings(
        &self,
        current: &foveonic::render::Settings,
        metadata: Option<&foveonic::source::Metadata>,
        defaults: &foveonic::render::RecipeDefaults,
    ) -> foveonic::source::Result<foveonic::render::Settings> {
        if self.active == Rendition::Current {
            return Ok(current.clone());
        }
        let metadata = metadata.ok_or("camera metadata is not ready")?;
        let mut baseline = match self.active {
            Rendition::Defaults => defaults.for_metadata(metadata)?,
            _ => foveonic::render::camera_settings(metadata, None)?,
        };
        baseline.preview_quality = current.preview_quality;
        baseline.output_size = current.output_size;
        baseline.crop = current.crop;
        baseline.rotation = current.rotation;
        Ok(baseline)
    }
}
impl App {
    pub(crate) fn toggle_before(&mut self, target: Rendition) {
        if self.metadata.is_none() || self.exporting || !self.sidecars.ready() {
            return;
        }
        if self.before.active == target {
            self.exit_before();
            if self.frame.as_ref().is_none_or(|f| {
                f.framing_preview != self.framing_preview_active()
                    || !crate::smart::same_edits(&f.requested, &self.settings)
            }) {
                self.submit(None);
            }
        } else {
            self.before.active = target;
            self.smart.reset();
            self.submit_request(None, None);
        }
    }
    pub(crate) fn exit_before(&mut self) {
        if self.before.active == Rendition::Current {
            return;
        }
        self.before.active = Rendition::Current;
        if let Some((frame, detail)) = self.before.saved.take() {
            self.frame = frame;
            self.detail = detail;
        }
        self.revision += 1;
        if let Some(frame) = &mut self.frame {
            // Presentation consumers publish revisions monotonically. Restoring
            // an owned frame is a new presentation without another render.
            frame.revision = self.revision;
        }
        self.worker.cancel_before(self.revision);
        self.worker.progress.finish(self.revision - 1, false);
        self.busy = false;
        self.scope_key = None;
        self.smart.reset();
        self.previews.invalidate();
        crate::viewport::clear(&self.render_state);
    }
    pub(crate) fn view_settings(&self) -> foveonic::render::Settings {
        let mut settings = self
            .before
            .settings(
                &self.settings,
                self.metadata.as_deref(),
                &self.preferences.editor.defaults,
            )
            .unwrap_or_else(|_| self.settings.clone());
        settings.adaptive_tone_tuning = self.preferences.adaptive_tone_tuning;
        settings
    }
    pub(crate) fn before_badge(&self, ui: &egui::Ui, rect: egui::Rect) {
        if self
            .frame
            .as_ref()
            .is_none_or(|f| f.rendition == Rendition::Current)
        {
            return;
        }
        crate::widgets::overlay_badge(ui, rect, "Before");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foveonic::render::{EditScope, EditSection, PhotographicMethod, RecipeDefaults, Settings};
    #[test]
    fn camera_and_default_renditions_never_replace_authored_values() {
        let metadata = crate::capture::processing_metadata(
            [("CAMMODEL".into(), "SIGMA DP2 Merrill".into())].into(),
        );
        let mut current = Settings {
            exposure: 1.2,
            ..Default::default()
        };
        current.white_balance_bias.warmth = 0.3;
        current.preview_quality = foveonic::render::PreviewQuality::Fast;
        let mut defaults = RecipeDefaults::default();
        let mut default = Settings::default();
        default.photographic.tone_method = PhotographicMethod::Universal;
        defaults.set(
            foveonic::source::ProcessingFamily::Merrill,
            EditScope::Tool(EditSection::Tone),
            &default,
        );
        let mut before = Before {
            active: Rendition::Camera,
            ..Default::default()
        };
        let baseline = before
            .settings(&current, Some(&metadata), &defaults)
            .unwrap();
        assert_eq!(baseline.exposure, 0.);
        assert_eq!(baseline.white_balance, "");
        assert_eq!(baseline.white_balance_bias.warmth, 0.);
        assert_eq!(baseline.preview_quality, current.preview_quality);
        before.active = Rendition::Defaults;
        let baseline = before
            .settings(&current, Some(&metadata), &defaults)
            .unwrap();
        assert_eq!(
            baseline.photographic.tone_method,
            PhotographicMethod::Universal
        );
        assert_eq!(current.exposure, 1.2);
        assert_eq!(current.white_balance_bias.warmth, 0.3);
        before.active = Rendition::Current;
        assert_eq!(before.settings(&current, None, &defaults).unwrap(), current);
    }
}
