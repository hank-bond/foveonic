//! Plans every viewport photo's images from inputs read off the app, without a GPU.

use super::{
    coordinator::Photos,
    request::{RenderRequest, RequestId},
    smart::patch_region,
    view_plan::{
        plan, ImageState, Motion, PhotoImages, Plan, Planned, Regions, Rendered, Use, ViewMode,
    },
};
use crate::viewport::PixelRegion;
use foveonic::render::Scale;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// A viewport photo's view as last planned: the request its images are planned
/// from and how it is shown. Planning runs after drawing, so a frame draws the
/// views planned at the end of the frame before.
pub(crate) struct PhotoView {
    /// The view's template request. Its id is unset: each image planned from it
    /// gets its own id, so compare images with `matches`, not by id.
    pub request: RenderRequest,
    pub mode: ViewMode,
}

/// How the viewport shows a photo before planning settles it.
pub(crate) enum Placement {
    Fit,
    /// Zoomed in far enough to need native detail. `visible` is unknown when
    /// the photo was not drawn this frame.
    Zoomed {
        visible: Option<PixelRegion>,
        /// The scale the photo's tile resolves when not zoomed.
        fit_scale: Scale,
    },
    /// Under the loupe. `regions` is unknown until the photo's metadata is known.
    Loupe {
        regions: Option<Regions>,
    },
}

/// What planning needs to know about one viewport photo.
pub(crate) struct PhotoInputs<'a, T> {
    pub path: PathBuf,
    /// The request its images are planned from; `None` until its settings load.
    pub request: Option<RenderRequest>,
    /// Its settings could not be resolved.
    pub failed: bool,
    pub images: PhotoImages<'a, T>,
    /// Whether the photo's rendered image supports regional patches for its
    /// request; `None` while it has no image.
    pub regional: Option<bool>,
    /// The native window of its whole image, which visible pixels are measured in.
    pub window: Option<[u32; 4]>,
    pub placement: Placement,
}

/// What the app sets for this round of planning.
pub(crate) struct Planning<'a> {
    pub selected: Option<&'a Path>,
    /// The editing selection, whose update the progress bar follows.
    pub editing: &'a [PathBuf],
    pub last_request_id: RequestId,
    pub background_completion: bool,
    pub now: Instant,
}

/// The result of planning every viewport photo.
pub(crate) struct Publication {
    /// Viewport and loading photos; the caller adds history photos.
    pub photos: Photos,
    pub planned: Vec<Planned>,
    pub views: HashMap<PathBuf, PhotoView>,
    /// Each loading or failed photo with its reason, for diagnostics.
    pub unplanned: Vec<(PathBuf, &'static str)>,
    pub last_request_id: RequestId,
    /// The selected photo's first image, which leads the progress pass.
    pub selected_request: Option<RequestId>,
    /// When a moving view will have rested long enough to plan.
    pub repaint_after: Option<Duration>,
}

/// Plan every viewport photo's images in use and order them for publication:
/// every patch first, the selected photo's first, then whole images once no
/// photo needs a patch, then full size.
pub(crate) fn plan_photos<T: Rendered>(
    inputs: Vec<PhotoInputs<'_, T>>,
    mut previous_views: HashMap<PathBuf, PhotoView>,
    motion: &mut BTreeMap<PathBuf, Motion>,
    failed: impl Fn(&RenderRequest) -> bool,
    planning: Planning<'_>,
) -> Publication {
    let (mut viewport, mut loading, mut keep_motion) = (Vec::new(), Vec::new(), Vec::new());
    let mut unplanned = Vec::new();
    let (mut patches, mut wholes, mut fulls) = (Vec::new(), Vec::new(), Vec::new());
    let mut views = HashMap::new();
    let mut request_id = planning.last_request_id;
    let mut selected_request = None;
    let mut tracked = Vec::new();
    let mut repaint_after: Option<Duration> = None;
    for input in inputs {
        let path = input.path.clone();
        let mut settle = |regions: Regions| {
            let (settled, wait) = motion
                .entry(path.clone())
                .or_insert_with(|| Motion::new(regions, planning.now))
                .settle(regions, planning.now);
            if let Some(wait) = wait {
                repaint_after = Some(repaint_after.map_or(wait, |earlier| earlier.min(wait)));
            }
            settled
        };
        // A photo whose settings could not be read plans nothing, so its work stops.
        if input.failed {
            unplanned.push((path.clone(), "failed"));
            viewport.push(path);
            continue;
        }
        // A loading photo keeps its work and draws from its last view.
        let Some(mut request) = input.request.clone() else {
            if let Some(view) = previous_views.remove(&path) {
                views.insert(path.clone(), view);
            }
            keep_motion.push(path.clone());
            unplanned.push((path.clone(), "loading"));
            loading.push(path);
            continue;
        };
        let view = PhotoView {
            mode: view_mode(&input, &mut request, &mut settle),
            request,
        };
        // A request that failed would fail again, so it is not rendered again.
        let done = |image: Option<Use>| {
            image.map(|mut image| {
                image.ready |= failed(&image.request);
                image
            })
        };
        let Plan { patch, whole, full } = plan(
            &view.mode,
            &view.request,
            &input.images,
            planning.background_completion,
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
        let (patch, whole, full) = (issue(patch), issue(whole), issue(full));
        let first = patch
            .as_ref()
            .or(whole.as_ref())
            .map(|image| image.request.id);
        if planning.selected == Some(path.as_path()) {
            selected_request = first;
            if let Some(first) = first {
                tracked.insert(0, first);
            }
        } else if planning.editing.contains(&path) {
            tracked.extend(first);
        }
        if patch.is_some() {
            keep_motion.push(path.clone());
        }
        patches.extend(patch);
        wholes.extend(whole);
        fulls.extend(full);
        viewport.push(path);
    }
    // A photo that zooms in again starts without motion, so its first patch is immediate.
    motion.retain(|path, _| keep_motion.contains(path));
    // The coordinator keeps a replaced request's place in its queue, so whole
    // images wait until no viewport photo needs a patch.
    let patches_needed = patches.iter().any(|patch| !patch.ready);
    patches.sort_by_key(|patch| planning.selected != Some(patch.request.path.as_path()));
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
            tracked: tracked.iter().position(|id| *id == image.request.id),
            state: if image.ready {
                ImageState::Ready
            } else if turn {
                ImageState::Render
            } else {
                ImageState::Waiting
            },
            request: image.request,
            speculative,
        })
        .collect();
    Publication {
        photos: Photos {
            planned: viewport,
            loading,
            history: Vec::new(),
        },
        planned,
        views,
        unplanned,
        last_request_id: request_id,
        selected_request,
        repaint_after,
    }
}

/// How a photo is shown, with its settled regions when zoomed or under the loupe.
fn view_mode<T>(
    input: &PhotoInputs<'_, T>,
    request: &mut RenderRequest,
    settle: &mut impl FnMut(Regions) -> Regions,
) -> ViewMode {
    if request.framing_preview {
        return ViewMode::Framing;
    }
    // Without regional support a magnified view needs the native whole image.
    let native_fit = |request: &mut RenderRequest| {
        let native = input.regional == Some(false);
        if native {
            request.preview_scale = Scale::NATIVE;
        }
        native
    };
    match &input.placement {
        Placement::Fit => ViewMode::Fit,
        Placement::Loupe { regions } => {
            if native_fit(request) {
                return ViewMode::Fit;
            }
            // Without regions, a whole-photo patch at the tile's scale places the lens.
            ViewMode::Loupe {
                regions: regions.map(&mut *settle),
            }
        }
        Placement::Zoomed { visible, fit_scale } => {
            request.preview_scale = *fit_scale;
            let Some(window) = input.window else {
                return ViewMode::Fit;
            };
            // Without screen bounds the photo was not drawn this frame, such as an
            // edited photo beyond the grid, so only its whole image is needed.
            let Some(visible) = visible else {
                return ViewMode::Fit;
            };
            let visible = visible.native_bounds(window);
            if native_fit(request) {
                return ViewMode::Fit;
            }
            ViewMode::Zoomed(settle(Regions {
                visible,
                patch: patch_region(visible, window),
            }))
        }
    }
}

#[cfg(test)]
#[path = "viewport_plan_test.rs"]
mod tests;
