//! Shares photographic export formats and packages encoded RGB samples with their color profiles.

use crate::{nodes::sensor::calibration::EncodeParams, render::TiffColorSpace, source::Result};
use image::ImageEncoder;
use serde::{Deserialize, Serialize};
use std::io::{Seek, Write};

mod jxl;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use tests::decode_jxl;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    #[default]
    Dng,
    Tiff,
    Jpeg,
    Png,
    Jxl,
}
impl Format {
    pub const ALL: [Self; 5] = [Self::Dng, Self::Tiff, Self::Jpeg, Self::Png, Self::Jxl];
    pub fn label(self) -> &'static str {
        match self {
            Self::Dng => "DNG",
            Self::Tiff => "TIFF",
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::Jxl => "JPEG XL",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Dng => "dng",
            Self::Tiff => "tiff",
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Jxl => "jxl",
        }
    }
    pub fn from_extension(path: &std::path::Path) -> Result<Self> {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match extension.as_str() {
            "tif" | "tiff" => Ok(Self::Tiff),
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            "png" => Ok(Self::Png),
            "jxl" => Ok(Self::Jxl),
            "dng" => Ok(Self::Dng),
            _ => Err(format!("unsupported output extension: {extension}").into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TiffCompression {
    #[default]
    None,
    Lzw,
    Zip,
}
impl TiffCompression {
    pub const ALL: [Self; 3] = [Self::None, Self::Lzw, Self::Zip];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Uncompressed",
            Self::Lzw => "LZW",
            Self::Zip => "ZIP",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BitDepth {
    Eight,
    #[default]
    Sixteen,
}

/// Output preferences stay separate from photographic edits and travel with each frozen job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EncodingOptions {
    pub tiff_compression: TiffCompression,
    pub jpeg_quality: u8,
    pub png_depth: BitDepth,
    pub jxl_lossless: bool,
    pub jxl_quality: u8,
    pub jxl_effort: u8,
}
impl Default for EncodingOptions {
    fn default() -> Self {
        Self {
            tiff_compression: TiffCompression::None,
            jpeg_quality: 95,
            png_depth: BitDepth::Sixteen,
            jxl_lossless: true,
            jxl_quality: 90,
            jxl_effort: 7,
        }
    }
}
impl EncodingOptions {
    pub(crate) fn jxl_lossless(self) -> bool {
        self.jxl_lossless || self.jxl_quality == 100
    }

    pub(crate) fn native_jxl(self, format: Format) -> bool {
        format == Format::Jxl && !self.jxl_lossless()
    }

    pub fn validate(self, format: Format) -> Result<()> {
        match format {
            Format::Dng => return Err("DNG requires the linear export recipe".into()),
            Format::Jpeg if !(1..=100).contains(&self.jpeg_quality) => {
                return Err("JPEG quality must be 1 through 100".into())
            }
            Format::Jxl => {
                if !(1..=11).contains(&self.jxl_effort) {
                    return Err("JPEG XL effort must be 1 through 11".into());
                }
                if !self.jxl_lossless && !(1..=100).contains(&self.jxl_quality) {
                    return Err("JPEG XL quality must be 1 through 100".into());
                }
            }
            _ => (),
        }
        Ok(())
    }
    pub(crate) fn key(self) -> [u8; 6] {
        [
            self.tiff_compression as u8,
            self.jpeg_quality,
            self.png_depth as u8,
            self.jxl_lossless as u8,
            self.jxl_quality,
            self.jxl_effort,
        ]
    }
}

/// Samples arrive as little-endian RGB16 with one padding component per pixel.
/// Encoders receive the already oriented, resized, destination-encoded image.
/// Lossy JXL uses native color signaling with a pure gamma ProPhoto transfer.
pub fn write<W: Write + Seek>(
    mut writer: W,
    bytes: &[u8],
    params: &EncodeParams,
    space: TiffColorSpace,
    format: Format,
    options: EncodingOptions,
    profile: &mut crate::profile::Profile,
) -> Result<()> {
    options.validate(format)?;
    let [width, height, _, _] = params.output;
    let count = (width as usize)
        .checked_mul(height as usize)
        .filter(|&n| n > 0)
        .ok_or("invalid output dimensions")?;
    if count.checked_mul(8) != Some(bytes.len()) {
        return Err("invalid packed RGB16 output".into());
    }
    let start = std::time::Instant::now();
    let pixels: Vec<u16> = bytes
        .as_chunks::<8>()
        .0
        .iter()
        .flat_map(|pixel| {
            [0, 2, 4].map(|offset| u16::from_le_bytes([pixel[offset], pixel[offset + 1]]))
        })
        .collect();
    profile.record("export RGB16 repack", start);
    let start = std::time::Instant::now();
    let rgb8 = || {
        pixels
            .iter()
            .map(|&v| ((u32::from(v) + 128) / 257) as u8)
            .collect::<Vec<_>>()
    };
    match format {
        Format::Tiff => {
            use tiff::encoder::{compression::DeflateLevel, Compression, Predictor};
            let compression = match options.tiff_compression {
                TiffCompression::None => Compression::Uncompressed,
                TiffCompression::Lzw => Compression::Lzw,
                TiffCompression::Zip => Compression::Deflate(DeflateLevel::Best),
            };
            let predictor = if options.tiff_compression == TiffCompression::None {
                Predictor::None
            } else {
                Predictor::Horizontal
            };
            let mut encoder = tiff::encoder::TiffEncoder::new(&mut writer)?
                .with_compression(compression)
                .with_predictor(predictor);
            let mut image = encoder.new_image::<tiff::encoder::colortype::RGB16>(width, height)?;
            image
                .encoder()
                .write_tag(tiff::tags::Tag::Unknown(34675), space.profile())?;
            image
                .encoder()
                .write_tag(tiff::tags::Tag::Orientation, 1_u16)?;
            image
                .encoder()
                .write_tag(tiff::tags::Tag::Software, "Foveonic GPU")?;
            image.write_data(&pixels)?;
        }
        Format::Jpeg => {
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut writer,
                options.jpeg_quality,
            );
            encoder.set_icc_profile(space.profile().to_vec())?;
            encoder.encode(&rgb8(), width, height, image::ExtendedColorType::Rgb8)?;
        }
        Format::Png => {
            use image::codecs::png::{CompressionType, FilterType, PngEncoder};
            let mut encoder = PngEncoder::new_with_quality(
                &mut writer,
                CompressionType::Best,
                FilterType::Adaptive,
            );
            encoder.set_icc_profile(space.profile().to_vec())?;
            match options.png_depth {
                BitDepth::Eight => {
                    encoder.write_image(&rgb8(), width, height, image::ExtendedColorType::Rgb8)?
                }
                BitDepth::Sixteen => encoder.write_image(
                    bytemuck::cast_slice(&pixels),
                    width,
                    height,
                    image::ExtendedColorType::Rgb16,
                )?,
            }
        }
        Format::Jxl => writer.write_all(&jxl::encode(&pixels, width, height, space, options)?)?,
        Format::Dng => unreachable!("validated photographic format"),
    }
    writer.flush()?;
    profile.record(format.label(), start);
    Ok(())
}
