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
    /// RGBA32Float texture; alpha is one and RGB retains signed floating values.
    Rgba32FloatTexture,
    /// RGB16 in two u32 words per pixel; the final 16 bits are padding.
    PackedRgb16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    SensorLayers,
    CalibratedLayers,
    LinearRgb(ColorSpace),
    /// Tone has been applied, but components are still in linear primaries, without transfer encoding.
    ToneMappedRgb(ColorSpace),
    EncodedRgb16(ColorSpace),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Description {
    /// Physical storage dimensions, not the dimensions of a cropped/rotated view.
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

/// An immutable, independently owned snapshot. Buffer consumers must only read its storage.
/// It retains metadata and a settings snapshot, but not the source's decoded CPU pixels.
/// Graph scratch never includes this resource after it has been returned.
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
        let Space::ToneMappedRgb(space) = self.description.space else {
            unreachable!("rendered image is tone-mapped RGB")
        };
        EncodeParams {
            source: [
                self.description.extent[0],
                self.description.extent[1],
                self.view.rotation,
                0,
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

/// Cropped/oriented floating RGB texture on the engine's device. Components are not monitor encoded;
/// the GUI presentation shader must handle working-space conversion and display transfer encoding.
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
