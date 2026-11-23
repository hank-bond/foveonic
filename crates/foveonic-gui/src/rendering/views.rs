//! Decides what each viewport photo shows, plans the images its view uses and selects what is drawn.

pub(crate) use super::viewport_plan::PhotoView;
use super::{
    rendered::{ImageKey, RenderedImages},
    request::{RenderRequest, RequestId},
    view_plan::{patch_on_top, Motion, PhotoImages, Regions, Rendered},
    viewport_plan::{plan_photos, PhotoInputs, Placement, Planning, Publication},
    worker::RenderResult,
};
use crate::{editor::Rendition, App};
use foveonic::{render::Scale, source::SourceOrigin};
use std::path::{Path, PathBuf};

/// A stored image is usable for a photo when it was made from the photo's
/// current source version.
pub(crate) fn usable<'a>(
    image: Option<&'a RenderResult>,
    origin: Option<&SourceOrigin>,
) -> Option<&'a RenderResult> {
    image.filter(|image| origin.is_none_or(|origin| *origin == image.origin))
}

/// The whole image drawn for a key, if usable.
pub(crate) fn whole_image<'a>(
    images: &'a RenderedImages<RenderResult>,
    key: &ImageKey,
    origin: Option<&SourceOrigin>,
) -> Option<&'a RenderResult> {
    usable(images.whole(key), origin)
}

impl App {
    /// What the photo shows: its look, and for the active photo its output,
    /// comparison and the crop and rotation plane.
    pub(crate) fn image_key(&self, path: &Path) -> ImageKey {
        self.key_for(path, self.photo_look(path))
    }

    /// What the photo shows with this look.
    fn key_for(&self, path: &Path, look: Rendition) -> ImageKey {
        let active = self.selected.as_deref() == Some(path);
        let output = if look == Rendition::Current {
            self.photo_output(path)
        } else {
            "final".into()
        };
        let comparison = look == Rendition::Current
            && self.comparison.requested()
            && self.multi_edit.settings(path).is_some_and(|settings| {
                foveonic::inspection::comparison_input(
                    settings,
                    &output,
                    self.multi_edit.metadata(path).as_deref(),
                )
                .is_some()
            });
        ImageKey {
            path: path.to_owned(),
            look,
            output,
            comparison,
            framing: active && self.framing_preview_active(),
        }
    }

    /// The chosen inspection output, applied to every viewport photo whose
    /// camera offers it; other photos show their final photograph.
    fn photo_output(&self, path: &Path) -> String {
        if self.output == "final" || self.selected.as_deref() == Some(path) {
            // The selected photo's output was normalized for its own camera.
            return self.output.clone();
        }
        let (Some(settings), Some(metadata)) = (
            self.multi_edit.settings(path),
            self.multi_edit.metadata(path),
        ) else {
            return "final".into();
        };
        let family = self
            .browser
            .metadata_summary(path)
            .and_then(|summary| summary.family);
        let offered =
            foveonic::inspection::image_outputs_with_family(settings, Some(&metadata), family)
                .iter()
                .any(|offered| offered.name == self.output);
        if offered {
            self.output.clone()
        } else {
            "final".into()
        }
    }

    /// The photo's stored images for what it shows. Images from another source
    /// version are not this photo's. The Current image standing in for another
    /// look does not match that look's request, so the planner renders it.
    fn usable_images(&self, path: &Path) -> PhotoImages<'_, RenderResult> {
        let origin = self.worker.cache.source_origin(path);
        let key = self.image_key(path);
        let patch = self
            .images
            .get(&key)
            .and_then(|images| images.patch.as_ref());
        PhotoImages {
            whole: usable(self.images.whole(&key), origin.as_ref()),
            patch: usable(patch, origin.as_ref()),
        }
    }

    /// The active photo's key and source version, for drawing it from the collection.
    pub(crate) fn active_key(&self) -> Option<(ImageKey, Option<SourceOrigin>)> {
        let path = self.selected.as_deref()?;
        Some((self.image_key(path), self.worker.cache.source_origin(path)))
    }

    /// What is drawn for a photo: its whole image, and its patch on top when its
    /// view is magnified and the layer rule allows.
    pub(crate) fn photo_layers(&self, path: &Path) -> [Option<&RenderResult>; 2] {
        let images = self.usable_images(path);
        let patch = self
            .views
            .get(path)
            .filter(|view| view.mode.magnified())
            .and_then(|view| {
                images.patch.filter(|patch| {
                    patch_on_top(*patch, images.whole, |frame| frame.matches(&view.request))
                })
            });
        [images.whole, patch]
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
        whole_image(&self.images, &key, origin.as_ref())
    }

    /// A photo's drawn whole image for its Current look, whatever it shows now.
    pub(crate) fn current_image(&self, path: &Path) -> Option<&RenderResult> {
        self.images.get(&ImageKey::current(path))?.whole.as_ref()
    }

    /// The request a photo's images are planned from: its look's settings, output
    /// and quality at the scale its tile resolves when not zoomed. Its images get
    /// their own ids when they are planned.
    fn view_request(&self, path: &Path) -> Option<RenderRequest> {
        let key = self.image_key(path);
        let policy = self.preferences.processing_quality;
        let preview_scale = self.preview_scale_for(path, policy)?;
        let mut settings = self.photo_view_settings(path)?;
        settings.working_scale = Scale::NATIVE;
        settings.preview_region = None;
        let quality = super::quality::for_view(policy, key.look, &key.output, key.comparison);
        settings.processing_quality = quality;
        Some(RenderRequest {
            detail: false,
            framing_preview: key.framing,
            calibration: self
                .noise_calibration
                .selected(self.preferences.noise_profile),
            rendition: key.look,
            id: RequestId::default(),
            path: path.to_owned(),
            settings,
            preview_scale,
            processing_quality: quality,
            output: key.output,
            comparison: key.comparison,
        })
    }

    /// What planning needs about each viewport photo, read from the app.
    fn viewport_photo_inputs(&self) -> Vec<PhotoInputs<'_, RenderResult>> {
        let policy = self.preferences.processing_quality;
        self.planned_photos()
            .into_iter()
            .map(|path| {
                let request = self.view_request(&path);
                let images = self.usable_images(&path);
                let regional = request.as_ref().and_then(|request| {
                    images.whole.or(images.patch).map(|image| {
                        request
                            .settings
                            .regional_preview_available(&image.artifact.metadata)
                            && request.output == "final"
                            && !request.comparison
                    })
                });
                let view = self.viewports.view(Some(&path));
                let placement = if self.loupe_inspecting(&path) {
                    Placement::Loupe {
                        regions: self.loupe_geometry().map(|geometry| Regions {
                            visible: geometry.visible,
                            patch: geometry.region,
                        }),
                    }
                } else if let Some(fit_scale) = self.fit_scale_for(&path, policy).filter(|_| {
                    view.explicit_zoom()
                        && self.preview_scale_for(&path, policy) == Some(Scale::NATIVE)
                }) {
                    Placement::Zoomed {
                        visible: view.visible_pixels(),
                        fit_scale,
                    }
                } else {
                    Placement::Fit
                };
                PhotoInputs {
                    window: images.whole.map(|whole| whole.display_window(false)),
                    path,
                    request,
                    images,
                    regional,
                    placement,
                }
            })
            .collect()
    }

    /// Rendering is not up to date: work is outstanding, a view is still
    /// settling, or the last plan had photos still loading.
    pub(crate) fn view_busy(&self) -> bool {
        self.worker.busy()
            || self.view_motion.borrow().values().any(Motion::settling)
            || self.photos_loading
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
        let previous_views = std::mem::take(&mut self.views);
        let mut motion = std::mem::take(self.view_motion.get_mut());
        let publication = plan_photos(
            self.viewport_photo_inputs(),
            previous_views,
            &mut motion,
            |request| self.images.failed(request),
            Planning {
                selected: self.selected.as_deref(),
                editing: self.edit_selection.paths(),
                last_request_id: self.last_request_id,
                background_completion: self.preferences.background_full_image_completion,
                now,
            },
        );
        let Publication {
            mut photos,
            planned,
            views,
            unplanned,
            last_request_id,
            selected_request,
            repaint_after,
        } = publication;
        *self.view_motion.get_mut() = motion;
        if let Some(wait) = repaint_after {
            self.context.request_repaint_after(wait);
        }
        self.views = views;
        self.photos_loading = !photos.loading.is_empty();
        photos.history = self.multi_edit.retained_paths();
        let viewport = photos.planned.clone();
        if self.worker.plan(photos, planned, now) {
            foveonic::diagnostics::event(
                foveonic::diagnostics::Category::Coordinator,
                "view.unplanned",
                |fields| {
                    fields.str(
                        "photos",
                        &serde_json::to_string(&unplanned).expect("plan diagnostics"),
                    );
                },
            );
            self.last_request_id = last_request_id;
            if let Some(((id, path), settings)) = selected_request
                .zip(self.selected.as_deref())
                .zip(self.selected_settings())
            {
                crate::timing::requested(id.0, path, "final", settings);
            }
        }
        if self
            .selected
            .as_ref()
            .is_some_and(|path| viewport.contains(path))
        {
            self.opening = None;
        }
        self.keep_images();
        self.busy = self.view_busy();
    }

    /// Keep each viewport photo's images for what it shows now and with Before
    /// off, and the Current image of viewport and history photos. Drop everything else, so outputs
    /// a photo no longer shows do not hold memory.
    fn keep_images(&mut self) {
        // Before is a shortcut, so what the photo shows with it off is kept too.
        let shown = self
            .planned_photos()
            .iter()
            .flat_map(|path| [self.image_key(path), self.key_for(path, Rendition::Current)])
            .collect::<Vec<_>>();
        let retained = self.multi_edit.retained_paths();
        self.images.retain(|key| {
            shown.contains(key)
                || ((retained.contains(&key.path)
                    || shown.iter().any(|shown| shown.path == key.path))
                    && *key == ImageKey::current(&key.path))
        });
    }

    /// The photo's visible region in native image coordinates.
    pub(crate) fn visible_region(&self, path: &Path) -> Option<[u32; 4]> {
        let window = self.photo_layers(path)[0]?.display_window(false);
        let visible = self.viewports.view(Some(path)).visible_pixels()?;
        Some(visible.native_bounds(window))
    }
}
