//! Decides what each displayed photo shows, plans the images its view uses and selects what is drawn.

use super::{
    rendered::{ImageKey, RenderedImages},
    request::{RenderRequest, RequestId},
    smart::render_region,
    view_plan::{patch_on_top, plan, Motion, PhotoImages, Plan, Planned, Rendered, Use, View},
    worker::RenderResult,
};
use crate::{editor::Rendition, App};
use foveonic::{render::Scale, source::SourceOrigin};
use std::path::{Path, PathBuf};

/// The photos being edited, those shown in the viewport and one the loupe
/// inspects. A shown photo the editing owner no longer holds has no settings, so
/// it keeps its images without rendering until the loupe admits it.
fn displayed(editing: &[PathBuf], shown: Vec<PathBuf>, loupe: Option<PathBuf>) -> Vec<PathBuf> {
    let mut photos = editing.to_vec();
    for path in shown.into_iter().chain(loupe) {
        if !photos.contains(&path) {
            photos.push(path);
        }
    }
    photos
}

/// A stored image is usable for a photo when it was made from the photo's
/// current source version.
fn usable<'a>(
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

/// A displayed photo's view as last planned: the request its images are planned
/// from and how it is shown. Planning runs after drawing, so a frame draws the
/// views planned at the end of the frame before.
pub(crate) struct PhotoView {
    /// The view's template request. Its id is unset: each image planned from it
    /// gets its own id, so compare images with `matches`, not by id.
    pub request: RenderRequest,
    pub view: View,
}

/// A photo whose view this frame publishes no images: it keeps its work.
enum Unplanned {
    /// Its settings, lens or screen bounds are not ready yet.
    Waiting(&'static str),
    /// Its settings could not be resolved.
    Failed,
}

impl App {
    /// What the photo shows: its look, and for the active photo its output,
    /// comparison and the crop and rotation plane.
    pub(crate) fn image_key(&self, path: &Path) -> ImageKey {
        self.key_for(path, self.photo_rendition(path))
    }

    /// What the photo shows with this look.
    fn key_for(&self, path: &Path, look: Rendition) -> ImageKey {
        let active = self.selected.as_deref() == Some(path);
        let look = if look != Rendition::Current && self.photo_view_settings(path).is_none() {
            // Camera and Defaults need capture metadata, which a Current image brings.
            Rendition::Current
        } else {
            look
        };
        let comparison = active
            && look == Rendition::Current
            && self.comparison.requested()
            && foveonic::inspection::comparison_input(
                &self.settings,
                &self.output,
                self.metadata.as_deref(),
            )
            .is_some();
        ImageKey {
            path: path.to_owned(),
            look,
            output: if active && look == Rendition::Current {
                self.output.clone()
            } else {
                "final".into()
            },
            comparison,
            framing: active && self.framing_preview_active(),
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
            .filter(|view| view.view.magnified())
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
        let mut settings = self.photo_view_settings(path)?;
        settings.working_scale = Scale::NATIVE;
        settings.preview_region = None;
        let policy = self.preferences.processing_quality;
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
            preview_scale: self.preview_scale_for(path, policy),
            processing_quality: quality,
            output: key.output,
            comparison: key.comparison,
        })
    }

    /// How a photo is shown, with its settled region when zoomed or under the loupe.
    fn photo_view(&self, path: &Path, request: &mut RenderRequest) -> Result<View, Unplanned> {
        if self.multi_edit.failed(path) {
            return Err(Unplanned::Failed);
        }
        if request.framing_preview {
            return Ok(View::Framing);
        }
        let images = self.usable_images(path);
        // Without regional support a magnified view needs the native whole image.
        let native_fit = |request: &mut RenderRequest, image: &RenderResult| {
            let regional = request
                .settings
                .regional_preview_available(&image.artifact.metadata)
                && request.output == "final"
                && !request.comparison;
            if !regional {
                request.preview_scale = Scale::NATIVE;
            }
            !regional
        };
        if self.loupe_inspecting(path) {
            if images
                .whole
                .or(images.patch)
                .is_some_and(|image| native_fit(request, image))
            {
                return Ok(View::Fit);
            }
            let placeable = [images.whole, images.patch]
                .into_iter()
                .flatten()
                .any(|image| crate::loupe_view::placeable(image, request));
            let lens = match self.loupe_geometry() {
                Some(geometry) => {
                    Some(self.settled_view(path, (geometry.visible, geometry.region)))
                }
                // The lens is placed once the next frame draws it over a placeable image.
                None if placeable => return Err(Unplanned::Waiting("lens")),
                // Nothing to place it on yet: render a whole-photo patch to place it.
                None => None,
            };
            return Ok(View::Loupe {
                lens,
                whole: self.multi_edit.rendering(path),
            });
        }
        let policy = self.preferences.processing_quality;
        let zoomed = self.viewports.view(Some(path)).explicit_zoom()
            && self.preview_scale_for(path, policy) == Scale::NATIVE;
        if !zoomed {
            return Ok(View::Fit);
        }
        request.preview_scale = self.fit_scale_for(path, policy);
        let Some(whole) = images.whole else {
            return Ok(View::Fit);
        };
        let Some(visible) = self.viewports.view(Some(path)).visible_pixels() else {
            // Selection discards old screen bounds. Wait for this photo to be drawn.
            return Err(Unplanned::Waiting("screen bounds"));
        };
        let window = whole.display_window(false);
        let visible = visible.native_bounds(window);
        if native_fit(request, whole) {
            return Ok(View::Fit);
        }
        let (visible, region) = self.settled_view(path, (visible, render_region(visible, window)));
        Ok(View::Zoomed { visible, region })
    }

    /// The settled view for a moving photo, scheduling a repaint for when it settles.
    fn settled_view(&self, path: &Path, view: ([u32; 4], [u32; 4])) -> ([u32; 4], [u32; 4]) {
        let now = std::time::Instant::now();
        let (settled, wait) = self
            .view_motion
            .borrow_mut()
            .entry(path.to_owned())
            .or_insert_with(|| Motion::new(view, now))
            .settle(view, now);
        if let Some(wait) = wait {
            self.context.request_repaint_after(wait);
        }
        settled
    }

    /// Rendering is not up to date: work is outstanding or a view is still settling.
    pub(crate) fn view_busy(&self) -> bool {
        self.worker.busy() || self.view_motion.borrow().values().any(Motion::settling)
    }

    /// Photos drawn in the viewport for the current browser selection.
    pub(crate) fn shown_photos(&self) -> Vec<PathBuf> {
        self.displayed_paths
            .iter()
            .filter(|path| self.viewport_paths.contains(path))
            .cloned()
            .collect()
    }

    fn displayed_photos(&self) -> Vec<PathBuf> {
        displayed(
            self.multi_edit.render_paths(),
            self.shown_photos(),
            self.loupe_path(),
        )
    }

    /// Plan every displayed photo's images in use and publish them in priority
    /// order: every patch first, the active photo's first, then whole images once
    /// no photo needs a patch, then full size.
    pub(crate) fn submit_photos(&mut self) {
        self.sync_multi_edit();
        self.sync_render_demand();
        let (mut photos, mut unplanned, mut keep_motion) = (Vec::new(), Vec::new(), Vec::new());
        let mut waiting = Vec::new();
        let (mut patches, mut wholes, mut fulls) = (Vec::new(), Vec::new(), Vec::new());
        let mut views = std::collections::HashMap::new();
        let mut previous_views = std::mem::take(&mut self.views);
        let mut request_id = self.last_request_id;
        let mut selected_request = None;
        for path in self.displayed_photos() {
            // An unplanned photo keeps its request, if it has one, for drawing.
            let planned = match self.view_request(&path) {
                None => Err((Unplanned::Waiting("settings"), None)),
                Some(mut request) => match self.photo_view(&path, &mut request) {
                    Ok(view) => Ok(PhotoView { request, view }),
                    Err(unplanned) => Err((unplanned, Some(request))),
                },
            };
            // A request that failed would fail again, so it is not rendered again.
            let done = |image: Option<Use>| {
                image.map(|mut image| {
                    image.ready |= self.images.failed(&image.request);
                    image
                })
            };
            let (patch, whole, full) = match planned {
                Ok(view) => {
                    let Plan { patch, whole, full } = plan(
                        &view.view,
                        &view.request,
                        &self.usable_images(&path),
                        self.preferences.background_full_image_completion,
                    );
                    views.insert(path.clone(), view);
                    // Each image in use is a request of its own, issued in order.
                    let mut issue = |image: Option<Use>| {
                        done(image).map(|mut image| {
                            request_id = request_id.next();
                            image.request.id = request_id;
                            image
                        })
                    };
                    (issue(patch), issue(whole), issue(full))
                }
                Err((Unplanned::Waiting(reason), request)) => {
                    // A waiting photo keeps drawing from its last view mode, with its
                    // current request when it has one, so the loupe can place its lens.
                    let previous = previous_views.remove(&path);
                    let view = match (request, previous) {
                        (Some(request), previous) => Some(PhotoView {
                            request,
                            view: previous.map_or(View::Fit, |previous| previous.view),
                        }),
                        (None, previous) => previous,
                    };
                    if let Some(view) = view {
                        views.insert(path.clone(), view);
                    }
                    keep_motion.push(path.clone());
                    waiting.push((path.clone(), reason));
                    unplanned.push(path);
                    continue;
                }
                Err((Unplanned::Failed, _)) => {
                    waiting.push((path.clone(), "failed"));
                    unplanned.push(path);
                    continue;
                }
            };
            if self.selected.as_ref() == Some(&path) {
                selected_request = patch
                    .as_ref()
                    .or(whole.as_ref())
                    .map(|image| image.request.id);
            }
            if patch.is_some() {
                keep_motion.push(path.clone());
            }
            patches.extend(patch);
            wholes.extend(whole);
            fulls.extend(full);
            photos.push(path);
        }
        self.views = views;
        // A photo that zooms in again starts without motion, so its first patch is immediate.
        self.view_motion
            .get_mut()
            .retain(|path, _| keep_motion.contains(path));
        // The coordinator keeps a replaced request's place in its queue, so whole
        // images wait until no displayed photo needs a patch.
        let patches_needed = patches.iter().any(|patch| !patch.ready);
        patches.sort_by_key(|patch| self.selected.as_ref() != Some(&patch.request.path));
        let planned = patches
            .into_iter()
            .map(|image| (image, false, true))
            .chain(
                wholes
                    .into_iter()
                    .map(|image| (image, false, !patches_needed)),
            )
            .chain(
                fulls
                    .into_iter()
                    .map(|image| (image, true, !patches_needed)),
            )
            .map(|(image, speculative, turn)| Planned {
                tracked: Some(image.request.id) == selected_request,
                render: !image.ready && turn,
                request: image.request,
                speculative,
            })
            .collect();
        let held = super::coordinator::Photos {
            displayed: photos.clone(),
            waiting: unplanned,
            history: self.multi_edit.retained_paths(),
        };
        if self.worker.plan(held, planned, std::time::Instant::now()) {
            foveonic::diagnostics::event(
                foveonic::diagnostics::Category::Coordinator,
                "view.unplanned",
                |fields| {
                    fields.str(
                        "photos",
                        &serde_json::to_string(&waiting).expect("plan diagnostics"),
                    );
                },
            );
            self.last_request_id = request_id;
            if let Some((id, path)) = selected_request.zip(self.selected.clone()) {
                crate::timing::requested(id.0, &path, "final", &self.settings);
            }
        }
        if self
            .selected
            .as_ref()
            .is_some_and(|path| photos.contains(path))
        {
            self.opening = None;
        }
        self.keep_images();
        self.busy = self.view_busy();
    }

    /// Keep each displayed photo's images for what it shows now and with Before
    /// off, and the Current image of displayed and history photos. Drop everything else, so outputs
    /// a photo no longer shows do not hold memory.
    fn keep_images(&mut self) {
        // Before is a shortcut, so what the photo shows with it off is kept too.
        let shown = self
            .displayed_photos()
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

#[cfg(test)]
#[path = "views_test.rs"]
mod tests;
