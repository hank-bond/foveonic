//! Checks each view-planning rule as an input-to-output example.

use super::*;
use crate::editor::Rendition;
use crate::rendering::request::RequestId;
use foveonic::render::{ProcessingQuality, Settings};

const VISIBLE: [u32; 4] = [100, 100, 500, 400];
const REGION: [u32; 4] = [80, 80, 520, 420];
fn quarter() -> Scale {
    Scale::new(4).unwrap()
}

struct Image {
    exposure: f32,
    revision: u64,
    scale: Scale,
    region: Option<[u32; 4]>,
}
impl Rendered for Image {
    fn matches(&self, request: &RenderRequest) -> bool {
        self.exposure == request.settings.exposure
    }
    fn same_edits(&self, other: &Self) -> bool {
        self.exposure == other.exposure
    }
    fn request_id(&self) -> RequestId {
        RequestId(self.revision)
    }
    fn scale(&self) -> Scale {
        self.scale
    }
    fn region(&self) -> Option<[u32; 4]> {
        self.region
    }
    fn requested_region(&self) -> Option<[u32; 4]> {
        self.region
    }
}
fn reduced(exposure: f32, revision: u64) -> Option<Image> {
    Some(Image {
        exposure,
        revision,
        scale: quarter(),
        region: None,
    })
}
fn patch(exposure: f32, revision: u64, region: [u32; 4]) -> Option<Image> {
    Some(Image {
        exposure,
        revision,
        scale: Scale::NATIVE,
        region: Some(region),
    })
}
fn full(exposure: f32, revision: u64) -> Option<Image> {
    Some(Image {
        exposure,
        revision,
        scale: Scale::NATIVE,
        region: None,
    })
}
fn request(exposure: f32) -> RenderRequest {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.exposure = exposure;
    RenderRequest {
        detail: false,
        framing_preview: false,
        calibration: None,
        rendition: Rendition::Current,
        id: RequestId(10),
        path: "photo.x3f".into(),
        settings,
        preview_scale: quarter(),
        processing_quality: ProcessingQuality::Speed,
        output: "final".into(),
        comparison: false,
    }
}
const ZOOMED: ViewMode = ViewMode::Zoomed(Regions {
    visible: VISIBLE,
    patch: REGION,
});

fn plan_for(
    view: &ViewMode,
    request: &RenderRequest,
    whole: Option<Image>,
    patch: Option<Image>,
) -> Plan {
    let images = PhotoImages {
        whole: whole.as_ref(),
        patch: patch.as_ref(),
    };
    plan(view, request, &images, true)
}

fn needed(image: &Option<Use>) -> Option<&RenderRequest> {
    image
        .as_ref()
        .filter(|image| !image.ready)
        .map(|image| &image.request)
}

#[test]
fn an_edit_while_zoomed_needs_the_patch_and_the_reduced_image() {
    let plan = plan_for(&ZOOMED, &request(1.), reduced(0., 1), patch(0., 2, REGION));
    let patch = needed(&plan.patch).expect("patch");
    assert!(patch.detail);
    assert_eq!(patch.preview_scale, Scale::NATIVE);
    assert_eq!(patch.settings.preview_region, Some(REGION));
    let whole = needed(&plan.whole).expect("whole");
    assert!(!whole.detail);
    assert_eq!(whole.preview_scale, quarter());
    assert_eq!(whole.settings.preview_region, None);
    assert!(
        plan.full.is_none(),
        "full size waits for patch and whole image"
    );
}

#[test]
fn an_edit_replaces_an_outdated_full_size_image_with_a_reduced_one() {
    let plan = plan_for(&ZOOMED, &request(1.), full(0., 3), patch(0., 2, REGION));
    assert_eq!(needed(&plan.whole).expect("whole").preview_scale, quarter());
}

#[test]
fn a_patch_that_no_longer_covers_the_view_is_replaced() {
    let plan = plan_for(
        &ZOOMED,
        &request(0.),
        reduced(0., 1),
        patch(0., 2, [300, 300, 700, 600]),
    );
    assert_eq!(
        needed(&plan.patch).expect("patch").settings.preview_region,
        Some(REGION)
    );
    assert!(plan.whole.as_ref().is_some_and(|whole| whole.ready));
    assert!(plan.full.is_none());
}

#[test]
fn ready_images_are_identified_by_the_work_that_made_them() {
    let covering = [60, 60, 560, 460];
    let plan = plan_for(
        &ZOOMED,
        &request(0.),
        reduced(0., 1),
        patch(0., 2, covering),
    );
    let patch = plan.patch.expect("patch");
    assert!(patch.ready);
    assert_eq!(patch.request.settings.preview_region, Some(covering));
    let whole = plan.whole.expect("whole");
    assert!(whole.ready);
    assert_eq!(whole.request.preview_scale, quarter());
}

#[test]
fn full_size_follows_once_patch_and_whole_image_are_current() {
    let plan = plan_for(&ZOOMED, &request(0.), reduced(0., 1), patch(0., 2, REGION));
    let full = needed(&plan.full).expect("full");
    assert_eq!(full.preview_scale, Scale::NATIVE);
    assert_eq!(full.settings.preview_region, None);
    assert!(!full.detail);
}

#[test]
fn full_size_follows_the_background_completion_preference() {
    let (whole, patch) = (reduced(0., 1), patch(0., 2, REGION));
    let images = PhotoImages {
        whole: whole.as_ref(),
        patch: patch.as_ref(),
    };
    assert!(plan(&ZOOMED, &request(0.), &images, false).full.is_none());
}

#[test]
fn a_current_full_size_image_covers_every_region_without_a_patch() {
    let plan = plan_for(&ZOOMED, &request(0.), full(0., 3), None);
    assert!(plan.patch.is_none() && plan.full.is_none());
    let whole = plan.whole.expect("whole");
    assert!(whole.ready);
    assert_eq!(whole.request.preview_scale, Scale::NATIVE);
}

#[test]
fn a_finer_whole_image_in_fit_view_is_in_use_only_at_the_tile_s_scale() {
    let plan = plan_for(&ViewMode::Fit, &request(0.), full(0., 3), None);
    let whole = plan.whole.expect("whole");
    assert!(whole.ready, "the finer image is shown");
    assert_eq!(whole.request.preview_scale, quarter());
}

#[test]
fn fit_view_uses_only_the_whole_image() {
    let edited = plan_for(
        &ViewMode::Fit,
        &request(1.),
        reduced(0., 1),
        patch(0., 2, REGION),
    );
    assert!(edited.patch.is_none() && edited.full.is_none());
    assert!(needed(&edited.whole).is_some());
    let current = plan_for(
        &ViewMode::Fit,
        &request(0.),
        reduced(0., 1),
        patch(0., 2, REGION),
    );
    assert!(current.whole.is_some_and(|whole| whole.ready));
}

#[test]
fn a_native_tile_scale_needs_no_separate_full_size_image() {
    let mut request = request(0.);
    request.preview_scale = Scale::NATIVE;
    let plan = plan_for(&ZOOMED, &request, full(0., 1), patch(0., 2, REGION));
    assert!(plan.patch.is_none() && plan.full.is_none());
    assert!(plan.whole.is_some_and(|whole| whole.ready));
}

#[test]
fn the_loupe_uses_only_its_patch_and_the_tile_s_own_image() {
    let lens = ViewMode::Loupe {
        regions: Some(Regions {
            visible: VISIBLE,
            patch: REGION,
        }),
    };
    let plan = plan_for(&lens, &request(0.), reduced(0., 1), None);
    assert_eq!(
        needed(&plan.patch).expect("patch").settings.preview_region,
        Some(REGION)
    );
    assert!(plan.whole.is_some_and(|whole| whole.ready));
    assert!(plan.full.is_none(), "the loupe never requests full size");
}

#[test]
fn a_loupe_over_a_current_full_size_image_uses_it_alone() {
    let lens = ViewMode::Loupe {
        regions: Some(Regions {
            visible: VISIBLE,
            patch: REGION,
        }),
    };
    let plan = plan_for(&lens, &request(0.), full(0., 1), None);
    assert!(plan.patch.is_none() && plan.full.is_none());
    let whole = plan.whole.expect("full size in use");
    assert!(whole.ready);
    assert_eq!(whole.request.preview_scale, Scale::NATIVE);
}

#[test]
fn a_loupe_without_any_image_places_itself_with_a_whole_photo_patch() {
    let placing = ViewMode::Loupe { regions: None };
    let plan = plan_for(&placing, &request(0.), None, None);
    let patch = needed(&plan.patch).expect("placement patch");
    assert!(patch.detail);
    assert_eq!(patch.settings.preview_region, None);
    assert_eq!(patch.preview_scale, quarter());
    assert!(
        plan.whole.is_some(),
        "every viewport photo renders its own whole image"
    );
}

#[test]
fn the_latest_patch_stays_on_top_until_the_image_behind_has_newer_edits() {
    let request = request(1.);
    let current = |image: &Image| image.matches(&request);
    let older_patch = patch(0.5, 3, REGION).unwrap();
    assert!(
        patch_on_top(&older_patch, reduced(0., 1).as_ref(), current),
        "scrubbing keeps the latest patch"
    );
    assert!(
        patch_on_top(&older_patch, reduced(0.5, 4).as_ref(), current),
        "a whole image planned alongside, issued after the patch, stays behind"
    );
    assert!(
        !patch_on_top(&older_patch, reduced(0.75, 6).as_ref(), current),
        "a whole image from a later plan has newer edits"
    );
    assert!(
        !patch_on_top(&older_patch, reduced(1., 4).as_ref(), current),
        "the image behind already has the current edits"
    );
    assert!(patch_on_top(
        &patch(1., 4, REGION).unwrap(),
        reduced(1., 5).as_ref(),
        current
    ));
}

#[test]
fn a_moving_view_plans_its_previous_region_until_it_rests() {
    let regions = |visible, patch| Regions { visible, patch };
    let initial = regions(VISIBLE, REGION);
    let start = std::time::Instant::now();
    let after = |ms| start + std::time::Duration::from_millis(ms);
    let mut motion = Motion::new(initial, start);
    let moved = regions([140, 100, 540, 400], [120, 80, 560, 420]);
    let (settled, wait) = motion.settle(moved, after(10));
    assert_eq!(settled, initial, "still moving");
    assert_eq!(wait, Some(SETTLE));
    let further = regions([180, 100, 580, 400], [160, 80, 600, 420]);
    assert_eq!(motion.settle(further, after(100)).0, initial);
    assert_eq!(
        motion.settle(further, after(219)).0,
        initial,
        "the rest restarts with each move"
    );
    assert_eq!(motion.settle(further, after(220)), (further, None));
}

#[test]
fn the_framing_plane_uses_only_its_whole_image() {
    let mut request = request(0.);
    request.framing_preview = true;
    let plan = plan_for(&ViewMode::Framing, &request, None, None);
    let whole = needed(&plan.whole).expect("whole");
    assert!(whole.framing_preview);
    assert!(plan.patch.is_none() && plan.full.is_none());
}
