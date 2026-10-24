//! Preserves embedded display colors from camera metadata and JPEG profiles in declared RGB16 spaces.

use crate::{color_space::ColorSpace, source::Result};
use image::ImageDecoder;
use moxcms::{ColorProfile, DataColorSpace, Layout, TransformOptions};
use std::{collections::BTreeMap, sync::Arc};

// Keep cancellation responsive and temporary transform storage small.
const TRANSFORM_PIXELS: usize = 4096;
const RGB_CHANNELS: usize = 3;
const RGB8_TO_RGB16: u16 = u16::MAX / u8::MAX as u16;

#[derive(Clone)]
pub(super) enum Encoding {
    Srgb,
    AdobeRgb,
    Profile {
        profile: Arc<ColorProfile>,
        fallback: Box<Encoding>,
    },
}

impl Encoding {
    pub fn camera(properties: &BTreeMap<String, String>) -> Self {
        match properties.get("COLORSPACE").map(String::as_str) {
            Some("AdobeRGB") => Self::AdobeRgb,
            _ => Self::Srgb,
        }
    }

    /// A JPEG's own declaration overrides its camera or enclosing JPEG.
    /// Unlabelled Exif thumbnails inherit the enclosing photograph's encoding.
    pub fn jpeg(&self, encoded: &[u8]) -> Self {
        let fallback = match crate::x3f::jpeg_color_space(encoded) {
            Some(ColorSpace::Srgb) => Self::Srgb,
            Some(ColorSpace::AdobeRgb) => Self::AdobeRgb,
            _ => self.clone(),
        };
        let profile = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(encoded))
            .ok()
            .and_then(|mut decoder| decoder.icc_profile().ok().flatten())
            .and_then(|bytes| ColorProfile::new_from_slice(&bytes).ok())
            .filter(|p| p.color_space == DataColorSpace::Rgb);
        if let Some(profile) = profile {
            return Self::Profile {
                profile: Arc::new(profile),
                fallback: Box::new(fallback),
            };
        }
        fallback
    }

    /// Retain known camera primaries. Optional ICC profiles normalize into
    /// wide ROMM RGB16 rather than clipping their colors to sRGB.
    pub fn decode(&self, rgb: &[u8]) -> Result<(Vec<u16>, crate::color_space::TiffColorSpace)> {
        use crate::color_space::TiffColorSpace;
        let samples = || {
            rgb.iter()
                .map(|v| u16::from(*v) * RGB8_TO_RGB16)
                .collect::<Vec<_>>()
        };
        let (profile, fallback) = match self {
            Self::Srgb => return Ok((samples(), TiffColorSpace::Srgb)),
            Self::AdobeRgb => return Ok((samples(), TiffColorSpace::AdobeRgb)),
            Self::Profile { profile, fallback } => (profile, fallback),
        };
        let destination = ColorProfile::new_from_slice(TiffColorSpace::ProPhotoRgb.profile())?;
        let transform = match profile.create_transform_16bit(
            Layout::Rgb,
            &destination,
            Layout::Rgb,
            TransformOptions {
                rendering_intent: moxcms::RenderingIntent::RelativeColorimetric,
                ..Default::default()
            },
        ) {
            Ok(transform) => transform,
            // A parsed optional profile can still lack a usable RGB transform.
            // Fall back before changing any samples.
            Err(_) => return fallback.decode(rgb),
        };
        let input = samples();
        let mut converted = vec![0; input.len()];
        for (chunk, output) in input
            .chunks(TRANSFORM_PIXELS * RGB_CHANNELS)
            .zip(converted.chunks_mut(TRANSFORM_PIXELS * RGB_CHANNELS))
        {
            crate::background::checkpoint_or_cancel()?;
            transform.transform(chunk, output)?;
        }
        Ok((converted, TiffColorSpace::ProPhotoRgb))
    }
}
