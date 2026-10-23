//! Prepares oriented display thumbnails and larger previews away from the UI thread.

use eframe::egui;

const THUMBNAIL_SIDE: u32 = 256;

pub(crate) struct DecodedThumbnail {
    pub(crate) image: egui::ColorImage,
    pub(crate) native_extent: [u32; 2],
}
pub(crate) struct PreparedPreview {
    pub(crate) thumbnail: DecodedThumbnail,
    pub(crate) preview: Option<egui::ColorImage>,
}
impl PreparedPreview {
    pub(crate) fn new(
        preview: foveonic::preview::Preview,
        large: bool,
    ) -> foveonic::source::Result<Self> {
        let image = image::RgbImage::from_raw(preview.extent[0], preview.extent[1], preview.rgb)
            .ok_or("invalid decoded preview")?;
        let image = match preview.rotation {
            90 => image::imageops::rotate90(&image),
            180 => image::imageops::rotate180(&image),
            270 => image::imageops::rotate270(&image),
            _ => image,
        };
        let mut native_extent = preview.native_extent;
        if matches!(preview.rotation, 90 | 270) {
            native_extent.swap(0, 1);
        }
        let scale = (THUMBNAIL_SIDE as f64 / f64::from(image.width().max(image.height()))).min(1.);
        let small = image::imageops::thumbnail(
            &image,
            (f64::from(image.width()) * scale).round().max(1.) as u32,
            (f64::from(image.height()) * scale).round().max(1.) as u32,
        );
        let thumbnail = DecodedThumbnail {
            image: egui::ColorImage::from_rgb(
                [small.width() as usize, small.height() as usize],
                &small,
            ),
            native_extent,
        };
        let preview = large.then(|| {
            egui::ColorImage::from_rgb([image.width() as usize, image.height() as usize], &image)
        });
        Ok(Self { thumbnail, preview })
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
            rgb: (0..128)
                .flat_map(|_| {
                    (0..256).flat_map(|x| if x < 128 { [255, 0, 0] } else { [0, 255, 0] })
                })
                .collect(),
            extent: [256, 128],
            rotation: 90,
            native_extent: [2000, 1000],
        };
        let thumbnail = DecodedThumbnail::from_preview(preview).unwrap();
        assert_eq!(thumbnail.native_extent, [1000, 2000]);
        assert_eq!(thumbnail.image.size, [128, 256]);
        assert_eq!(thumbnail.image.pixels[0], eframe::egui::Color32::RED);
        assert_eq!(
            thumbnail.image.pixels[128 * 255],
            eframe::egui::Color32::GREEN
        );
    }

    #[test]
    fn decoded_preview_downsampling_preserves_native_extent() {
        let preview = foveonic::preview::Preview {
            rgb: vec![128; 600 * 300 * 3],
            extent: [600, 300],
            rotation: 0,
            native_extent: [6000, 3000],
        };
        let thumbnail = DecodedThumbnail::from_preview(preview).unwrap();
        assert_eq!(thumbnail.image.size, [256, 128]);
        assert_eq!(thumbnail.native_extent, [6000, 3000]);
    }
}
