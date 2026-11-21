//! Checks that rendered images are kept per photo, look and output and never replaced by older ones.

use super::*;
use crate::rendering::request::RequestId;

struct Image {
    key: ImageKey,
    revision: u64,
    patch: bool,
}
impl Stored for Image {
    fn key(&self) -> ImageKey {
        self.key.clone()
    }
    fn patch(&self) -> bool {
        self.patch
    }
}
impl Rendered for Image {
    fn matches(&self, _: &RenderRequest) -> bool {
        true
    }
    fn same_edits(&self, _: &Self) -> bool {
        true
    }
    fn request_id(&self) -> RequestId {
        RequestId(self.revision)
    }
    fn scale(&self) -> foveonic::render::Scale {
        foveonic::render::Scale::NATIVE
    }
    fn region(&self) -> Option<[u32; 4]> {
        None
    }
    fn requested_region(&self) -> Option<[u32; 4]> {
        None
    }
}
fn key(look: Rendition) -> ImageKey {
    ImageKey {
        path: "photo.x3f".into(),
        look,
        output: "final".into(),
        comparison: false,
        framing: false,
    }
}
fn image(look: Rendition, revision: u64, patch: bool) -> Image {
    Image {
        key: key(look),
        revision,
        patch,
    }
}

#[test]
fn each_look_keeps_its_own_whole_image_and_patch() {
    let mut images = RenderedImages::default();
    assert!(images.accept(image(Rendition::Current, 1, false)));
    assert!(images.accept(image(Rendition::Current, 2, true)));
    assert!(images.accept(image(Rendition::Camera, 3, false)));
    let current = images.get(&key(Rendition::Current)).unwrap();
    assert_eq!(current.whole.as_ref().unwrap().revision, 1);
    assert_eq!(current.patch.as_ref().unwrap().revision, 2);
    let camera = images.get(&key(Rendition::Camera)).unwrap();
    assert_eq!(camera.whole.as_ref().unwrap().revision, 3);
    assert!(camera.patch.is_none());
}

#[test]
fn an_older_image_never_replaces_a_newer_one() {
    let mut images = RenderedImages::default();
    assert!(images.accept(image(Rendition::Current, 5, false)));
    assert!(!images.accept(image(Rendition::Current, 4, false)));
    assert!(images.accept(image(Rendition::Current, 6, false)));
    assert_eq!(
        images
            .get(&key(Rendition::Current))
            .unwrap()
            .whole
            .as_ref()
            .unwrap()
            .revision,
        6
    );
}

#[test]
fn retaining_drops_only_rejected_keys() {
    let mut images = RenderedImages::default();
    images.accept(image(Rendition::Current, 1, false));
    images.accept(image(Rendition::Camera, 2, false));
    images.retain(|key| key.look == Rendition::Current);
    assert!(images.get(&key(Rendition::Current)).is_some());
    assert!(images.get(&key(Rendition::Camera)).is_none());
    assert_eq!(images.images().count(), 1);
}

fn request(revision: u64, exposure: f32) -> RenderRequest {
    let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
    settings.exposure = exposure;
    RenderRequest {
        detail: true,
        framing_preview: false,
        calibration: None,
        rendition: Rendition::Current,
        id: RequestId(revision),
        path: "photo.x3f".into(),
        settings,
        preview_scale: foveonic::render::Scale::NATIVE,
        processing_quality: foveonic::render::ProcessingQuality::Quality,
        output: "final".into(),
        comparison: false,
    }
}

#[test]
fn a_failed_request_stays_failed_until_its_settings_change_or_its_photo_leaves() {
    let mut images = RenderedImages::<Image>::default();
    images.fail(std::sync::Arc::new(request(3, 0.)));
    assert!(
        images.failed(&request(7, 0.)),
        "replanning does not retry it"
    );
    assert!(!images.failed(&request(7, 0.5)), "an edit is a new request");
    images.retain(|key| key.look != Rendition::Current);
    assert!(!images.failed(&request(7, 0.)));
}

#[test]
fn the_current_image_stands_in_until_another_look_has_a_whole_image() {
    let mut images = RenderedImages::default();
    images.accept(image(Rendition::Current, 1, false));
    let camera = key(Rendition::Camera);
    assert_eq!(images.whole(&camera).map(|image| image.revision), Some(1));
    images.accept(image(Rendition::Camera, 2, false));
    assert_eq!(images.whole(&camera).map(|image| image.revision), Some(2));
}
