//! Prepares display thumbnails and larger previews from upright images away from the UI thread.

use crate::display_image::Pixels;

const THUMBNAIL_SIDE: u32 = 256;

pub(crate) struct DecodedThumbnail {
    pub(crate) image: Pixels,
    pub(crate) native_extent: [u32; 2],
}
pub(crate) struct PreparedPreview {
    pub(crate) thumbnail: DecodedThumbnail,
    pub(crate) preview: Option<Pixels>,
}
impl PreparedPreview {
    pub(crate) fn retained_bytes(&self) -> u64 {
        self.thumbnail.image.retained_bytes()
            + self.preview.as_ref().map_or(0, Pixels::retained_bytes)
    }
    pub(crate) fn new(
        preview: foveonic::preview::Preview,
        large: bool,
    ) -> foveonic::source::Result<Self> {
        let (color_space, native_extent) = (preview.color_space, preview.upright_native_extent());
        let image = preview.upright_linear(if large {
            foveonic::decode_cache::PREVIEW_MAX_SIDE
        } else {
            THUMBNAIL_SIDE
        })?;
        // Release the decoded RGB16 input before making the display copies.
        drop(preview);
        Ok(Self::from_linear(image, color_space, native_extent, large))
    }
    /// `image` is upright and `native_extent` gives its upright photograph dimensions.
    pub(crate) fn from_linear(
        image: image::Rgb32FImage,
        color_space: foveonic::color_space::TiffColorSpace,
        native_extent: [u32; 2],
        large: bool,
    ) -> Self {
        let scale = (THUMBNAIL_SIDE as f64 / f64::from(image.width().max(image.height()))).min(1.);
        let small = foveonic::preview::resize_linear(
            &image,
            [
                (f64::from(image.width()) * scale).round().max(1.) as u32,
                (f64::from(image.height()) * scale).round().max(1.) as u32,
            ],
        );
        let thumbnail = DecodedThumbnail {
            image: Pixels::from_linear(&small, color_space.into()),
            native_extent,
        };
        let preview = large.then(|| Pixels::from_linear(&image, color_space.into()));
        Self { thumbnail, preview }
    }
}
#[cfg(test)]
impl DecodedThumbnail {
    fn from_preview(preview: foveonic::preview::Preview) -> foveonic::source::Result<Self> {
        Ok(PreparedPreview::new(preview, false)?.thumbnail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reduction_before_rotation_keeps_framing_and_linear_colors() {
        for space in foveonic::color_space::TiffColorSpace::ALL {
            let rgb: Vec<_> = (0..1025 * 601 * 3)
                .map(|i| (i as u16).wrapping_mul(7919))
                .collect();
            for rotation in [0, 90, 180, 270] {
                let preview = foveonic::preview::Preview {
                    rgb: rgb.clone(),
                    color_space: space,
                    extent: [1025, 601],
                    native_extent: [6000, 4000],
                    rotation,
                };
                let image = preview.linear_image().unwrap();
                let rotated = match rotation {
                    90 => image::imageops::rotate90(&image),
                    180 => image::imageops::rotate180(&image),
                    270 => image::imageops::rotate270(&image),
                    _ => image,
                };
                let prepared = PreparedPreview::new(preview, false).unwrap();
                let expected = foveonic::preview::resize_linear(
                    &rotated,
                    prepared.thumbnail.image.size.map(|v| v as u32),
                );
                let expected = Pixels::from_linear(&expected, space.into());
                assert_eq!(
                    prepared.thumbnail.native_extent,
                    if matches!(rotation, 90 | 270) {
                        [4000, 6000]
                    } else {
                        [6000, 4000]
                    }
                );
                for (actual, expected) in prepared.thumbnail.image.rgba.iter().zip(expected.rgba) {
                    for c in 0..4 {
                        let a = half::f16::from_bits(actual[c]).to_f32();
                        let b = half::f16::from_bits(expected[c]).to_f32();
                        assert!((a - b).abs() <= 0.001, "{space:?} {rotation}: {a} != {b}");
                    }
                }
                assert!(prepared.preview.is_none());
            }
        }
    }
    #[test]
    fn decoded_preview_downsampling_preserves_native_extent() {
        let preview = foveonic::preview::Preview {
            color_space: foveonic::color_space::TiffColorSpace::Srgb,
            rgb: vec![128 * 257; 600 * 300 * 3],
            extent: [600, 300],
            rotation: 0,
            native_extent: [6000, 3000],
        };
        let thumbnail = DecodedThumbnail::from_preview(preview).unwrap();
        assert_eq!(thumbnail.image.size, [256, 128]);
        assert_eq!(thumbnail.native_extent, [6000, 3000]);
    }
}
