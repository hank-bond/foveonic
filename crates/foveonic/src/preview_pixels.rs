//! Resizes bounded preview samples in linear light while retaining their declared RGB primaries.

use super::Preview;
use crate::{
    color_space::{TiffColorSpace, TransferFunction},
    source::Result,
};
use std::sync::OnceLock;

const TRANSFORM_SAMPLES: usize = 4096 * 3;

impl Preview {
    pub fn linear_image(&self) -> Result<image::Rgb32FImage> {
        // Share transfer tables across small thumbnails and fitted previews.
        // Avoid millions of power evaluations when preparing a large JPEG.
        static TABLES: OnceLock<[Vec<f32>; 3]> = OnceLock::new();
        let tables = TABLES.get_or_init(|| {
            TiffColorSpace::ALL.map(|space| {
                (0..=u16::MAX)
                    .map(|v| decode_sample(space, f32::from(v) / f32::from(u16::MAX)))
                    .collect()
            })
        });
        let table = &tables[self.color_space as usize];
        let mut linear = Vec::with_capacity(self.rgb.len());
        for chunk in self.rgb.chunks(TRANSFORM_SAMPLES) {
            crate::background::checkpoint_or_cancel()?;
            linear.extend(chunk.iter().map(|v| table[usize::from(*v)]));
        }
        image::Rgb32FImage::from_raw(self.extent[0], self.extent[1], linear)
            .ok_or_else(|| "invalid decoded preview".into())
    }

    pub fn resized(self, side: u32) -> Result<Self> {
        if self.extent[0].max(self.extent[1]) <= side {
            return Ok(self);
        }
        let scale = f64::from(side) / f64::from(self.extent[0].max(self.extent[1]));
        let extent = self
            .extent
            .map(|v| (f64::from(v) * scale).round().max(1.) as u32);
        let image = resize_linear(&self.linear_image()?, extent);
        Ok(Self {
            rgb: image
                .as_raw()
                .iter()
                .map(|v| encode_sample(self.color_space, *v))
                .collect(),
            extent,
            ..self
        })
    }
}

/// Samples remain bounded in their original primaries during reduction.
pub fn resize_linear(image: &image::Rgb32FImage, extent: [u32; 2]) -> image::Rgb32FImage {
    // image::thumbnail adds integer rounding even to floats, brightening them.
    // The floating resize path handles fractional samples correctly.
    image::imageops::resize(
        image,
        extent[0],
        extent[1],
        image::imageops::FilterType::Triangle,
    )
}

/// Decode display assets in their declared primaries, independently of TIFF recipes.
pub fn decode_sample(space: TiffColorSpace, encoded: f32) -> f32 {
    // Embedded Adobe RGB uses the standard's 563/256 transfer. The TIFF
    // export recipe retains its existing separately selected encoding.
    if space == TiffColorSpace::AdobeRgb {
        return encoded.powf(563. / 256.);
    }
    TransferFunction::from(space).decode(f64::from(encoded)) as f32
}

/// Encode bounded display assets without changing their primaries.
pub fn encode_sample(space: TiffColorSpace, linear: f32) -> u16 {
    let value = linear.clamp(0., 1.);
    let encoded = match space {
        TiffColorSpace::Srgb if value <= 0.0031308 => value * 12.92,
        TiffColorSpace::Srgb => 1.055 * value.powf(1. / 2.4) - 0.055,
        TiffColorSpace::AdobeRgb => value.powf(256. / 563.),
        TiffColorSpace::ProPhotoRgb if value < 1. / 512. => value * 16.,
        TiffColorSpace::ProPhotoRgb => value.powf(1. / 1.8),
    };
    (encoded * f32::from(u16::MAX)).round() as u16
}
