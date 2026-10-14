//! Keeps GPU images with their photo settings and defines crop and orientation views.

pub use crate::calibration::ColorSpace;
use crate::calibration::EncodeParams;
use crate::render::Settings;
use crate::source::Metadata;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Three interleaved f32 components, 12 bytes per pixel.
    Rgb32Float,
    /// One floating component per pixel, for masks and diagnostic fields.
    Scalar32Float,
    /// RGBA32Float texture. Alpha is one and RGB can retain negative values.
    Rgba32FloatTexture,
    /// RGB16 in two u32 words per pixel. The final 16 bits are padding.
    PackedRgb16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    SensorLayers,
    ScalarField,
    /// Luminance ratio. One leaves brightness unchanged and the display range is 0..2.
    LuminanceRatio,
    /// Signed reconstruction detail in camera code units. The display range is −1000..1000.
    SignedDetail,
    CalibratedLayers,
    /// Camera-color RGB in code units, with a nominal inspection range of 0..4095.
    LinearRgb(ColorSpace),
    /// Tone-mapped components in linear working primaries.
    ToneMappedRgb(ColorSpace),
    EncodedRgb16(ColorSpace),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Description {
    /// Physical storage dimensions.
    pub extent: [u32; 2],
    pub layout: Layout,
    pub space: Space,
}

impl Description {
    pub fn byte_len(self) -> u64 {
        u64::from(self.extent[0])
            * u64::from(self.extent[1])
            * match self.layout {
                Layout::Rgb32Float => 12,
                Layout::Scalar32Float => 4,
                Layout::Rgba32FloatTexture => 16,
                Layout::PackedRgb16 => 8,
            }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    /// Half-open active rectangle in physical storage coordinates.
    pub crop: [u32; 4],
    pub rotation: u32,
    /// Final oriented dimensions, including requested output resizing.
    pub extent: [u32; 2],
}

/// An immutable GPU snapshot with source metadata and rendering settings.
/// Its read claim keeps the samples unchanged even when later renders reuse storage.
#[derive(Clone)]
pub struct RenderedImage {
    pub(crate) buffer: crate::gpu::ReadGuard,
    pub(crate) description: Description,
    pub(crate) view: View,
    pub(crate) metadata: Arc<Metadata>,
    pub(crate) settings: Settings,
    pub(crate) owner: Arc<()>,
}

impl RenderedImage {
    pub(crate) fn buffer(&self) -> &crate::gpu::ReadGuard {
        &self.buffer
    }
    pub fn description(&self) -> Description {
        self.description
    }
    pub fn view(&self) -> View {
        self.view
    }
    pub fn source_metadata(&self) -> &Metadata {
        &self.metadata
    }
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub(crate) fn params(&self) -> EncodeParams {
        let space = match self.description.space {
            Space::LinearRgb(space) | Space::ToneMappedRgb(space) | Space::EncodedRgb16(space) => {
                space
            }
            Space::SensorLayers
            | Space::CalibratedLayers
            | Space::ScalarField
            | Space::LuminanceRatio
            | Space::SignedDetail => ColorSpace::Srgb,
        };
        EncodeParams {
            source: [
                self.description.extent[0],
                self.description.extent[1],
                self.view.rotation,
                u32::from(self.description.layout == Layout::Scalar32Float),
            ],
            crop: self.view.crop,
            output: [
                self.view.extent[0],
                self.view.extent[1],
                u32::from(space == ColorSpace::AdobeRgb),
                0,
            ],
        }
    }
}

/// Keep this preview in the working color space so display conversion happens
/// in the presentation shader, not in the stored photo.
pub struct ImageTexture {
    pub(crate) texture: wgpu::Texture,
    pub(crate) description: Description,
}

impl ImageTexture {
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
    pub fn description(&self) -> Description {
        self.description
    }
}
