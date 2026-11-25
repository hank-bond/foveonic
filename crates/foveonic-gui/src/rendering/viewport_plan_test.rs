//! Checks how every viewport photo is planned, from inputs alone and without a GPU.

use super::*;
use crate::editor::Rendition;
use foveonic::render::{ProcessingQuality, Settings};

struct Image {
    exposure: f32,
    id: u64,
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
        RequestId(self.id)
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
fn quarter() -> Scale {
    Scale::new(4).unwrap()
}
fn whole(exposure: f32) -> Image {
    Image {
        exposure,
        id: 1,
        scale: quarter(),
        region: None,
    }
}
fn request(path: &str, exposure: f32) -> RenderRequest {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.exposure = exposure;
    RenderRequest {
        detail: false,
        framing_preview: false,
        calibration: None,
        rendition: Rendition::Current,
        id: RequestId::default(),
        path: path.into(),
        settings,
        preview_scale: quarter(),
        processing_quality: ProcessingQuality::Speed,
        output: "final".into(),
        comparison: false,
    }
}
/// A photo shown fit with its settings loaded and no stored images.
fn photo<'a>(path: &str, exposure: f32) -> PhotoInputs<'a, Image> {
    PhotoInputs {
        path: path.into(),
        request: Some(request(path, exposure)),
        images: PhotoImages {
            whole: None,
            patch: None,
        },
        regional: None,
        window: None,
        placement: Placement::Fit,
    }
}
/// Visible pixels covering `[100, 100, 500, 400)` of the whole image.
fn visible() -> crate::viewport::PixelRegion {
    crate::viewport::PixelRegion {
        top_left: [100, 100],
        bottom_right: [499, 399],
        extent: [400, 300],
    }
}
const WINDOW: [u32; 4] = [0, 0, 2000, 1500];

fn run(inputs: Vec<PhotoInputs<'_, Image>>, selected: Option<&str>) -> Publication {
    run_with(inputs, selected, HashMap::new(), |_| false)
}
fn run_with(
    inputs: Vec<PhotoInputs<'_, Image>>,
    selected: Option<&str>,
    previous: HashMap<PathBuf, PhotoView>,
    failed: impl Fn(&RenderRequest) -> bool,
) -> Publication {
    run_editing(inputs, selected, &[], previous, failed)
}
fn run_editing(
    inputs: Vec<PhotoInputs<'_, Image>>,
    selected: Option<&str>,
    editing: &[PathBuf],
    previous: HashMap<PathBuf, PhotoView>,
    failed: impl Fn(&RenderRequest) -> bool,
) -> Publication {
    let selected = selected.map(PathBuf::from);
    plan_photos(
        inputs,
        previous,
        &mut BTreeMap::new(),
        failed,
        Planning {
            selected: selected.as_deref(),
            editing,
            last_request_id: RequestId(100),
            background_completion: false,
            now: Instant::now(),
        },
    )
}
fn paths(paths: &[PathBuf]) -> Vec<&str> {
    paths.iter().map(|path| path.to_str().unwrap()).collect()
}

#[test]
fn a_photo_whose_settings_are_loading_waits_and_keeps_its_last_view() {
    let mut loading = photo("a.x3f", 0.);
    loading.request = None;
    let previous = HashMap::from([(
        PathBuf::from("a.x3f"),
        PhotoView {
            request: request("a.x3f", 0.),
            mode: ViewMode::Fit,
        },
    )]);
    let publication = run_with(vec![loading], Some("a.x3f"), previous, |_| false);
    assert!(publication.planned.is_empty());
    assert_eq!(paths(&publication.photos.loading), ["a.x3f"]);
    assert!(publication.photos.planned.is_empty());
    assert_eq!(publication.unplanned[0].1, "loading");
    assert!(
        publication.views.contains_key(Path::new("a.x3f")),
        "it keeps drawing from its last view"
    );
    assert_eq!(publication.last_request_id, RequestId(100));
}

#[test]
fn a_request_that_failed_to_render_is_not_rendered_again() {
    let publication = run_with(vec![photo("a.x3f", 0.)], None, HashMap::new(), |_| true);
    assert_eq!(publication.planned.len(), 1);
    assert_eq!(publication.planned[0].state, ImageState::Ready);
}

#[test]
fn each_image_gets_its_own_id_and_the_selected_photos_first_image_is_tracked() {
    let publication = run(vec![photo("a.x3f", 0.), photo("b.x3f", 0.)], Some("b.x3f"));
    let ids: Vec<_> = publication
        .planned
        .iter()
        .map(|planned| planned.request.id)
        .collect();
    assert_eq!(ids, [RequestId(101), RequestId(102)]);
    assert_eq!(publication.last_request_id, RequestId(102));
    assert_eq!(publication.selected_request, Some(RequestId(102)));
    let tracked: Vec<_> = publication
        .planned
        .iter()
        .filter(|planned| planned.tracked.is_some())
        .map(|planned| planned.request.path.clone())
        .collect();
    assert_eq!(paths(&tracked), ["b.x3f"]);
}

#[test]
fn a_zoomed_photo_plans_its_patch_before_its_whole_image() {
    let stale = whole(0.);
    let mut zoomed = photo("a.x3f", 1.);
    zoomed.images.whole = Some(&stale);
    zoomed.window = Some(WINDOW);
    zoomed.regional = Some(true);
    zoomed.placement = Placement::Zoomed {
        visible: Some(visible()),
        fit_scale: quarter(),
    };
    let publication = run(vec![zoomed], Some("a.x3f"));
    let [patch, whole] = &publication.planned[..] else {
        panic!("expected a patch and a whole image");
    };
    assert!(patch.request.detail && patch.state == ImageState::Render);
    assert_eq!(
        patch.request.settings.preview_region,
        Some(patch_region([100, 100, 500, 400], WINDOW))
    );
    assert!(!whole.request.detail);
    assert_eq!(
        whole.state,
        ImageState::Render,
        "a whole image follows the patch in order instead of waiting for it"
    );
    assert_eq!(patch.tracked, Some(0));
}

#[test]
fn a_zoomed_photo_without_regional_support_is_planned_as_a_native_fit() {
    let stored = whole(0.);
    let mut zoomed = photo("a.x3f", 0.);
    zoomed.images.whole = Some(&stored);
    zoomed.window = Some(WINDOW);
    zoomed.regional = Some(false);
    zoomed.placement = Placement::Zoomed {
        visible: Some(visible()),
        fit_scale: quarter(),
    };
    let publication = run(vec![zoomed], None);
    assert!(matches!(
        publication.views[Path::new("a.x3f")].mode,
        ViewMode::Fit
    ));
    assert_eq!(publication.planned[0].request.preview_scale, Scale::NATIVE);
    assert!(!publication.planned[0].request.detail);
}

#[test]
fn a_zoomed_photo_not_drawn_this_frame_needs_only_its_whole_image() {
    let stored = whole(0.);
    let mut zoomed = photo("a.x3f", 0.);
    zoomed.images.whole = Some(&stored);
    zoomed.window = Some(WINDOW);
    zoomed.regional = Some(true);
    zoomed.placement = Placement::Zoomed {
        visible: None,
        fit_scale: quarter(),
    };
    let publication = run(vec![zoomed], None);
    assert!(publication.unplanned.is_empty());
    assert!(matches!(
        publication.views[Path::new("a.x3f")].mode,
        ViewMode::Fit
    ));
    assert!(publication
        .planned
        .iter()
        .all(|planned| !planned.request.detail));
}

#[test]
fn the_loupe_on_a_photo_with_nothing_to_place_its_lens_on_renders_a_placement_patch() {
    let mut loupe = photo("a.x3f", 0.);
    loupe.placement = Placement::Loupe { regions: None };
    let publication = run(vec![loupe], None);
    assert!(
        publication.unplanned.is_empty(),
        "it does not wait for a lens"
    );
    let placement = &publication.planned[0];
    assert!(placement.request.detail && placement.state == ImageState::Render);
    assert_eq!(placement.request.settings.preview_region, None);
}

#[test]
fn a_placed_loupe_plans_a_patch_for_its_regions() {
    let stored = whole(0.);
    let mut loupe = photo("a.x3f", 0.);
    loupe.images.whole = Some(&stored);
    loupe.regional = Some(true);
    let regions = Regions {
        visible: [100, 100, 200, 200],
        patch: [80, 80, 220, 220],
    };
    loupe.placement = Placement::Loupe {
        regions: Some(regions),
    };
    let publication = run(vec![loupe], None);
    let patch = &publication.planned[0];
    assert!(patch.request.detail);
    assert_eq!(patch.request.settings.preview_region, Some(regions.patch));
}

#[test]
fn before_while_zoomed_renders_the_look_and_keeps_the_current_image_as_a_stand_in() {
    let current = whole(0.);
    let mut before = photo("a.x3f", 0.);
    before.request.as_mut().unwrap().rendition = Rendition::Camera;
    before.request.as_mut().unwrap().settings.exposure = 0.3;
    // The stored Current image stands in for the Camera look until it renders.
    before.images.whole = Some(&current);
    before.window = Some(WINDOW);
    before.regional = Some(true);
    before.placement = Placement::Zoomed {
        visible: Some(visible()),
        fit_scale: quarter(),
    };
    let publication = run(vec![before], Some("a.x3f"));
    assert!(publication
        .planned
        .iter()
        .all(|planned| planned.request.rendition == Rendition::Camera));
    assert!(
        publication
            .planned
            .iter()
            .any(|planned| planned.state == ImageState::Render),
        "the stand-in does not match the Camera look, so the look renders"
    );
}

#[test]
fn the_progress_pass_follows_every_edited_photos_first_image_selected_first() {
    let editing = [PathBuf::from("b.x3f"), PathBuf::from("c.x3f")];
    let publication = run_editing(
        vec![photo("a.x3f", 0.), photo("b.x3f", 0.), photo("c.x3f", 0.)],
        Some("c.x3f"),
        &editing,
        HashMap::new(),
        |_| false,
    );
    let tracked: Vec<_> = publication
        .planned
        .iter()
        .filter(|planned| planned.tracked.is_some())
        .map(|planned| planned.request.path.clone())
        .collect();
    assert_eq!(
        paths(&tracked),
        ["b.x3f", "c.x3f"],
        "a.x3f is shown but not edited"
    );
    assert_eq!(publication.selected_request, Some(RequestId(103)));
    let selected = publication
        .planned
        .iter()
        .find(|planned| planned.request.path == Path::new("c.x3f"))
        .unwrap();
    assert_eq!(
        selected.tracked,
        Some(0),
        "the selected photo leads the pass"
    );
}

#[test]
fn only_the_editing_selection_is_eviction_protected() {
    let publication = run_editing(
        vec![photo("a.x3f", 0.), photo("b.x3f", 0.), photo("c.x3f", 0.)],
        Some("a.x3f"),
        &[PathBuf::from("b.x3f")],
        HashMap::new(),
        |_| false,
    );
    assert_eq!(paths(&publication.photos.eviction_protected), ["b.x3f"]);
}
