//! Preserves embedded display colors from camera metadata and JPEG profiles in declared RGB16 spaces.

use crate::{color_space::ColorSpace, source::Result};
use image::ImageDecoder;
use moxcms::{ColorProfile, DataColorSpace, Layout, TransformOptions};
use std::{collections::BTreeMap, sync::Arc};

// Keep cancellation responsive and temporary transform storage small.
const TRANSFORM_PIXELS: usize = 4096;
const RGB_CHANNELS: usize = 3;
const RGB8_TO_RGB16: u16 = u16::MAX / u8::MAX as u16;

fn profile_warning(boundary: &str, error: &(dyn std::error::Error + 'static)) {
    static READ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static PARSE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static TRANSFORM: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let counter = match boundary {
        "preview_color.profile_read" => &READ,
        "preview_color.profile_parse" => &PARSE,
        _ => &TRANSFORM,
    };
    let occurrences = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    if occurrences.is_power_of_two() {
        crate::support::warning_error(
            boundary,
            error,
            crate::support::OperationContext {
                component: Some("embedded preview".into()),
                settings: Some(serde_json::json!({"occurrences":occurrences})),
                ..Default::default()
            },
        );
    }
}

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
            .and_then(|mut decoder| match decoder.icc_profile() {
                Ok(profile) => profile,
                Err(error) => {
                    profile_warning("preview_color.profile_read", &error);
                    None
                }
            })
            .and_then(|bytes| match ColorProfile::new_from_slice(&bytes) {
                Ok(profile) => Some(profile),
                Err(error) => {
                    profile_warning("preview_color.profile_parse", &error);
                    None
                }
            })
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
            Err(error) => {
                profile_warning("preview_color.profile_transform", &error);
                return fallback.decode(rgb);
            }
        };
        let mut input = Vec::with_capacity(TRANSFORM_PIXELS * RGB_CHANNELS);
        let mut converted = vec![0; rgb.len()];
        for (chunk, output) in rgb
            .chunks(TRANSFORM_PIXELS * RGB_CHANNELS)
            .zip(converted.chunks_mut(TRANSFORM_PIXELS * RGB_CHANNELS))
        {
            crate::background::checkpoint_or_cancel()?;
            input.clear();
            input.extend(chunk.iter().map(|v| u16::from(*v) * RGB8_TO_RGB16));
            transform.transform(&input, output)?;
        }
        Ok((converted, TiffColorSpace::ProPhotoRgb))
    }
}
