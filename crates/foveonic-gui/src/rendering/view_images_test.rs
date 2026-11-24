//! Runs the app's planning loop for every view kind and the cases native scenarios found, without a GPU.

use super::*;
use crate::{
    rendering::{
        matching,
        smart::patch_region,
        view_plan::{answers, ImageState, Regions, Rendered, ViewMode, SETTLE},
    },
    viewport::PixelRegion,
};

/// A delivered render of the request it answers, as the worker would return it.
struct Image {
    request: RenderRequest,
    origin: SourceOrigin,
}
impl Rendered for Image {
    fn matches(&self, request: &RenderRequest) -> bool {
        answers(&self.request, self.request.processing_quality, request)
    }
    fn same_edits(&self, other: &Self) -> bool {
        matching::same_authored_edits(&self.request.settings, &other.request.settings)
    }
    fn request_id(&self) -> RequestId {
        self.request.id
    }
    fn scale(&self) -> Scale {
        self.request.preview_scale
    }
    fn region(&self) -> Option<[u32; 4]> {
        self.request.settings.preview_region
    }
    fn requested_region(&self) -> Option<[u32; 4]> {
        self.request.settings.preview_region
    }
}
impl Stored for Image {
    fn key(&self) -> ImageKey {
        ImageKey {
            path: self.request.path.clone(),
            look: self.request.rendition,
            output: self.request.output.clone(),
            comparison: self.request.comparison,
            framing: self.request.framing_preview,
        }
    }
    fn patch(&self) -> bool {
        self.request.detail
    }
    fn origin(&self) -> &SourceOrigin {
        &self.origin
    }
    fn window(&self) -> [u32; 4] {
        WINDOW
    }
    fn supports_regions(&self, _: &Settings) -> bool {
        true
    }
}

const WINDOW: [u32; 4] = [0, 0, 2000, 1500];
const INSPECTION: &str = "linear.sharpen";

fn quarter() -> Scale {
    Scale::new(4).unwrap()
}
fn settings(exposure: f32) -> Settings {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.exposure = exposure;
    settings
}
/// A photo with its settings loaded, shown with its Current look and final output.
fn looks(path: &str, exposure: f32) -> PhotoLooks {
    PhotoLooks {
        path: path.into(),
        look: Rendition::Current,
        look_settings: None,
        current_settings: Some(settings(exposure)),
        output: "final".into(),
        selected: false,
        offered: Vec::new(),
        compares: false,
        framing: false,
    }
}
fn fit(looks: PhotoLooks) -> PhotoFacts {
    PhotoFacts {
        looks,
        origin: None,
        preview_scale: Some(quarter()),
        placement: Placement::Fit,
    }
}
/// Zoomed to native detail with `[left, top]` of the whole image at the tile's top left.
fn zoomed(looks: PhotoLooks, left: u32, top: u32) -> PhotoFacts {
    PhotoFacts {
        placement: Placement::Zoomed {
            visible: Some(visible(left, top)),
            fit_scale: quarter(),
        },
        preview_scale: Some(Scale::NATIVE),
        ..fit(looks)
    }
}
/// A 400 × 300 visible region at `[left, top]`.
fn visible(left: u32, top: u32) -> PixelRegion {
    PixelRegion {
        top_left: [left, top],
        bottom_right: [left + 399, top + 299],
        extent: [400, 300],
    }
}
fn regions(left: u32, top: u32) -> Regions {
    let visible = visible(left, top).native_bounds(WINDOW);
    Regions {
        visible,
        patch: patch_region(visible, WINDOW),
    }
}

/// The planning loop the app runs every frame, with the worker replaced by
/// the publication it would receive and a render of whatever it asks for.
struct Loop {
    view_images: ViewImages<Image>,
    selected: Option<PathBuf>,
    editing: Vec<PathBuf>,
    history: Vec<PathBuf>,
    policy: ProcessingQuality,
    now: Instant,
    /// The last publication the worker accepted, which it keeps until one differs.
    photos: Option<Photos>,
    published: Vec<Planned>,
}
impl Loop {
    fn new() -> Self {
        Self {
            view_images: ViewImages::default(),
            selected: None,
            editing: Vec::new(),
            history: Vec::new(),
            policy: ProcessingQuality::Speed,
            now: Instant::now(),
            photos: None,
            published: Vec::new(),
        }
    }

    /// Plan one frame. The worker, like the runtime, takes a publication only
    /// when it differs from the last one it took.
    fn plan(&mut self, photos: Vec<PhotoFacts>) -> Outcome {
        let mut sent = None;
        let (last_photos, last_planned) = (&self.photos, &self.published);
        let outcome = self.view_images.plan(
            photos,
            Round {
                selected: self.selected.as_deref(),
                editing: &self.editing,
                history: self.history.clone(),
                policy: self.policy,
                calibration: None,
                background_completion: false,
                now: self.now,
            },
            |photos, planned| {
                let unchanged = last_photos.as_ref() == Some(&photos)
                    && last_planned.len() == planned.len()
                    && last_planned.iter().zip(&planned).all(|(old, new)| {
                        old.speculative == new.speculative
                            && old.state == new.state
                            && matching::same_delivery_request(&old.request, &new.request)
                    });
                if !unchanged {
                    sent = Some((photos, planned));
                }
                !unchanged
            },
        );
        if let Some((photos, planned)) = sent {
            self.photos = Some(photos);
            self.published = planned;
        }
        outcome
    }

    /// Forget the last publication, as clearing the worker does.
    fn clear_worker(&mut self) {
        self.photos = None;
        self.published.clear();
    }

    /// The images the last publication asks the worker to render.
    fn to_render(&self) -> Vec<&RenderRequest> {
        self.published
            .iter()
            .filter(|planned| planned.state == ImageState::Render)
            .map(|planned| &planned.request)
            .collect()
    }

    /// Deliver the images the last publication asks for that `pick` selects.
    fn render_where(&mut self, pick: impl Fn(&RenderRequest) -> bool) {
        let requests: Vec<_> = self
            .to_render()
            .into_iter()
            .filter(|request| pick(request))
            .cloned()
            .collect();
        for request in requests {
            assert!(self.view_images.accept(Image {
                request,
                origin: SourceOrigin::default(),
            }));
        }
    }

    fn render(&mut self) {
        self.render_where(|_| true);
    }

    /// The ids of the whole image and patch drawn for what the photo shows.
    fn drawn(&self, looks: &PhotoLooks) -> [Option<RequestId>; 2] {
        self.view_images
            .layers(&looks.key(), None)
            .map(|image| image.map(|image| image.request.id))
    }
}

#[test]
fn a_fit_photo_renders_once_then_plans_nothing_new() {
    let mut app = Loop::new();
    app.plan(vec![fit(looks("a.x3f", 0.))]);
    let [whole] = &app.to_render()[..] else {
        panic!("expected one whole image");
    };
    assert!(!whole.detail && whole.preview_scale == quarter());
    let id = whole.id;
    app.render();
    app.plan(vec![fit(looks("a.x3f", 0.))]);
    assert!(app.to_render().is_empty());
    assert_eq!(app.drawn(&looks("a.x3f", 0.)), [Some(id), None]);
}

#[test]
fn a_zoomed_photo_draws_its_patch_once_both_layers_arrive() {
    let mut app = Loop::new();
    app.plan(vec![fit(looks("a.x3f", 0.))]);
    app.render();
    // An edit while zoomed outdates both layers.
    app.plan(vec![zoomed(looks("a.x3f", 1.), 100, 100)]);
    let rendering = app.to_render();
    assert_eq!(
        rendering.len(),
        2,
        "the patch and the whole image behind it"
    );
    assert!(rendering[0].detail, "the patch comes first");
    assert_eq!(
        rendering[0].settings.preview_region,
        Some(regions(100, 100).patch)
    );
    let stale = app.drawn(&looks("a.x3f", 1.));
    assert!(
        stale[0].is_some() && stale[1].is_none(),
        "the old image stays while both render"
    );
    app.render_where(|request| request.detail);
    let patched = app.drawn(&looks("a.x3f", 1.));
    assert_eq!(patched[0], stale[0], "the old image stays behind the patch");
    assert!(patched[1].is_some(), "the current patch is drawn on top");
    app.render();
    let drawn = app.drawn(&looks("a.x3f", 1.));
    assert_ne!(drawn[0], stale[0]);
    assert_eq!(drawn[1], patched[1]);
    app.plan(vec![zoomed(looks("a.x3f", 1.), 100, 100)]);
    assert!(app.to_render().is_empty());
}

#[test]
fn a_moving_view_plans_only_the_region_it_rests_on() {
    let mut app = Loop::new();
    app.plan(vec![fit(looks("a.x3f", 0.))]);
    app.render();
    app.plan(vec![zoomed(looks("a.x3f", 0.), 100, 100)]);
    app.render();
    let step = SETTLE / 4;
    for left in [200, 300, 400] {
        app.now += step;
        let outcome = app.plan(vec![zoomed(looks("a.x3f", 0.), left, 100)]);
        assert!(app.to_render().is_empty(), "nothing renders while moving");
        assert!(app.view_images.waiting() && outcome.repaint_after.is_some());
    }
    app.now += SETTLE;
    app.plan(vec![zoomed(looks("a.x3f", 0.), 400, 100)]);
    let [patch] = &app.to_render()[..] else {
        panic!("expected one patch");
    };
    assert_eq!(patch.settings.preview_region, Some(regions(400, 100).patch));
    assert!(!app.view_images.waiting());
}

#[test]
fn the_framing_plane_renders_uncropped_and_leaving_it_returns_to_the_crop() {
    let mut cropped = looks("a.x3f", 0.);
    cropped.current_settings.as_mut().unwrap().crop.bounds = [0.1, 0.1, 0.9, 0.9];
    cropped.selected = true;
    let framing = PhotoLooks {
        framing: true,
        ..cropped.clone()
    };
    let mut app = Loop::new();
    app.plan(vec![fit(cropped.clone())]);
    app.render();
    let [photograph, _] = app.drawn(&cropped);

    app.plan(vec![fit(framing.clone())]);
    let [plane] = &app.to_render()[..] else {
        panic!("expected the plane's whole image");
    };
    assert!(plane.framing_preview && !plane.detail);
    assert_eq!(
        app.drawn(&framing)[0],
        photograph,
        "the photograph stands in until the plane arrives"
    );
    app.render();
    assert_ne!(app.drawn(&framing)[0], photograph);

    app.plan(vec![fit(cropped.clone())]);
    assert!(app.to_render().is_empty(), "the photograph was kept");
    assert_eq!(app.drawn(&cropped)[0], photograph);
}

#[test]
fn the_loupe_on_an_unedited_photo_places_its_lens_then_renders_its_patch() {
    let mut app = Loop::new();
    app.selected = Some("b.x3f".into());
    app.editing = vec!["b.x3f".into()];
    let loupe = |regions| PhotoFacts {
        placement: Placement::Loupe { regions },
        ..fit(looks("a.x3f", 0.))
    };
    app.plan(vec![loupe(None), fit(looks("b.x3f", 0.))]);
    let placement = app
        .to_render()
        .into_iter()
        .find(|request| request.path == Path::new("a.x3f") && request.detail)
        .expect("a whole-photo patch places the lens");
    assert_eq!(placement.settings.preview_region, None);
    assert_eq!(placement.preview_scale, quarter());
    app.render();

    let placed = regions(100, 100);
    app.plan(vec![loupe(Some(placed)), fit(looks("b.x3f", 0.))]);
    let [patch] = &app.to_render()[..] else {
        panic!("expected the lens patch");
    };
    assert_eq!(patch.settings.preview_region, Some(placed.patch));
    let id = patch.id;
    app.render();
    assert_eq!(app.drawn(&looks("a.x3f", 0.))[1], Some(id));
}

#[test]
fn inspection_applies_where_the_camera_offers_it_and_other_photos_show_final() {
    let inspect = |path: &str, offered: &[&'static str], selected: bool| PhotoLooks {
        output: INSPECTION.into(),
        offered: offered.to_vec(),
        selected,
        ..looks(path, 0.)
    };
    let photos = || {
        vec![
            fit(inspect("offers.x3f", &[INSPECTION], false)),
            fit(inspect("other.x3f", &["final"], false)),
            fit(inspect("selected.x3f", &[], true)),
        ]
    };
    let mut app = Loop::new();
    app.plan(photos());
    let outputs: Vec<_> = app
        .to_render()
        .iter()
        .map(|request| (request.path.to_str().unwrap(), request.output.as_str()))
        .collect();
    assert_eq!(
        outputs,
        [
            ("offers.x3f", INSPECTION),
            ("other.x3f", "final"),
            ("selected.x3f", INSPECTION),
        ]
    );
    assert!(app
        .to_render()
        .iter()
        .filter(|request| request.output == INSPECTION)
        .all(|request| request.processing_quality == ProcessingQuality::Quality));
}

#[test]
fn the_loupe_keeps_its_view_after_a_crop_change_until_a_placeable_image_arrives() {
    let mut app = Loop::new();
    let placed = regions(100, 100);
    let loupe = |looks, regions| PhotoFacts {
        placement: Placement::Loupe { regions },
        ..fit(looks)
    };
    app.plan(vec![fit(looks("a.x3f", 0.))]);
    app.render();
    app.plan(vec![loupe(looks("a.x3f", 0.), Some(placed))]);
    app.render();
    let before = app.drawn(&looks("a.x3f", 0.));

    // No layer has the new crop, so the lens cannot be placed.
    let mut recropped = looks("a.x3f", 0.);
    recropped.current_settings.as_mut().unwrap().crop.bounds = [0.2, 0.2, 0.8, 0.8];
    app.plan(vec![loupe(recropped.clone(), None)]);
    let view = app
        .view_images
        .view(Path::new("a.x3f"))
        .expect("its view stays");
    assert_eq!(
        view.request.settings.crop.bounds,
        [0.2, 0.2, 0.8, 0.8],
        "the lens is placed from the new crop"
    );
    assert!(matches!(view.mode, ViewMode::Loupe { regions: None }));
    assert_eq!(
        app.drawn(&recropped)[0],
        before[0],
        "the old image stays drawn"
    );
    let placement = app
        .to_render()
        .into_iter()
        .find(|request| request.detail)
        .expect("a whole-photo patch places the lens");
    assert_eq!(placement.settings.preview_region, None);
    app.render();

    app.plan(vec![loupe(recropped.clone(), Some(placed))]);
    assert!(app
        .to_render()
        .iter()
        .any(|request| request.settings.preview_region == Some(placed.patch)));
}

#[test]
fn a_before_toggle_reuses_images_and_turning_it_off_is_immediate() {
    let photograph = PhotoLooks {
        selected: true,
        ..looks("a.x3f", 1.)
    };
    let current = PhotoLooks {
        output: INSPECTION.into(),
        ..photograph.clone()
    };
    let before = PhotoLooks {
        look: Rendition::Camera,
        look_settings: Some(settings(0.)),
        ..current.clone()
    };
    let mut app = Loop::new();
    app.selected = Some("a.x3f".into());
    app.editing = vec!["a.x3f".into()];
    app.plan(vec![fit(photograph.clone())]);
    app.render();
    let shown = app.drawn(&photograph)[0];
    app.plan(vec![fit(current.clone())]);
    app.render();
    let inspection = app.drawn(&current)[0];

    app.plan(vec![fit(before.clone())]);
    let [camera] = &app.to_render()[..] else {
        panic!("expected the Camera look");
    };
    assert_eq!(camera.rendition, Rendition::Camera);
    assert_eq!(camera.output, "final", "Before shows the photograph");
    assert_eq!(
        app.drawn(&before)[0],
        shown,
        "the Current photograph stands in"
    );
    app.render();
    assert_ne!(app.drawn(&before)[0], shown);

    app.plan(vec![fit(current.clone())]);
    assert!(
        app.to_render().is_empty(),
        "the inspection image was kept while Before was on"
    );
    assert_eq!(app.drawn(&current)[0], inspection);
}

#[test]
fn returning_from_inspection_shows_the_kept_photograph_without_rendering() {
    let photograph = PhotoLooks {
        selected: true,
        ..looks("a.x3f", 0.)
    };
    let inspecting = PhotoLooks {
        output: INSPECTION.into(),
        ..photograph.clone()
    };
    let mut app = Loop::new();
    app.plan(vec![fit(photograph.clone())]);
    app.render();
    let shown = app.drawn(&photograph)[0];

    app.plan(vec![fit(inspecting.clone())]);
    assert_eq!(
        app.drawn(&inspecting)[0],
        shown,
        "the photograph stands in for the output"
    );
    app.render();
    assert_ne!(app.drawn(&inspecting)[0], shown);

    app.plan(vec![fit(photograph.clone())]);
    assert!(app.to_render().is_empty());
    assert_eq!(app.drawn(&photograph)[0], shown);
}

#[test]
fn a_cold_photo_under_before_renders_current_until_its_metadata_arrives() {
    let cold = PhotoLooks {
        look: Rendition::Camera,
        ..looks("a.x3f", 1.)
    };
    let mut app = Loop::new();
    app.editing = vec!["a.x3f".into()];
    app.plan(vec![fit(cold)]);
    let [first] = &app.to_render()[..] else {
        panic!("expected one image");
    };
    assert_eq!(first.rendition, Rendition::Current);
    app.render();

    let resolved = PhotoLooks {
        look: Rendition::Camera,
        look_settings: Some(settings(0.)),
        ..looks("a.x3f", 1.)
    };
    app.plan(vec![fit(resolved.clone())]);
    let [camera] = &app.to_render()[..] else {
        panic!("expected the Camera look");
    };
    assert_eq!(camera.rendition, Rendition::Camera);
    assert!(app.drawn(&resolved)[0].is_some(), "Current stands in");
}

#[test]
fn a_photo_beyond_the_editing_limit_stays_planned_while_on_screen() {
    let mut app = Loop::new();
    app.plan(vec![fit(looks("a.x3f", 0.))]);
    app.render();
    let shown = app.drawn(&looks("a.x3f", 0.));
    // The editing limit released its settings while it is still drawn.
    let released = PhotoLooks {
        current_settings: None,
        ..looks("a.x3f", 0.)
    };
    app.plan(vec![fit(released.clone())]);
    assert!(app.published.is_empty());
    let photos = app.photos.as_ref().unwrap();
    assert_eq!(photos.loading, [PathBuf::from("a.x3f")]);
    assert!(app.view_images.waiting());
    assert!(app.view_images.view(Path::new("a.x3f")).is_some());
    assert_eq!(app.drawn(&released), shown, "its images stay");
}

#[test]
fn a_photo_leaving_the_view_keeps_only_its_current_image_as_history() {
    let inspecting = PhotoLooks {
        output: INSPECTION.into(),
        offered: vec![INSPECTION],
        ..looks("a.x3f", 0.)
    };
    let mut app = Loop::new();
    app.plan(vec![fit(looks("a.x3f", 0.))]);
    app.render();
    app.plan(vec![fit(inspecting.clone())]);
    app.render();
    app.plan(vec![fit(looks("b.x3f", 0.)), fit(looks("c.x3f", 0.))]);
    app.render();
    assert!(
        app.view_images.current_image(Path::new("a.x3f")).is_none(),
        "a photo outside history keeps nothing"
    );

    app.plan(vec![fit(looks("a.x3f", 0.))]);
    app.render();
    app.plan(vec![fit(inspecting.clone())]);
    app.render();
    app.history = vec!["a.x3f".into()];
    app.plan(vec![fit(looks("b.x3f", 0.))]);
    let photos = app.photos.as_ref().unwrap();
    assert_eq!(photos.history, [PathBuf::from("a.x3f")]);
    assert!(app
        .published
        .iter()
        .all(|planned| planned.request.path != Path::new("a.x3f")));
    assert!(app.view_images.current_image(Path::new("a.x3f")).is_some());
    assert!(
        app.view_images.images().all(
            |image| image.request.path != Path::new("a.x3f") || image.request.output == "final"
        ),
        "its inspection image is dropped"
    );
}

#[test]
fn a_stored_current_image_needs_no_new_work() {
    let mut app = Loop::new();
    app.history = vec!["a.x3f".into()];
    app.plan(vec![fit(looks("a.x3f", 0.))]);
    app.render();
    let shown = app.drawn(&looks("a.x3f", 0.));
    app.plan(vec![fit(looks("b.x3f", 0.))]);
    app.render();
    // Back in the viewport, as after selecting it again.
    app.plan(vec![fit(looks("a.x3f", 0.)), fit(looks("b.x3f", 0.))]);
    assert!(app.to_render().is_empty());
    assert!(app
        .published
        .iter()
        .all(|planned| planned.state == ImageState::Ready));
    assert_eq!(app.drawn(&looks("a.x3f", 0.)), shown);
}

/// Displayed work that is invalidated and rendered again keeps every photo's
/// previous layers drawn until new ones arrive, so no tile goes blank. A quality
/// change stands in for calibration, defaults and edits, which stop requests
/// matching through the same rule (`answers`).
#[test]
fn invalidated_photos_keep_their_previous_images_until_new_ones_arrive() {
    let mut app = Loop::new();
    let photos = || {
        vec![
            fit(looks("fit.x3f", 0.)),
            zoomed(looks("zoomed.x3f", 0.), 100, 100),
        ]
    };
    app.plan(vec![
        fit(looks("fit.x3f", 0.)),
        fit(looks("zoomed.x3f", 0.)),
    ]);
    app.render();
    app.plan(photos());
    app.render();
    let fit_shown = app.drawn(&looks("fit.x3f", 0.));
    let zoomed_shown = app.drawn(&looks("zoomed.x3f", 0.));
    assert!(fit_shown[0].is_some() && zoomed_shown.iter().all(Option::is_some));

    app.policy = ProcessingQuality::Quality;
    app.plan(photos());
    assert_eq!(
        app.to_render().len(),
        3,
        "every displayed image renders again"
    );
    assert_eq!(app.drawn(&looks("fit.x3f", 0.)), fit_shown);
    assert_eq!(app.drawn(&looks("zoomed.x3f", 0.)), zoomed_shown);

    app.render_where(|request| request.detail);
    let zoomed_now = app.drawn(&looks("zoomed.x3f", 0.));
    assert_eq!(zoomed_now[0], zoomed_shown[0]);
    assert_ne!(zoomed_now[1], zoomed_shown[1]);
    assert_eq!(app.drawn(&looks("fit.x3f", 0.)), fit_shown);
    app.render();
    let fit_now = app.drawn(&looks("fit.x3f", 0.))[0];
    assert!(fit_now.is_some() && fit_now != fit_shown[0]);
    app.plan(photos());
    assert!(app.to_render().is_empty());
}

#[test]
fn ids_advance_only_when_a_changed_publication_is_sent() {
    let mut app = Loop::new();
    app.selected = Some("a.x3f".into());
    app.plan(vec![fit(looks("a.x3f", 0.))]);
    let issued = app.view_images.last_request_id();
    assert_ne!(issued, RequestId::default());
    let outcome = app.plan(vec![fit(looks("a.x3f", 0.))]);
    assert_eq!(
        app.view_images.last_request_id(),
        issued,
        "an unchanged plan issues nothing"
    );
    assert_eq!(outcome.selected_request, None);
    let outcome = app.plan(vec![fit(looks("a.x3f", 1.))]);
    assert!(app.view_images.last_request_id() > issued);
    assert_eq!(
        outcome.selected_request,
        Some(app.view_images.last_request_id())
    );
}

#[test]
fn a_failed_request_is_not_requested_again_until_it_changes() {
    let mut app = Loop::new();
    app.plan(vec![fit(looks("a.x3f", 0.))]);
    let [failed] = &app.to_render()[..] else {
        panic!("expected one whole image");
    };
    let failed = Arc::new((*failed).clone());
    app.view_images.fail(failed);
    app.plan(vec![fit(looks("a.x3f", 0.))]);
    assert!(app.to_render().is_empty(), "it counts as done");
    assert_eq!(app.drawn(&looks("a.x3f", 0.)), [None, None]);
    app.plan(vec![fit(looks("a.x3f", 1.))]);
    assert_eq!(app.to_render().len(), 1, "an edit is a new request");
}

#[test]
fn before_while_zoomed_renders_the_look_and_turning_it_off_reuses_the_current_layers() {
    let current = looks("a.x3f", 1.);
    let before = PhotoLooks {
        look: Rendition::Camera,
        look_settings: Some(settings(0.)),
        ..current.clone()
    };
    let mut app = Loop::new();
    app.editing = vec!["a.x3f".into()];
    app.plan(vec![fit(current.clone())]);
    app.render();
    app.plan(vec![zoomed(current.clone(), 100, 100)]);
    app.render();
    let shown = app.drawn(&current);
    assert!(shown.iter().all(Option::is_some));

    app.plan(vec![zoomed(before.clone(), 100, 100)]);
    let rendering = app.to_render();
    assert_eq!(rendering.len(), 2, "the look's patch and whole image");
    assert!(rendering
        .iter()
        .all(|request| request.rendition == Rendition::Camera));
    assert!(rendering[0].detail);
    assert_eq!(
        app.drawn(&before),
        [shown[0], None],
        "the Current photograph stands in"
    );
    app.render();
    assert!(app.drawn(&before).iter().all(Option::is_some));

    app.plan(vec![zoomed(current.clone(), 100, 100)]);
    assert!(app.to_render().is_empty());
    assert_eq!(app.drawn(&current), shown);
}

#[test]
fn comparison_applies_with_the_inspection_output_where_the_camera_offers_it() {
    let comparing = |path: &str, offered: &[&'static str]| PhotoLooks {
        output: INSPECTION.into(),
        offered: offered.to_vec(),
        compares: true,
        ..looks(path, 0.)
    };
    let mut app = Loop::new();
    app.plan(vec![fit(looks("a.x3f", 0.)), fit(looks("b.x3f", 0.))]);
    app.render();
    app.plan(vec![
        zoomed(comparing("a.x3f", &[INSPECTION]), 100, 100),
        zoomed(comparing("b.x3f", &[]), 100, 100),
    ]);
    let compared = app
        .published
        .iter()
        .find(|planned| planned.request.path == Path::new("a.x3f"))
        .unwrap();
    assert!(compared.request.comparison && compared.request.output == INSPECTION);
    assert_eq!(
        compared.request.processing_quality,
        ProcessingQuality::Quality
    );
    assert!(
        !compared.request.detail && compared.request.preview_scale == Scale::NATIVE,
        "without regional support a zoomed comparison renders at full size"
    );
    let other = app
        .published
        .iter()
        .filter(|planned| planned.request.path == Path::new("b.x3f"))
        .collect::<Vec<_>>();
    assert!(other
        .iter()
        .all(|planned| !planned.request.comparison && planned.request.output == "final"));
    assert!(
        other.iter().any(|planned| planned.request.detail),
        "the photo showing its photograph keeps its patch"
    );
}

/// Clearing empties the collection only when nothing stays displayed, as when
/// an opened folder has no photos. The next photos shown render from scratch.
#[test]
fn after_clearing_the_next_frame_renders_every_displayed_photo() {
    let mut app = Loop::new();
    app.plan(vec![fit(looks("a.x3f", 0.)), fit(looks("b.x3f", 0.))]);
    app.render();
    app.view_images.clear_images();
    app.clear_worker();
    app.plan(vec![fit(looks("a.x3f", 0.)), fit(looks("b.x3f", 0.))]);
    let paths: Vec<_> = app
        .to_render()
        .iter()
        .map(|request| request.path.clone())
        .collect();
    assert_eq!(paths, [PathBuf::from("a.x3f"), PathBuf::from("b.x3f")]);
    assert!(app
        .published
        .iter()
        .all(|planned| planned.state == ImageState::Render));
}
