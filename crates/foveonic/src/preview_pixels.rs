//! Resizes bounded preview samples in linear light while retaining their declared RGB primaries.

use super::Preview;
use crate::{
    color_space::{TiffColorSpace, TransferFunction},
    source::Result,
};
use std::sync::OnceLock;

const TRANSFORM_SAMPLES: usize = 4096 * 3;

#[cfg(test)]
#[path = "preview_pixels_test.rs"]
mod tests;

fn transfer_table(space: TiffColorSpace) -> &'static [f32] {
    static TABLES: OnceLock<[Vec<f32>; 3]> = OnceLock::new();
    &TABLES.get_or_init(|| {
        TiffColorSpace::ALL.map(|space| {
            (0..=u16::MAX)
                .map(|v| decode_sample(space, f32::from(v) / f32::from(u16::MAX)))
                .collect()
        })
    })[space as usize]
}

/// Decode only the pixels requested by the existing linear-light resampler.
/// This avoids retaining a full RGB32F copy beside a large embedded RGB16 image.
struct LinearPreview<'a> {
    preview: &'a Preview,
    table: &'static [f32],
}
impl image::GenericImageView for LinearPreview<'_> {
    type Pixel = image::Rgb<f32>;

    fn dimensions(&self) -> (u32, u32) {
        (self.preview.extent[0], self.preview.extent[1])
    }
    fn get_pixel(&self, x: u32, y: u32) -> Self::Pixel {
        let index = (y as usize * self.preview.extent[0] as usize + x as usize) * 3;
        image::Rgb(std::array::from_fn(|c| {
            self.table[usize::from(self.preview.rgb[index + c])]
        }))
    }
}

impl Preview {
    pub fn linear_image(&self) -> Result<image::Rgb32FImage> {
        // Share transfer tables across small thumbnails and fitted previews.
        // Avoid millions of power evaluations when preparing a large JPEG.
        let table = transfer_table(self.color_space);
        let mut linear = Vec::with_capacity(self.rgb.len());
        for chunk in self.rgb.chunks(TRANSFORM_SAMPLES) {
            crate::priority::checkpoint_or_cancel()?;
            linear.extend(chunk.iter().map(|v| table[usize::from(*v)]));
        }
        image::Rgb32FImage::from_raw(self.extent[0], self.extent[1], linear)
            .ok_or_else(|| "invalid decoded preview".into())
    }

    /// Reduce in linear light without first materializing a full floating image.
    pub fn linear_resized(&self, side: u32) -> Result<image::Rgb32FImage> {
        let samples = (self.extent[0] as usize)
            .checked_mul(self.extent[1] as usize)
            .and_then(|n| n.checked_mul(3));
        if side == 0 || self.extent.contains(&0) || samples != Some(self.rgb.len()) {
            return Err("invalid decoded preview or target size".into());
        }
        let scale = (f64::from(side) / f64::from(self.extent[0].max(self.extent[1]))).min(1.);
        let extent = self
            .extent
            .map(|v| (f64::from(v) * scale).round().max(1.) as u32);
        crate::priority::checkpoint_or_cancel()?;
        let view = LinearPreview {
            preview: self,
            table: transfer_table(self.color_space),
        };
        let image = image::imageops::resize(
            &view,
            extent[0],
            extent[1],
            image::imageops::FilterType::Triangle,
        );
        crate::priority::checkpoint_or_cancel()?;
        Ok(image)
    }

    pub fn resized(self, side: u32) -> Result<Self> {
        if self.extent[0].max(self.extent[1]) <= side {
            return Ok(self);
        }
        let image = self.linear_resized(side)?;
        drop(self.rgb);
        Ok(Self {
            rgb: image
                .as_raw()
                .iter()
                .map(|v| encode_sample(self.color_space, *v))
                .collect(),
            extent: [image.width(), image.height()],
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
