//! Checks that a Merrill High original renders at half size in its camera color space.

use foveonic::color_space::TiffColorSpace;
use foveonic::render::EncodedImage;

#[test]
#[ignore = "requires a GPU and MERRILL_X3F (a Merrill High original)"]
fn merrill_high_previews_render_at_half_size() {
    let path = std::env::var_os("MERRILL_X3F").expect("MERRILL_X3F");
    let image = foveonic_quicklook::render_preview(path.as_ref()).unwrap();
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
