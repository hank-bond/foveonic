//! Normalizes embedded display pixels from camera metadata and JPEG profiles to sRGB.

use crate::{color_space::ColorSpace, source::Result};
use image::ImageDecoder;
use moxcms::{ColorProfile, DataColorSpace, Layout, TransformOptions};
use std::{collections::BTreeMap, sync::Arc};

// Keep cancellation responsive and temporary transform storage small.
const TRANSFORM_PIXELS: usize = 4096;
const RGB_CHANNELS: usize = 3;

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

    /// Color management is part of decoding small display assets. RAW processing
    /// stays on the GPU, and already-normalized rendered previews bypass this path.
    pub fn to_srgb(&self, rgb: &mut [u8]) -> Result<()> {
        let (profile, fallback) = match self {
            Self::Srgb => return Ok(()),
            Self::AdobeRgb => (Arc::new(ColorProfile::new_adobe_rgb()), None),
            Self::Profile { profile, fallback } => (profile.clone(), Some(fallback)),
        };
        let transform = match profile.create_transform_8bit(
            Layout::Rgb,
            &ColorProfile::new_srgb(),
            Layout::Rgb,
            TransformOptions {
                rendering_intent: moxcms::RenderingIntent::RelativeColorimetric,
                ..Default::default()
            },
        ) {
            Ok(transform) => transform,
            // Parsing can succeed even when a profile lacks data required for
            // an RGB transform. Optional profiles must not hide a usable preview.
            Err(error) => {
                return match fallback {
                    Some(encoding) => encoding.to_srgb(rgb),
                    None => Err(error.into()),
                }
            }
        };
        let mut converted = vec![0; TRANSFORM_PIXELS * RGB_CHANNELS];
        for chunk in rgb.chunks_mut(converted.len()) {
            crate::background::checkpoint_or_cancel()?;
            let output = &mut converted[..chunk.len()];
            transform.transform(chunk, output)?;
            chunk.copy_from_slice(output);
        }
        Ok(())
    }
}
