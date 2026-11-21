//! Selects the Before look (Camera or Defaults) and derives its settings without changing authored edits.

use crate::App;
use eframe::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Rendition {
    #[default]
    Current,
    Camera,
    Defaults,
}
/// The look the editing selection is shown with. Before renders the same view
/// with Camera or Defaults settings in place of the authored ones.
#[derive(Default)]
pub struct Before {
    pub active: Rendition,
}
impl Before {
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
        // The selected photo's settings are a placeholder until its sidecar loads.
        let current = if self.selected.as_deref() == Some(path) {
            self.sidecars.ready().then_some(&self.settings)
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
        Some(self.preferences.with_workspace_settings(&settings))
    }
    /// Show the editing selection with `target`, or return to Current when it is already shown.
    pub(crate) fn toggle_before(&mut self, target: Rendition) {
        if self.edit_selection.paths().is_empty() || !self.sidecars.ready() {
            return;
        }
        self.before.active = if self.before.active == target {
            Rendition::Current
        } else {
            target
        };
    }
    pub(crate) fn exit_before(&mut self) {
        self.before.active = Rendition::Current;
    }
    pub(crate) fn before_badge(&self, ui: &egui::Ui, rect: egui::Rect) {
        if self
            .active_image()
            .is_none_or(|f| f.request.rendition == Rendition::Current)
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
            let before = Before { active: rendition };
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
