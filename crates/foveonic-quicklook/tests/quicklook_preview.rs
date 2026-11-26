//! Checks Quick Look previews and thumbnails against tracked original photographs.

use foveonic::color_space::TiffColorSpace;
use foveonic::render::EncodedImage;
use std::path::PathBuf;

fn photograph(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").expect("FOVEONIC_FIXTURE_DIR"))
        .join(name)
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn merrill_high_previews_render_at_half_size() {
    let image = foveonic_quicklook::render_preview(&photograph("blue-parking-sign.x3f")).unwrap();
    let [width, height] = image.extent;
    assert_eq!(width.max(height), 2352);
    assert_eq!(width.min(height), 1568);
    assert_eq!(
        image.samples.len(),
        width as usize * height as usize * EncodedImage::BYTES_PER_PIXEL
    );
    assert!(matches!(
        image.color_space,
        TiffColorSpace::Srgb | TiffColorSpace::AdobeRgb
    ));
    // A real photograph has both dark and bright samples, not a blank buffer.
    let red = image
        .samples
        .as_chunks::<{ EncodedImage::BYTES_PER_PIXEL }>()
        .0
        .iter()
        .map(|pixel| u16::from_le_bytes([pixel[0], pixel[1]]));
    let (low, high) = red.fold((u16::MAX, 0), |(low, high), v| (low.min(v), high.max(v)));
    assert!(high - low > u16::MAX / 4, "{low}..{high}");
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR"]
fn thumbnails_are_upright_and_fit_the_request() {
    // Stored sideways: the full-size JPEG is 4704x3136 with a 90 degree rotation.
    let merrill = photograph("blue-parking-sign.x3f");
    for side in [128, 512] {
        let image = foveonic_quicklook::render_thumbnail(&merrill, side).unwrap();
        let [width, height] = image.extent;
        assert_eq!(height, side, "{side}");
        assert!(width < height, "portrait once upright: {width}x{height}");
        assert_eq!(
            image.samples.len(),
            width as usize * height as usize * EncodedImage::BYTES_PER_PIXEL
        );
    }
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR"]
fn thumbnails_never_use_a_differently_framed_camera_jpeg() {
    // DP0's full-size JPEG is cropped to 4:3. Only its small image shows the Raw's 3:2.
    let image = foveonic_quicklook::render_thumbnail(&photograph("dp0-quattro.x3f"), 1024).unwrap();
    assert_eq!(image.extent, [160, 106]);
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR"]
fn a_failed_render_falls_back_to_the_upright_embedded_image() {
    let path = photograph("blue-parking-sign.x3f");
    let image = foveonic_quicklook::or_embedded(&path, || Err("render failed".into())).unwrap();
    assert_eq!(image.extent[1], 2048);
    assert!(image.extent[0] < image.extent[1]);
    let panicked = foveonic_quicklook::or_embedded(&path, || panic!("GPU validation")).unwrap();
    assert_eq!(panicked.extent, image.extent);
    let missing = photograph("missing.x3f");
    let error = foveonic_quicklook::or_embedded(&missing, || Err("render failed".into()));
    assert_eq!(error.err().unwrap().to_string(), "render failed");
}
