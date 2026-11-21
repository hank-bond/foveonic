//! Derives each displayed photo's render requests and drawn layers from its view, current request and rendered images.

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

/// How a displayed photo is shown.
pub(crate) enum View {
    Fit,
    /// The crop and rotation plane, which shows the uncropped editing extent.
    Framing,
    /// `visible` must be covered; `region` adds the recipe's margins and is what a patch renders.
    Zoomed {
        visible: [u32; 4],
        region: [u32; 4],
    },
    /// A transient magnifier over a tile. `lens` is the settled visible and patch
    /// region, or `None` before the photo has any image to place it on. `whole` is
    /// false when the tile shows no rendered image of its own.
    Loupe {
        lens: Option<([u32; 4], [u32; 4])>,
        whole: bool,
    },
}
impl View {
    /// Shows a region at native resolution, so a patch can be drawn over the whole image.
    pub fn magnified(&self) -> bool {
        matches!(
            self,
            View::Zoomed { .. } | View::Loupe { lens: Some(_), .. }
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

/// An image in use as published to the coordinator. `render` is false for an
/// image that already exists or waits its turn behind other photos' patches.
#[derive(Clone)]
pub(crate) struct Planned {
    pub request: RenderRequest,
    pub speculative: bool,
    pub render: bool,
    /// The progress bar follows this image: the selected photo's first image.
    pub tracked: bool,
}

/// The images a photo's view uses.
pub(crate) struct Plan {
    pub patch: Option<Use>,
    pub whole: Option<Use>,
    /// Speculative: rendered only when memory and priority allow.
    pub full: Option<Use>,
}

/// `request` carries the current edits and, in `preview_scale`, the scale the
/// photo's tile resolves when not zoomed. `background_completion` is the user's
/// preference for speculative full-size images.
pub(crate) fn plan<T: Rendered>(
    view: &View,
    request: &RenderRequest,
    images: &PhotoImages<T>,
    background_completion: bool,
) -> Plan {
    let tile = request.preview_scale;
    let whole_current = images
        .whole
        .filter(|whole| whole.matches(request) && whole.scale().divisor() <= tile.divisor());
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
    let patch_for = |visible: [u32; 4], region: [u32; 4]| match images.patch.filter(|patch| {
        patch.matches(request)
            && patch
                .region()
                .is_some_and(|covered| super::smart::contains(covered, visible))
    }) {
        Some(patch) => Use {
            request: derive(request, Scale::NATIVE, patch.requested_region()),
            ready: true,
        },
        None => Use {
            request: derive(request, Scale::NATIVE, Some(region)),
            ready: false,
        },
    };
    match view {
        View::Fit | View::Framing => Plan {
            patch: None,
            whole,
            full: None,
        },
        // A current full-size image covers every region.
        View::Zoomed { .. } | View::Loupe { lens: Some(_), .. } if native_current => Plan {
            patch: None,
            whole,
            full: None,
        },
        View::Zoomed { visible, region } => {
            let patch = patch_for(*visible, *region);
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
        View::Loupe { lens, whole: shown } => {
            let patch = match lens {
                Some((visible, region)) => patch_for(*visible, *region),
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
                whole: whole.filter(|_| *shown),
                full: None,
            }
        }
    }
}

impl Rendered for super::worker::RenderResult {
    fn matches(&self, request: &RenderRequest) -> bool {
        self.request.path == request.path
            && self.request.rendition == request.rendition
            && self.request.output == request.output
            && self.processing_quality() == request.processing_quality
            && self.request.framing_preview == request.framing_preview
            && self.request.comparison == request.comparison
            && self.calibration_signature()
                == request
                    .calibration
                    .as_ref()
                    .map(|profile| profile.signature())
            && super::matching::same_image_geometry(&self.request.settings, &request.settings)
            && super::matching::same_authored_edits(&self.request.settings, &request.settings)
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
    latest: ([u32; 4], [u32; 4]),
    settled: ([u32; 4], [u32; 4]),
    changed: std::time::Instant,
}
impl Motion {
    /// A view that has not moved before settles at once.
    pub(crate) fn new(view: ([u32; 4], [u32; 4]), now: std::time::Instant) -> Self {
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
        view: ([u32; 4], [u32; 4]),
        now: std::time::Instant,
    ) -> (([u32; 4], [u32; 4]), Option<std::time::Duration>) {
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
/// whole image with the same edits came from one plan; the patch is sharper.
pub(crate) fn patch_on_top<T: Rendered>(
    patch: &T,
    whole: Option<&T>,
    current: impl Fn(&T) -> bool,
) -> bool {
    current(patch)
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
