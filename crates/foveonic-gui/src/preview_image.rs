//! Prepares oriented display thumbnails and larger previews away from the UI thread.

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
        let rotation = preview.rotation;
        let color_space = preview.color_space;
        let native_extent = preview.native_extent;
        let image = preview.linear_image()?;
        drop(preview);
        Ok(Self::from_linear(
            image,
            rotation,
            color_space,
            native_extent,
            large,
        ))
    }
    pub(crate) fn from_linear(
        image: image::Rgb32FImage,
        rotation: u32,
        color_space: foveonic::color_space::TiffColorSpace,
        mut native_extent: [u32; 2],
        large: bool,
    ) -> Self {
        fn rotate(image: image::Rgb32FImage, rotation: u32) -> image::Rgb32FImage {
            match rotation {
                90 => image::imageops::rotate90(&image),
                180 => image::imageops::rotate180(&image),
                270 => image::imageops::rotate270(&image),
                _ => image,
            }
        }
        let image = rotate(image, rotation);
        if matches!(rotation, 90 | 270) {
            native_extent.swap(0, 1);
        }
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
    fn decoded_preview_rotates_pixels_and_native_extent_once() {
        let preview = foveonic::preview::Preview {
            color_space: foveonic::color_space::TiffColorSpace::Srgb,
            rgb: (0..128)
                .flat_map(|_| {
                    (0..256).flat_map(|x| {
                        if x < 128 {
                            [65535, 0, 0]
                        } else {
                            [0, 65535, 0]
                        }
                    })
                })
                .collect(),
            extent: [256, 128],
            rotation: 90,
            native_extent: [2000, 1000],
        };
        let thumbnail = DecodedThumbnail::from_preview(preview).unwrap();
        assert_eq!(thumbnail.native_extent, [1000, 2000]);
        assert_eq!(thumbnail.image.size, [128, 256]);
        let one = half::f16::ONE.to_bits();
        assert_eq!(thumbnail.image.rgba[0], [one, 0, 0, one]);
        assert_eq!(thumbnail.image.rgba[128 * 255], [0, one, 0, one]);
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
