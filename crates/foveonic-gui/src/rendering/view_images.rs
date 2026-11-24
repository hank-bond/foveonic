//! Holds every viewport photo's view, its rendered images and the planning state carried between frames, without a GPU.

use super::{
    coordinator::Photos,
    quality,
    rendered::{ImageKey, RenderedImages, Stored},
    request::{RenderRequest, RequestId},
    view_plan::{patch_on_top, Motion, PhotoImages, Planned},
    viewport_plan::{plan_photos, PhotoInputs, PhotoView, Placement, Planning, Publication},
};
use crate::editor::Rendition;
use foveonic::{
    noise_calibration::Calibration,
    render::{ProcessingQuality, Scale, Settings},
    source::SourceOrigin,
};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// A stored image is usable for a photo when it was made from the photo's
/// current source version.
fn usable<'a, T: Stored>(image: Option<&'a T>, origin: Option<&SourceOrigin>) -> Option<&'a T> {
    image.filter(|image| origin.is_none_or(|origin| origin == image.origin()))
}

/// How the app shows one photo, read off the app as plain values.
#[derive(Clone)]
pub(crate) struct PhotoLooks {
    pub path: PathBuf,
    /// The look asked for: Before's look for the editing selection, else Current.
    pub look: Rendition,
    /// Settings for `look` once they resolve. Camera and Defaults need capture metadata.
    pub look_settings: Option<Settings>,
    /// The Current look's settings once loaded. Read only while `look_settings`
    /// is `None`, so the app may skip it when the look resolves.
    pub current_settings: Option<Settings>,
    /// The inspection output chosen for the viewport.
    pub output: String,
    /// The selected photo's output was already normalized for its own camera.
    pub selected: bool,
    /// The outputs the photo's camera offers; empty until its settings and
    /// metadata are known. Read only for an inspection output on a photo that
    /// is not selected, so the app may skip listing them otherwise.
    pub offered: Vec<&'static str>,
    /// Comparison is on and the photo has an input to compare `output` with.
    pub compares: bool,
    /// The photo shows the crop and rotation plane.
    pub framing: bool,
}
impl PhotoLooks {
    /// The look the photo shows and its settings: the asked-for look, or Current
    /// while Camera or Defaults cannot be resolved yet, as before capture metadata
    /// arrives with a Current image.
    pub fn shown(&self) -> (Rendition, Option<&Settings>) {
        match &self.look_settings {
            Some(settings) if self.look != Rendition::Current => (self.look, Some(settings)),
            _ => (Rendition::Current, self.current_settings.as_ref()),
        }
    }

    /// What the photo shows.
    pub fn key(&self) -> ImageKey {
        self.key_for(self.shown().0)
    }

    /// What the photo shows with this look. The chosen inspection output applies
    /// to every photo whose camera offers it; other photos show their final photograph.
    pub fn key_for(&self, look: Rendition) -> ImageKey {
        let applies =
            self.output == "final" || self.selected || self.offered.contains(&self.output.as_str());
        let current = look == Rendition::Current && applies;
        ImageKey {
            path: self.path.clone(),
            look,
            output: if current {
                self.output.clone()
            } else {
                "final".into()
            },
            comparison: current && self.compares,
            framing: self.framing,
        }
    }
}

/// One viewport photo as the app reads it this frame.
pub(crate) struct PhotoFacts {
    pub looks: PhotoLooks,
    /// The photo's current source version. Images from another version are not this photo's.
    pub origin: Option<SourceOrigin>,
    /// The scale its view needs now; `None` until its settings and metadata summary load.
    pub preview_scale: Option<Scale>,
    pub placement: Placement,
}

/// What the app sets for this round of planning.
pub(crate) struct Round<'a> {
    pub selected: Option<&'a Path>,
    /// The editing selection, whose update the progress bar follows.
    pub editing: &'a [PathBuf],
    /// Photos that left the viewport and keep their Current image.
    pub history: Vec<PathBuf>,
    pub policy: ProcessingQuality,
    pub calibration: Option<Arc<Calibration>>,
    pub background_completion: bool,
    pub now: Instant,
}

/// What the app follows up on after a round of planning.
pub(crate) struct Outcome {
    /// The selected photo's first image, when the plan was published.
    pub selected_request: Option<RequestId>,
    /// When a moving view will have rested long enough to plan.
    pub repaint_after: Option<Duration>,
    /// The photos planned this round.
    pub planned: Vec<PathBuf>,
}

/// Every viewport photo's view, the rendered images the app holds and what
/// planning carries from one frame to the next.
pub(crate) struct ViewImages<T> {
    /// Every rendered image the app holds, keyed by photo, look and output.
    images: RenderedImages<T>,
    /// Each viewport photo's view as last planned.
    views: HashMap<PathBuf, PhotoView>,
    /// Zoomed photos' moving regions.
    motion: BTreeMap<PathBuf, Motion>,
    /// The last request id issued.
    last_request_id: RequestId,
    /// The last plan had photos whose settings or metadata summaries were still being read.
    loading: bool,
}
impl<T> Default for ViewImages<T> {
    fn default() -> Self {
        Self {
            images: RenderedImages::default(),
            views: HashMap::new(),
            motion: BTreeMap::new(),
            last_request_id: RequestId::default(),
            loading: false,
        }
    }
}
impl<T: Stored> ViewImages<T> {
    /// The last request id issued.
    pub fn last_request_id(&self) -> RequestId {
        self.last_request_id
    }

    /// Issue the next request id outside planning, which marks a selection in
    /// timing diagnostics.
    pub fn issue_request_id(&mut self) -> RequestId {
        self.last_request_id = self.last_request_id.next();
        self.last_request_id
    }

    pub fn view(&self, path: &Path) -> Option<&PhotoView> {
        self.views.get(path)
    }

    /// A view is still settling, or the last plan had photos still loading.
    pub fn waiting(&self) -> bool {
        self.motion.values().any(Motion::settling) || self.loading
    }

    pub fn images(&self) -> impl Iterator<Item = &T> {
        self.images.images()
    }

    /// Every image made from its photo's current source version, as `origin` reports it.
    pub fn usable_images<'a>(
        &'a self,
        origin: impl Fn(&Path) -> Option<SourceOrigin> + 'a,
    ) -> impl Iterator<Item = &'a T> {
        self.images
            .images()
            .filter(move |image| usable(Some(*image), origin(&image.key().path).as_ref()).is_some())
    }

    /// Store a delivered image unless the stored image of the same kind is newer.
    pub fn accept(&mut self, image: T) -> bool {
        self.images.accept(image)
    }

    pub fn fail(&mut self, request: Arc<RenderRequest>) {
        self.images.fail(request);
    }

    pub fn clear_images(&mut self) {
        self.images.clear();
    }

    /// The whole image drawn for a key, if usable.
    pub fn whole(&self, key: &ImageKey, origin: Option<&SourceOrigin>) -> Option<&T> {
        usable(self.images.whole(key), origin)
    }

    /// A photo's whole image for its Current look, whatever it shows now.
    pub fn current_image(&self, path: &Path) -> Option<&T> {
        self.images.get(&ImageKey::current(path))?.whole.as_ref()
    }

    /// What is drawn for a key: its whole image, and its patch on top when the
    /// photo's view is magnified and the layer rule allows.
    pub fn layers(&self, key: &ImageKey, origin: Option<&SourceOrigin>) -> [Option<&T>; 2] {
        let images = photo_images(&self.images, key, origin);
        let patch = self
            .views
            .get(&key.path)
            .filter(|view| view.mode.magnified())
            .and_then(|view| {
                images.patch.filter(|patch| {
                    patch_on_top(*patch, images.whole, |image| image.matches(&view.request))
                })
            });
        [images.whole, patch]
    }

    /// Plan every viewport photo's images in use, keep only the images still
    /// needed and hand the publication to `publish`, which reports whether it
    /// changed. Ids advance only when a changed publication was sent.
    pub fn plan(
        &mut self,
        photos: Vec<PhotoFacts>,
        round: Round<'_>,
        publish: impl FnOnce(Photos, Vec<Planned>) -> bool,
    ) -> Outcome {
        // Before is a shortcut, so what a photo shows with it off is kept too.
        let kept = photos
            .iter()
            .flat_map(|photo| [photo.looks.key(), photo.looks.key_for(Rendition::Current)])
            .collect::<Vec<_>>();
        let previous_views = std::mem::take(&mut self.views);
        let inputs = photos
            .into_iter()
            .map(|photo| inputs(&self.images, photo, &round))
            .collect();
        let Publication {
            mut photos,
            planned,
            views,
            unplanned,
            last_request_id,
            selected_request,
            repaint_after,
        } = plan_photos(
            inputs,
            previous_views,
            &mut self.motion,
            |request| self.images.failed(request),
            Planning {
                selected: round.selected,
                editing: round.editing,
                last_request_id: self.last_request_id,
                background_completion: round.background_completion,
                now: round.now,
            },
        );
        self.views = views;
        self.loading = !photos.loading.is_empty();
        self.keep(&kept, &round.history);
        photos.history = round.history;
        let viewport = photos.planned.clone();
        let published = publish(photos, planned);
        if published {
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
        }
        Outcome {
            selected_request: selected_request.filter(|_| published),
            repaint_after,
            planned: viewport,
        }
    }

    /// Keep each viewport photo's images for what it shows now and with Before
    /// off, and the Current image of viewport and history photos. Drop everything
    /// else, so outputs a photo no longer shows do not hold memory.
    fn keep(&mut self, shown: &[ImageKey], history: &[PathBuf]) {
        self.images.retain(|key| {
            shown.contains(key)
                || ((history.contains(&key.path)
                    || shown.iter().any(|shown| shown.path == key.path))
                    && *key == ImageKey::current(&key.path))
        });
    }
}

/// The photo's stored images for what it shows. The Current image standing in
/// for another look does not match that look's request, so the planner renders it.
fn photo_images<'a, T: Stored>(
    images: &'a RenderedImages<T>,
    key: &ImageKey,
    origin: Option<&SourceOrigin>,
) -> PhotoImages<'a, T> {
    let patch = images.get(key).and_then(|images| images.patch.as_ref());
    PhotoImages {
        whole: usable(images.whole(key), origin),
        patch: usable(patch, origin),
    }
}

/// What planning needs about one viewport photo.
fn inputs<'a, T: Stored>(
    images: &'a RenderedImages<T>,
    photo: PhotoFacts,
    round: &Round<'_>,
) -> PhotoInputs<'a, T> {
    let key = photo.looks.key();
    let images = photo_images(images, &key, photo.origin.as_ref());
    let request = view_request(&photo.looks, photo.preview_scale, round);
    let regional = request.as_ref().and_then(|request| {
        images.whole.or(images.patch).map(|image| {
            image.supports_regions(&request.settings)
                && request.output == "final"
                && !request.comparison
        })
    });
    PhotoInputs {
        window: images.whole.map(Stored::window),
        path: key.path,
        request,
        images,
        regional,
        placement: photo.placement,
    }
}

/// The request a photo's images are planned from: its look's settings, output
/// and quality at the scale its tile resolves when not zoomed. Its images get
/// their own ids when they are planned.
fn view_request(
    looks: &PhotoLooks,
    preview_scale: Option<Scale>,
    round: &Round<'_>,
) -> Option<RenderRequest> {
    let key = looks.key();
    let preview_scale = preview_scale?;
    let mut settings = looks.shown().1?.clone();
    settings.working_scale = Scale::NATIVE;
    settings.preview_region = None;
    let quality = quality::for_view(round.policy, key.look, &key.output, key.comparison);
    settings.processing_quality = quality;
    Some(RenderRequest {
        detail: false,
        framing_preview: key.framing,
        calibration: round.calibration.clone(),
        rendition: key.look,
        id: RequestId::default(),
        path: key.path,
        settings,
        preview_scale,
        processing_quality: quality,
        output: key.output,
        comparison: key.comparison,
    })
}

#[cfg(test)]
#[path = "view_images_test.rs"]
mod tests;
