//! Reads what each viewport photo shows off the app, plans its view through the view owner and selects what is drawn.

use super::{
    rendered::ImageKey,
    view_images::{PhotoFacts, PhotoLooks, Round},
    view_plan::Regions,
    viewport_plan::Placement,
    worker::RenderResult,
};
use crate::{editor::Rendition, App};
use foveonic::{render::Scale, source::SourceOrigin};
use std::path::{Path, PathBuf};

impl App {
    /// How the photo is shown: its looks, the chosen output and what its camera
    /// offers, comparison and, with a framing tool armed, the crop and rotation plane.
    pub(crate) fn photo_looks(&self, path: &Path) -> PhotoLooks {
        let look = self.photo_rendition(path);
        let selected = self.selected.as_deref() == Some(path);
        let settings = self.multi_edit.settings(path);
        let metadata = self.multi_edit.metadata(path);
        let offered = if self.output == "final" || selected {
            Vec::new()
        } else {
            let family = self
                .browser
                .metadata_summary(path)
                .and_then(|summary| summary.family);
            settings
                .zip(metadata.as_deref())
                .map(|(settings, metadata)| {
                    foveonic::inspection::image_outputs_with_family(
                        settings,
                        Some(metadata),
                        family,
                    )
                    .iter()
                    .map(|offered| offered.name)
                    .collect()
                })
                .unwrap_or_default()
        };
        let look_settings = (look != Rendition::Current)
            .then(|| self.look_settings(path, look))
            .flatten();
        PhotoLooks {
            path: path.to_owned(),
            look,
            current_settings: look_settings
                .is_none()
                .then(|| self.look_settings(path, Rendition::Current))
                .flatten(),
            look_settings,
            output: self.output.clone(),
            selected,
            offered,
            compares: self.comparison.requested()
                && settings.is_some_and(|settings| {
                    foveonic::inspection::comparison_input(
                        settings,
                        &self.output,
                        metadata.as_deref(),
                    )
                    .is_some()
                }),
            framing: self.frames(path),
        }
    }

    /// What the photo shows.
    pub(crate) fn image_key(&self, path: &Path) -> ImageKey {
        self.photo_looks(path).key()
    }

    /// The active photo's key and source version, for drawing it from the collection.
    pub(crate) fn active_key(&self) -> Option<(ImageKey, Option<SourceOrigin>)> {
        let path = self.selected.as_deref()?;
        Some((self.image_key(path), self.worker.cache.source_origin(path)))
    }

    /// What is drawn for a photo: its whole image, and its patch on top when its
    /// view is magnified and the layer rule allows.
    pub(crate) fn photo_layers(&self, path: &Path) -> [Option<&RenderResult>; 2] {
        let origin = self.worker.cache.source_origin(path);
        self.view_images
            .layers(&self.image_key(path), origin.as_ref())
    }

    /// The patch drawn over a photo's whole image.
    pub(crate) fn photo_patch(&self, path: &Path) -> Option<&RenderResult> {
        self.photo_layers(path)[1]
    }

    /// The patch drawn over the active photo.
    pub(crate) fn active_patch(&self) -> Option<&RenderResult> {
        self.selected
            .as_deref()
            .and_then(|path| self.photo_patch(path))
    }

    /// The active photo's drawn whole image.
    pub(crate) fn active_image(&self) -> Option<&RenderResult> {
        let (key, origin) = self.active_key()?;
        self.view_images.whole(&key, origin.as_ref())
    }

    /// A photo's drawn whole image for its Current look, whatever it shows now.
    pub(crate) fn current_image(&self, path: &Path) -> Option<&RenderResult> {
        self.view_images.current_image(path)
    }

    /// What planning needs about each viewport photo, read from the app.
    fn viewport_photo_facts(&self) -> Vec<PhotoFacts> {
        let policy = self.preferences.processing_quality;
        self.planned_photos()
            .into_iter()
            .map(|path| {
                let view = self.viewports.view(Some(&path));
                let preview_scale = self.preview_scale_for(&path, policy);
                let placement = if self.loupe_inspecting(&path) {
                    Placement::Loupe {
                        regions: self.loupe_geometry().map(|geometry| Regions {
                            visible: geometry.visible,
                            patch: geometry.region,
                        }),
                    }
                } else if let Some(fit_scale) = self
                    .fit_scale_for(&path, policy)
                    .filter(|_| view.explicit_zoom() && preview_scale == Some(Scale::NATIVE))
                {
                    Placement::Zoomed {
                        visible: view.visible_pixels(),
                        fit_scale,
                    }
                } else {
                    Placement::Fit
                };
                PhotoFacts {
                    looks: self.photo_looks(&path),
                    origin: self.worker.cache.source_origin(&path),
                    preview_scale,
                    placement,
                }
            })
            .collect()
    }

    /// Rendering is not up to date: work is outstanding, a view is still
    /// settling, or the last plan had photos still loading.
    pub(crate) fn view_busy(&self) -> bool {
        self.worker.busy() || self.view_images.waiting()
    }

    /// Photos drawn in the viewport for the current browser selection.
    pub(crate) fn viewport_photos(&self) -> Vec<PathBuf> {
        self.viewport_selection
            .iter()
            .filter(|path| self.browser_selection.contains(path))
            .cloned()
            .collect()
    }

    /// The photos planned this frame: the viewport photos, and edited photos
    /// the viewport has no room to draw, whose edits still render.
    pub(crate) fn planned_photos(&self) -> Vec<PathBuf> {
        let mut photos = self.viewport_photos();
        for path in self.edit_selection.paths() {
            if !photos.contains(path) {
                photos.push(path.clone());
            }
        }
        photos
    }

    /// Plan every viewport photo's images in use and publish them.
    pub(crate) fn submit_photos(&mut self) {
        self.sync_multi_edit();
        // Planning and the selected photo's controls wait for metadata summaries.
        let mut needed = self.planned_photos();
        if let Some(selected) = self.selected.clone().filter(|path| !needed.contains(path)) {
            needed.push(selected);
        }
        self.browser.need_summaries(&needed);
        let now = std::time::Instant::now();
        let photos = self.viewport_photo_facts();
        let round = Round {
            selected: self.selected.as_deref(),
            editing: self.edit_selection.paths(),
            history: self.multi_edit.retained_paths(),
            policy: self.preferences.processing_quality,
            calibration: self
                .noise_calibration
                .selected(self.preferences.noise_profile),
            background_completion: self.preferences.background_full_image_completion,
            now,
        };
        let worker = &self.worker;
        let outcome = self.view_images.plan(photos, round, |photos, planned| {
            worker.plan(photos, planned, now)
        });
        if let Some(wait) = outcome.repaint_after {
            self.context.request_repaint_after(wait);
        }
        if let Some(((id, path), settings)) = outcome
            .selected_request
            .zip(self.selected.as_deref())
            .zip(self.selected_settings())
        {
            crate::timing::requested(id.0, path, "final", settings);
        }
        if self
            .selected
            .as_ref()
            .is_some_and(|path| outcome.planned.contains(path))
        {
            self.opening = None;
        }
        self.busy = self.view_busy();
    }

    /// The photo's visible region in native image coordinates.
    pub(crate) fn visible_region(&self, path: &Path) -> Option<[u32; 4]> {
        let window = self.photo_layers(path)[0]?.display_window(false);
        let visible = self.viewports.view(Some(path)).visible_pixels()?;
        Some(visible.native_bounds(window))
    }
}
