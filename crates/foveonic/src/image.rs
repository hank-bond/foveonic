//! Keeps GPU images with their photo settings and defines crop and orientation views.

pub use crate::color_space::ColorSpace;
use crate::nodes::sensor::calibration::EncodeParams;
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
    /// Original camera RGB codes, including blue averaged over a sensor cell.
    SensorRgb,
    /// Original full-resolution blue sensor codes.
    SensorBlue,
    /// Low-resolution camera RGB after offset and code-scale correction.
    CorrectedSensorRgb,
    /// High-resolution blue after offset, code-scale correction and upper cap.
    CorrectedSensorBlue,
    SensorLayers,
    ScalarField,
    /// Single-channel monochrome brightness in linear camera-code units.
    Monochrome,
    /// Luminance ratio. One leaves brightness unchanged and the display range is 0..2.
    LuminanceRatio,
    /// Signed reconstruction detail in camera code units. The display range is −1000..1000.
    SignedDetail,
    CalibratedLayers,
    /// Signed linear codes awaiting the reconstruction finalizer's required gain.
    NormalizedCodeRgb,
    /// Signed linear RGB with required gain applied, stored as integral code values.
    LinearCodeRgb,
    /// Signed root-domain RGB in the calibrated color basis, stored as integral code values.
    SignedCodeRgb,
    /// Camera-color RGB in code units, with a nominal inspection range of 0..4095.
    LinearRgb(ColorSpace),
    /// Tone-mapped components in linear working primaries.
    ToneMappedRgb(ColorSpace),
    EncodedRgb16(crate::render::TiffColorSpace),
}

impl Space {
    /// Canonical graph-buffer storage. Materialized textures declare their own layout.
    pub fn buffer_layout(self) -> Layout {
        match self {
            Self::SensorBlue
            | Self::CorrectedSensorBlue
            | Self::ScalarField
            | Self::Monochrome
            | Self::LuminanceRatio
            | Self::SignedDetail => Layout::Scalar32Float,
            Self::EncodedRgb16(_) => Layout::PackedRgb16,
            Self::SensorRgb
            | Self::CorrectedSensorRgb
            | Self::SensorLayers
            | Self::CalibratedLayers
            | Self::NormalizedCodeRgb
            | Self::LinearCodeRgb
            | Self::SignedCodeRgb
            | Self::LinearRgb(_)
            | Self::ToneMappedRgb(_) => Layout::Rgb32Float,
        }
    }

    /// These intermediate planes expose their complete physical grid for inspection.
    pub(crate) fn uses_full_sensor_view(self) -> bool {
        matches!(
            self,
            Self::SensorRgb
                | Self::SensorBlue
                | Self::CorrectedSensorRgb
                | Self::CorrectedSensorBlue
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Description {
    /// Physical storage dimensions.
    pub extent: [u32; 2],
    pub layout: Layout,
    pub space: Space,
}

impl Description {
    pub(crate) fn new(extent: [u32; 2], space: Space) -> Self {
        Self {
            extent,
            layout: space.buffer_layout(),
            space,
        }
    }

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
        let transfer = match self.description.space {
            Space::LinearRgb(space) | Space::ToneMappedRgb(space) => space as u32,
            Space::EncodedRgb16(space) => space as u32,
            Space::CorrectedSensorRgb
            | Space::CorrectedSensorBlue
            | Space::SensorRgb
            | Space::SensorBlue
            | Space::SensorLayers
            | Space::NormalizedCodeRgb
            | Space::LinearCodeRgb
            | Space::SignedCodeRgb
            | Space::CalibratedLayers
            | Space::ScalarField
            | Space::Monochrome
            | Space::LuminanceRatio
            | Space::SignedDetail => 0,
        };
        EncodeParams {
            source: [
                self.description.extent[0],
                self.description.extent[1],
                self.view.rotation,
                u32::from(self.description.layout == Layout::Scalar32Float),
            ],
            crop: self.view.crop,
            output: [self.view.extent[0], self.view.extent[1], transfer, 0],
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
