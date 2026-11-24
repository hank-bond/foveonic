//! Derives each viewport photo's render requests and drawn layers from its view, current request and rendered images.

use super::request::RenderRequest;
use crate::rendering::request::RequestId;
use foveonic::render::Scale;

/// What the planner needs to know about a rendered image.
pub(crate) trait Rendered {
    /// Same source, rendition, policy and authored edits as the request.
    fn matches(&self, request: &RenderRequest) -> bool;
    /// Rendered with the same authored edits as `other`.
    fn same_edits(&self, other: &Self) -> bool;
    /// The id of the request it answers; a larger id is a later request.
    fn request_id(&self) -> RequestId;
    fn scale(&self) -> Scale;
    /// Native image bounds a patch covers. `None` for a whole image.
    fn region(&self) -> Option<[u32; 4]>;
    /// The region the patch was requested for, which identifies its work.
    fn requested_region(&self) -> Option<[u32; 4]>;
}

/// The part of a photo on screen and the part a patch renders for it, in
/// native pixels. The patch region adds the recipe's margins to the visible region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Regions {
    pub visible: [u32; 4],
    pub patch: [u32; 4],
}

/// How a viewport photo is shown.
pub(crate) enum ViewMode {
    Fit,
    /// The crop and rotation plane, which shows the uncropped editing extent.
    Framing,
    /// Zoomed to native detail at the settled regions.
    Zoomed(Regions),
    /// A transient magnifier over the photo at the settled loupe regions, or
    /// `None` before the photo's metadata places the lens.
    Loupe {
        regions: Option<Regions>,
    },
}
impl ViewMode {
    /// Shows a region at native resolution, so a patch can be drawn over the whole image.
    pub fn magnified(&self) -> bool {
        matches!(
            self,
            ViewMode::Zoomed(_)
                | ViewMode::Loupe {
                    regions: Some(_),
                    ..
                }
        )
    }
}

/// A photo's stored images for what it shows: its whole image (reduced or full
/// size, whichever arrived last) and its patch.
pub(crate) struct PhotoImages<'a, T> {
    pub whole: Option<&'a T>,
    pub patch: Option<&'a T>,
}

/// An image a view uses. When `ready`, a stored image already matches
/// `request`, so the coordinator keeps the work that made it and starts none.
#[derive(Clone)]
pub(crate) struct Use {
    pub request: RenderRequest,
    pub ready: bool,
}

/// Where an image in use stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageState {
    /// A stored image is current; the coordinator keeps the work that made it.
    Ready,
    /// Rendered in the plan's order.
    Render,
}

/// An image in use as published to the coordinator.
#[derive(Clone)]
pub(crate) struct Planned {
    pub request: RenderRequest,
    pub speculative: bool,
    pub state: ImageState,
    /// The progress pass follows this image, at this place in the pass: each
    /// edited photo's first image, the selected photo's first.
    pub tracked: Option<usize>,
}

/// The images a photo's view uses.
pub(crate) struct Plan {
    pub patch: Option<Use>,
    pub whole: Option<Use>,
    /// Speculative: rendered only when memory and priority allow.
    pub full: Option<Use>,
}

/// A whole image is current for a view when it has the view's edits and is at
/// least as fine as the tile needs.
pub(crate) fn whole_current<T: Rendered>(whole: &T, request: &RenderRequest) -> bool {
    whole.matches(request) && whole.scale().divisor() <= request.preview_scale.divisor()
}

/// `request` carries the current edits and, in `preview_scale`, the scale the
/// photo's tile resolves when not zoomed. `background_completion` is the user's
/// preference for speculative full-size images.
pub(crate) fn plan<T: Rendered>(
    view: &ViewMode,
    request: &RenderRequest,
    images: &PhotoImages<T>,
    background_completion: bool,
) -> Plan {
    let tile = request.preview_scale;
    let whole_current = images.whole.filter(|whole| whole_current(*whole, request));
    let native_current = whole_current.is_some_and(|whole| whole.scale().is_native());
    // A finer image than the view needs is shown, but only a zoomed or loupe view
    // covered by a full-size image keeps the native work that made it, so its
    // memory can go.
    let in_use = if native_current && view.magnified() {
        Scale::NATIVE
    } else {
        tile
    };
    let whole = Some(Use {
        request: derive(request, in_use, None),
        ready: whole_current.is_some(),
    });
    // A patch for `region` is current when an existing patch with the current
    // edits covers `visible`.
    let patch_for = |regions: Regions| match images.patch.filter(|patch| {
        patch.matches(request)
            && patch
                .region()
                .is_some_and(|covered| super::smart::contains(covered, regions.visible))
    }) {
        Some(patch) => Use {
            request: derive(request, Scale::NATIVE, patch.requested_region()),
            ready: true,
        },
        None => Use {
            request: derive(request, Scale::NATIVE, Some(regions.patch)),
            ready: false,
        },
    };
    match view {
        ViewMode::Fit | ViewMode::Framing => Plan {
            patch: None,
            whole,
            full: None,
        },
        // A current full-size image covers every region.
        ViewMode::Zoomed(_)
        | ViewMode::Loupe {
            regions: Some(_), ..
        } if native_current => Plan {
            patch: None,
            whole,
            full: None,
        },
        ViewMode::Zoomed(regions) => {
            let patch = patch_for(*regions);
            let full =
                (background_completion && patch.ready && whole_current.is_some()).then(|| Use {
                    request: derive(request, Scale::NATIVE, None),
                    ready: false,
                });
            Plan {
                patch: Some(patch),
                whole,
                full,
            }
        }
        ViewMode::Loupe { regions } => {
            let patch = match regions {
                Some(regions) => patch_for(*regions),
                // A whole-photo patch at the tile's scale places the lens.
                None => {
                    let mut placement = derive(request, tile, None);
                    placement.detail = true;
                    Use {
                        ready: images.patch.is_some_and(|patch| patch.matches(request)),
                        request: placement,
                    }
                }
            };
            Plan {
                patch: Some(patch),
                whole,
                full: None,
            }
        }
    }
}

/// An image rendered for `rendered` at `quality` has the same source, rendition,
/// policy, calibration and authored edits as `request`.
pub(crate) fn answers(
    rendered: &RenderRequest,
    quality: foveonic::render::ProcessingQuality,
    request: &RenderRequest,
) -> bool {
    fn signature(request: &RenderRequest) -> Option<&str> {
        request
            .calibration
            .as_deref()
            .map(|profile| profile.signature())
    }
    rendered.path == request.path
        && rendered.rendition == request.rendition
        && rendered.output == request.output
        && quality == request.processing_quality
        && rendered.framing_preview == request.framing_preview
        && rendered.comparison == request.comparison
        && signature(rendered) == signature(request)
        && super::matching::same_image_geometry(&rendered.settings, &request.settings)
        && super::matching::same_authored_edits(&rendered.settings, &request.settings)
}

impl Rendered for super::worker::RenderResult {
    fn matches(&self, request: &RenderRequest) -> bool {
        answers(&self.request, self.processing_quality(), request)
    }
    fn same_edits(&self, other: &Self) -> bool {
        super::matching::same_authored_edits(&self.request.settings, &other.request.settings)
    }
    fn request_id(&self) -> RequestId {
        self.request.id
    }
    fn scale(&self) -> Scale {
        self.artifact.settings.working_scale
    }
    fn region(&self) -> Option<[u32; 4]> {
        self.artifact.settings.preview_region
    }
    fn requested_region(&self) -> Option<[u32; 4]> {
        self.request.settings.preview_region
    }
}

/// How long a zoomed view's visible region must stay unchanged before a patch is
/// planned for it. People release the pointer 100–200 ms after they stop moving.
pub(crate) const SETTLE: std::time::Duration = std::time::Duration::from_millis(120);

/// A zoomed photo's view while it moves: its visible region and the region a
/// patch would render for it.
pub(crate) struct Motion {
    latest: Regions,
    settled: Regions,
    changed: std::time::Instant,
}
impl Motion {
    /// A view that has not moved before settles at once.
    pub(crate) fn new(view: Regions, now: std::time::Instant) -> Self {
        Self {
            latest: view,
            settled: view,
            changed: now,
        }
    }

    /// The latest view has not rested long enough to be planned.
    pub(crate) fn settling(&self) -> bool {
        self.settled != self.latest
    }

    /// The settled view, and how long until the latest view settles if it has not yet.
    pub(crate) fn settle(
        &mut self,
        view: Regions,
        now: std::time::Instant,
    ) -> (Regions, Option<std::time::Duration>) {
        if view != self.latest {
            self.latest = view;
            self.changed = now;
        }
        let rested = now.saturating_duration_since(self.changed);
        if rested >= SETTLE {
            self.settled = self.latest;
        }
        (
            self.settled,
            (self.settled != self.latest).then(|| SETTLE - rested),
        )
    }
}

/// The latest patch stays on top unless the whole image behind it already has
/// newer edits, so a slider move never exposes a blurrier image. A patch and a
/// whole image with the same edits came from one plan; the patch is sharper,
/// unless the whole image is a current full-size one, which the plan uses for
/// every region instead.
pub(crate) fn patch_on_top<T: Rendered>(
    patch: &T,
    whole: Option<&T>,
    current: impl Fn(&T) -> bool,
) -> bool {
    let full_current = whole.is_some_and(|whole| current(whole) && whole.scale().is_native());
    (current(patch) && !full_current)
        || whole.is_none_or(|whole| {
            !current(whole) && (patch.same_edits(whole) || patch.request_id() > whole.request_id())
        })
}

fn derive(request: &RenderRequest, scale: Scale, region: Option<[u32; 4]>) -> RenderRequest {
    let mut derived = request.clone();
    derived.detail = region.is_some();
    derived.preview_scale = scale;
    derived.settings.working_scale = Scale::NATIVE;
    derived.settings.preview_region = region;
    derived
}

#[cfg(test)]
#[path = "view_plan_test.rs"]
mod tests;
