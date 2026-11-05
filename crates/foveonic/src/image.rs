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
    /// Binary diagnostic mask on the complete physical sensor grid.
    SensorMask,
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
    /// Linear-light RGB stored as integral codes, with a recipe-defined nominal white.
    LinearCodeRgb,
    /// Signed root-domain RGB in the calibrated color basis, stored as integral code values.
    SignedCodeRgb,
    /// Unsigned16 RGB codes, represented as integral Float32 values.
    UnsignedCodeRgb,
    /// Camera-color RGB in linear codes. Description declares their nominal white.
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
            | Self::SensorMask
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
            | Self::UnsignedCodeRgb
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
                | Self::SensorMask
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Description {
    /// Nominal white for LinearRgb code buffers. Other spaces ignore this field.
    pub linear_white: u32,
    /// Physical storage dimensions.
    pub extent: [u32; 2],
    pub layout: Layout,
    pub space: Space,
    /// The prepared camera view has been materialized into storage.
    /// Authored composition remains a view on a rendered buffer.
    pub view_baked: bool,
}

impl Description {
    pub(crate) fn new(extent: [u32; 2], space: Space) -> Self {
        Self {
            extent,
            linear_white: 4095,
            layout: space.buffer_layout(),
            space,
            view_baked: false,
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

impl View {
    /// Map a picker position in the oriented crop back into physical storage.
    pub(crate) fn pixel_at_uv(self, uv: [f32; 2]) -> crate::source::Result<[u32; 2]> {
        if uv.iter().any(|v| !v.is_finite() || !(0. ..=1.).contains(v)) {
            return Err("Pick inside the photograph.".into());
        }
        let [l, t, r, b] = self.crop;
        if l >= r || t >= b {
            return Err("Cannot sample an empty image.".into());
        }
        let [w, h] = if matches!(self.rotation, 90 | 270) {
            [b - t, r - l]
        } else {
            [r - l, b - t]
        };
        let x = ((uv[0] * w as f32) as u32).min(w - 1);
        let y = ((uv[1] * h as f32) as u32).min(h - 1);
        match self.rotation {
            0 => Ok([l + x, t + y]),
            90 => Ok([l + y, b - 1 - x]),
            180 => Ok([r - 1 - x, b - 1 - y]),
            270 => Ok([r - 1 - y, t + x]),
            _ => Err("Unsupported picker orientation.".into()),
        }
    }
}

#[test]
fn picker_coordinates_follow_crop_rotation_and_inclusive_uv_edges() {
    for (rotation, first, last) in [
        (0, [10, 20], [29, 49]),
        (90, [10, 49], [29, 20]),
        (180, [29, 49], [10, 20]),
        (270, [29, 20], [10, 49]),
    ] {
        let view = View {
            crop: [10, 20, 30, 50],
            rotation,
            extent: [20, 30],
        };
        assert_eq!(view.pixel_at_uv([0., 0.]).unwrap(), first);
        assert_eq!(view.pixel_at_uv([1., 1.]).unwrap(), last);
        assert!(view.pixel_at_uv([f32::NAN, 0.]).is_err());
        assert!(view.pixel_at_uv([-0.1, 0.]).is_err());
    }
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
    pub(crate) dehaze_basis: Option<crate::nodes::finish::dehaze::Basis>,
}

impl RenderedImage {
    pub(crate) fn buffer(&self) -> &crate::gpu::ReadGuard {
        &self.buffer
    }
    pub(crate) fn pixel_at_uv(&self, uv: [f32; 2]) -> crate::source::Result<[u32; 2]> {
        let view = self.view();
        view.pixel_at_uv(uv)?;
        let params = self.params();
        if params.mapping[0][3] == 0. {
            return view.pixel_at_uv(uv);
        }
        let q = uv.map(f64::from);
        Ok(std::array::from_fn(|i| {
            (f64::from(params.mapping[i][0]) * q[0]
                + f64::from(params.mapping[i][1]) * q[1]
                + f64::from(params.mapping[i][2]))
            .round()
            .clamp(f64::from(view.crop[i]), f64::from(view.crop[i + 2] - 1)) as u32
        }))
    }
    pub fn description(&self) -> Description {
        self.description
    }
    pub fn view(&self) -> View {
        if self.description.space.uses_full_sensor_view() || self.settings.preview_region.is_some()
        {
            self.view
        } else {
            self.settings
                .rotation
                .view(self.view, self.settings.crop, self.settings.output_size)
        }
    }
    /// The prepared camera view before the authored crop.
    pub fn uncropped_view(&self) -> View {
        self.view
    }
    /// Present the camera-oriented source for draft rotation, retaining immutable samples.
    pub fn without_composition(&self) -> Self {
        let mut image = self.without_crop();
        image.settings.rotation = Default::default();
        image.settings.output_size = None;
        image
    }
    /// Expand crop editing to the complete rotated photograph.
    pub fn without_crop(&self) -> Self {
        let mut image = self.clone();
        image.settings.crop = Default::default();
        image
    }
    pub fn source_metadata(&self) -> &Metadata {
        &self.metadata
    }
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub(crate) fn params(&self) -> EncodeParams {
        let view = self.view();
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
            | Space::UnsignedCodeRgb
            | Space::CalibratedLayers
            | Space::ScalarField
            | Space::SensorMask
            | Space::Monochrome
            | Space::LuminanceRatio
            | Space::SignedDetail => 0,
        };
        EncodeParams {
            mapping: if self.description.space.uses_full_sensor_view()
                || self.settings.preview_region.is_some()
            {
                [[0.; 4]; 2]
            } else {
                self.settings.rotation.mapping(view, self.settings.crop)
            },
            source: [
                self.description.extent[0],
                self.description.extent[1],
                view.rotation,
                u32::from(self.description.layout == Layout::Scalar32Float),
            ],
            crop: view.crop,
            output: [view.extent[0], view.extent[1], transfer, 0],
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
