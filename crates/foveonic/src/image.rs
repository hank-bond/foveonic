//! Describes owned GPU image snapshots, their source context, and logical output views.

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
    /// RGBA32Float texture; alpha is one and RGB retains signed floating values.
    Rgba32FloatTexture,
    /// RGB16 in two u32 words per pixel; the final 16 bits are padding.
    PackedRgb16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    SensorLayers,
    ScalarField,
    CalibratedLayers,
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
/// Consumers have read-only access; the snapshot owns its storage for its lifetime.
pub struct RenderedImage {
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) description: Description,
    pub(crate) view: View,
    pub(crate) metadata: Arc<Metadata>,
    pub(crate) settings: Settings,
    pub(crate) owner: Arc<()>,
}

impl RenderedImage {
    pub fn buffer(&self) -> &wgpu::Buffer {
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
            Space::SensorLayers | Space::CalibratedLayers | Space::ScalarField => ColorSpace::Srgb,
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

/// Cropped/oriented floating RGB texture on the engine's device, retaining working primaries.
/// The presentation shader handles working-space conversion and display transfer encoding.
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
