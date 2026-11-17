//! Keeps transient camera/default renditions separate from authored edits and photographic preview publication.

use crate::{rendering::worker, App};
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
    saved: Option<Option<worker::Frame>>,
    pub detail: Option<worker::Frame>,
}
impl Before {
    pub(crate) fn memory_resources(&self, inventory: &mut crate::memory::Inventory) {
        for frame in self.saved.iter().flatten().chain(self.detail.iter()) {
            inventory.frame(frame);
        }
    }
    pub(crate) fn memory_key(&self, key: &mut Vec<u64>) {
        for frame in self.saved.iter().flatten().chain(self.detail.iter()) {
            crate::memory::frame_key(frame, key);
        }
    }
    pub fn current_frame(&self) -> Option<&worker::Frame> {
        self.saved.as_ref()?.as_ref()
    }
    pub fn take_current(&mut self) -> Option<Option<worker::Frame>> {
        self.saved.take()
    }
    pub fn retain_current(&mut self, frame: &mut Option<worker::Frame>) {
        if self.active != Rendition::Current && self.saved.is_none() {
            self.saved = Some(frame.take());
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
        baseline.working_scale = current.working_scale;
        baseline.output_size = current.output_size;
        baseline.crop = current.crop;
        baseline.rotation = current.rotation;
        Ok(baseline)
    }
}
impl App {
    pub(crate) fn photo_rendition(&self, path: &std::path::Path) -> Rendition {
        if self.edit_selection.contains(path) {
            self.before.active
        } else {
            Rendition::Current
        }
    }
    pub(crate) fn photo_view_settings(
        &self,
        path: &std::path::Path,
    ) -> Option<foveonic::render::Settings> {
        let current = if self.selected.as_deref() == Some(path) {
            Some(&self.settings)
        } else {
            self.multi_edit.settings(path)
        }?;
        let settings = if self.photo_rendition(path) == Rendition::Current {
            current.clone()
        } else {
            let metadata = self.multi_edit.metadata(path).or_else(|| {
                (self.selected.as_deref() == Some(path))
                    .then(|| self.metadata.clone())
                    .flatten()
            });
            self.before
                .settings(
                    current,
                    metadata.as_deref(),
                    &self.preferences.editor.defaults,
                )
                .ok()?
        };
        Some(self.preferences.render_settings(&settings))
    }
    pub(crate) fn before_frame_matches(
        &self,
        path: &std::path::Path,
        frame: &worker::Frame,
        settings: &foveonic::render::Settings,
    ) -> bool {
        frame.path == path
            && frame.output == "final"
            && !frame.framing_preview
            && frame.comparison.is_none()
            && frame.rendition == self.photo_rendition(path)
            && frame.processing_quality == self.preferences.processing_quality
            && crate::rendering::matching::same_authored_edits(&frame.requested, settings)
            && frame.calibration_signature.as_deref()
                == self
                    .noise_calibration
                    .selected(self.preferences.noise_profile)
                    .as_ref()
                    .map(|profile| profile.signature())
            && self
                .worker
                .cache
                .source_origin(path)
                .is_none_or(|origin| origin == frame.origin)
    }
    pub(crate) fn rendition_frame(
        &self,
        path: &std::path::Path,
        rendition: Rendition,
    ) -> Option<&worker::Frame> {
        if rendition == Rendition::Current {
            return self.current_photo_frame(path);
        }
        let settings = self.photo_view_settings(path)?;
        self.frame
            .iter()
            .filter(|frame| self.selected.as_deref() == Some(path) && frame.rendition == rendition)
            .chain(self.multi_edit.before_frame(path, rendition))
            .filter(|frame| self.before_frame_matches(path, frame, &settings))
            .max_by_key(|frame| frame.content_revision())
    }
    pub(crate) fn rendition_detail(
        &self,
        path: &std::path::Path,
        rendition: Rendition,
    ) -> Option<&worker::Frame> {
        if rendition == Rendition::Current {
            return self.multi_edit.detail(path);
        }
        let settings = self.photo_view_settings(path)?;
        self.before
            .detail
            .iter()
            .filter(|frame| self.selected.as_deref() == Some(path) && frame.rendition == rendition)
            .chain(self.multi_edit.before_detail(path, rendition))
            .filter(|frame| self.before_frame_matches(path, frame, &settings))
            .max_by_key(|frame| frame.content_revision())
    }
    /// Transfer the selected photograph's alternate view back to its existing owner.
    pub(crate) fn cache_before_view(&mut self) {
        if self
            .frame
            .as_ref()
            .is_some_and(|frame| frame.rendition != Rendition::Current)
        {
            self.multi_edit.retain_before(self.frame.take().unwrap());
        }
        if let Some(detail) = self.before.detail.take() {
            self.multi_edit.retain_before(detail);
        }
        if let Some(current) = self.before.take_current() {
            self.frame = current;
        }
    }
    pub(crate) fn restore_before_view(&mut self) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        let rendition = self.photo_rendition(&path);
        if rendition == Rendition::Current {
            return;
        }
        let Some(settings) = self.photo_view_settings(&path) else {
            return;
        };
        if let Some(mut frame) = self
            .multi_edit
            .take_before_frame(&path, rendition)
            .filter(|frame| self.before_frame_matches(&path, frame, &settings))
        {
            self.before.retain_current(&mut self.frame);
            frame.revision = self.revision;
            self.frame = Some(frame);
        }
        self.before.detail = self
            .multi_edit
            .take_before_detail(&path, rendition)
            .filter(|frame| self.before_frame_matches(&path, frame, &settings));
    }
    pub(crate) fn submit_before_photos(&mut self) {
        let policy = self.preferences.processing_quality;
        for path in self.edit_selection.paths().to_vec() {
            let Some(mut settings) = self.photo_view_settings(&path) else {
                continue;
            };
            let rendition = self.photo_rendition(&path);
            let refinement = if let Some(intent) =
                self.photo_refinement_for(&path, settings.clone(), rendition)
            {
                let crate::rendering::smart::PhotoRefinement::Ready(refinement) = intent else {
                    continue;
                };
                *refinement
            } else {
                let scale = self.preview_scale_for(&path, policy);
                if let Some(frame) = self.rendition_frame(&path, rendition) {
                    if crate::rendering::request::camera_preview_scale(
                        scale,
                        &frame.artifact.metadata,
                    )
                    .is_ok_and(|wanted| {
                        crate::rendering::quality::accepts_scale(
                            wanted,
                            frame.artifact.settings.working_scale,
                        )
                    }) {
                        if self.selected.as_ref() == Some(&path) {
                            self.opening = None;
                        }
                        continue;
                    }
                }
                // Establish a small alternate whole image before asking for a native patch.
                let preview_scale = if scale == foveonic::render::Scale::NATIVE
                    && self.viewports.view(Some(&path)).explicit_zoom()
                {
                    foveonic::render::Scale::new(8).unwrap()
                } else {
                    scale
                };
                settings.working_scale = foveonic::render::Scale::NATIVE;
                settings.preview_region = None;
                crate::rendering::request::Refinement {
                    first: crate::rendering::request::Request {
                        auxiliary_preview: false,
                        detail: false,
                        framing_preview: false,
                        calibration: self
                            .noise_calibration
                            .selected(self.preferences.noise_profile),
                        rendition,
                        revision: self.revision + 1,
                        path: path.clone(),
                        settings,
                        preview_scale,
                        processing_quality: policy,
                        output: "final".into(),
                        comparison: false,
                        export: None,
                    },
                    visible_region: None,
                    preview: None,
                    whole: None,
                }
            };
            let revision = refinement
                .whole
                .as_ref()
                .or(refinement.preview.as_ref())
                .unwrap_or(&refinement.first)
                .revision;
            let queued_at = self
                .opening
                .as_ref()
                .filter(|_| self.selected.as_ref() == Some(&path))
                .map_or_else(std::time::Instant::now, |opening| opening.began);
            self.publish_render_view();
            if self.worker.refine(refinement, queued_at) {
                self.revision = revision;
            }
            // Identical intent is already published to the coordinator.
            if self.selected.as_ref() == Some(&path) {
                self.opening = None;
            }
        }
        self.busy = self.worker.busy();
    }
    pub(crate) fn toggle_before(&mut self, target: Rendition) {
        if self.edit_selection.paths().is_empty() || self.exporting || !self.sidecars.ready() {
            return;
        }
        if self.before.active == target {
            self.exit_before();
            if self.frame.as_ref().is_none_or(|f| {
                f.processing_quality != self.current_processing_quality()
                    || f.framing_preview != self.framing_preview_active()
                    || !crate::rendering::matching::same_authored_edits(
                        &f.requested,
                        &self.settings,
                    )
            }) {
                self.submit(None);
            }
        } else {
            self.cache_before_view();
            self.before.active = target;
            self.sync_multi_edit();
            self.restore_before_view();
            self.publish_render_view();
            self.submit_before_photos();
        }
    }
    pub(crate) fn exit_before(&mut self) {
        if self.before.active == Rendition::Current {
            return;
        }
        self.cache_before_view();
        self.before.active = Rendition::Current;
        self.before.detail = None;
        for path in self.multi_edit.retained_paths() {
            self.worker.retire_before(&path);
        }
        self.multi_edit.request_current();
        let processing_quality = self.current_processing_quality();
        self.revision += 1;
        if let Some(path) = &self.selected {
            if self.output == "final" && !self.comparison.requested() {
                if let Some(frame) = self
                    .multi_edit
                    .take_frame(path)
                    .filter(|frame| frame.processing_quality == processing_quality)
                {
                    if self
                        .frame
                        .as_ref()
                        .is_none_or(|saved| frame.content_revision() >= saved.content_revision())
                    {
                        self.frame = Some(frame);
                    } else {
                        self.multi_edit.retain(frame);
                    }
                }
            }
        }
        if let Some(frame) = &mut self.frame {
            // Presentation consumers publish revisions monotonically. Restoring
            // an owned frame is a new presentation without another render.
            frame.revision = self.revision;
        }
        if let Some(frame) = &self.frame {
            self.worker.delivered(frame.revision);
        }
        self.sync_render_progress(None);
        self.scope_key = None;
        self.previews.invalidate();
        crate::viewport::clear(&self.render_state);
    }
    pub(crate) fn view_settings(&self) -> foveonic::render::Settings {
        let settings = self
            .before
            .settings(
                &self.settings,
                self.metadata.as_deref(),
                &self.preferences.editor.defaults,
            )
            .unwrap_or_else(|_| self.settings.clone());
        self.preferences.render_settings(&settings)
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
    fn capturing_an_absent_current_frame_is_still_a_saved_presentation() {
        let mut before = Before {
            active: Rendition::Camera,
            ..Default::default()
        };
        let mut frame = None;
        before.retain_current(&mut frame);
        assert!(matches!(before.saved, Some(None)));
        // Repeated alternate replies cannot replace the captured absence.
        before.retain_current(&mut frame);
        assert!(matches!(before.take_current(), Some(None)));
        assert!(before.take_current().is_none());
    }

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
        current.working_scale = foveonic::render::Scale::new(2).unwrap();
        current.crop.bounds = [0.1, 0.2, 0.9, 0.8];
        current.rotation.turns = 1;
        current.rotation.degrees = 3.;
        current.output_size = Some([1200, 800]);
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
        assert!(crate::rendering::matching::same_image_geometry(
            &current, &baseline
        ));
        assert_eq!(baseline.exposure, 0.);
        assert_eq!(baseline.white_balance, "");
        assert_eq!(baseline.white_balance_bias.warmth, 0.);
        assert_eq!(baseline.working_scale, current.working_scale);
        before.active = Rendition::Defaults;
        let baseline = before
            .settings(&current, Some(&metadata), &defaults)
            .unwrap();
        assert!(crate::rendering::matching::same_image_geometry(
            &current, &baseline
        ));
        assert_eq!(
            baseline.photographic.tone_method,
            PhotographicMethod::Universal
        );
        assert_eq!(current.exposure, 1.2);
        assert_eq!(current.white_balance_bias.warmth, 0.3);
        before.active = Rendition::Current;
        assert_eq!(before.settings(&current, None, &defaults).unwrap(), current);
    }
    #[test]
    fn group_baselines_use_each_cameras_defaults_and_authored_geometry() {
        let metadata = ["SIGMA DP2 Merrill", "SIGMA SD14"].map(|model| {
            crate::capture::processing_metadata([("CAMMODEL".into(), model.into())].into())
        });
        let mut defaults = RecipeDefaults::default();
        let universal = Settings {
            photographic: foveonic::render::PhotographicControls {
                tone_method: PhotographicMethod::Universal,
                ..Default::default()
            },
            ..Default::default()
        };
        defaults.set(
            foveonic::source::ProcessingFamily::Merrill,
            EditScope::Tool(EditSection::Tone),
            &universal,
        );
        let mut currents = [
            Settings::new("Sunlight", "Standard"),
            Settings::new("Cloudy", "Standard"),
        ];
        currents[0].crop.bounds = [0.1, 0.2, 0.8, 0.9];
        currents[0].rotation.turns = 1;
        currents[0].output_size = Some([1200, 800]);
        currents[0].exposure = 1.2;
        currents[1].crop.bounds = [0.2, 0.1, 0.9, 0.7];
        currents[1].rotation.turns = 3;
        currents[1].output_size = Some([900, 600]);
        currents[1].exposure = -0.5;
        let authored = currents.clone();
        for rendition in [Rendition::Camera, Rendition::Defaults] {
            let before = Before {
                active: rendition,
                ..Default::default()
            };
            let baselines = currents
                .iter()
                .zip(&metadata)
                .map(|(current, metadata)| {
                    before.settings(current, Some(metadata), &defaults).unwrap()
                })
                .collect::<Vec<_>>();
            for (baseline, current) in baselines.iter().zip(&currents) {
                assert!(crate::rendering::matching::same_image_geometry(
                    baseline, current
                ));
                assert_eq!(baseline.exposure, 0.);
            }
            assert_ne!(baselines[0].crop, baselines[1].crop);
            if rendition == Rendition::Defaults {
                assert_eq!(
                    baselines[0].photographic.tone_method,
                    PhotographicMethod::Universal
                );
                assert_eq!(
                    baselines[1].photographic.tone_method,
                    PhotographicMethod::Reference
                );
            }
        }
        assert_eq!(currents, authored);
    }
}
