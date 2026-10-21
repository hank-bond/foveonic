//! Builds histograms and smooth waveform traces from GPU image regions.

use crate::gpu::Binding::{Read, Write};
use crate::{
    gpu::{Gpu, Kernel},
    image::{RenderedImage, Space},
    profile::Profile,
    source::Result,
};

pub const BINS: usize = 256;
/// One sample contributes this many weight units to the waveform.
pub const DENSITY_SCALE: u32 = 256;
const DENSITY_OFFSET: usize = 4 * BINS;
const OUTSIDE_OFFSET: usize = DENSITY_OFFSET + 4 * BINS * BINS;
const COUNT: usize = OUTSIDE_OFFSET + 12;

/// Histograms count samples. Waveforms share each sample's weight across nearby cells.
/// RGB keeps the buffer's color space and transfer state. Sensor codes use 0–4095.
pub struct Scopes {
    pub space: Space,
    pub samples: [u32; 2],
    pub counts: Vec<u32>,
}
impl Scopes {
    pub fn channels(&self) -> usize {
        if self.space.buffer_layout() == crate::image::Layout::Scalar32Float {
            1
        } else {
            3
        }
    }
    pub fn histogram(&self, channel: usize) -> &[u32] {
        &self.counts[channel * BINS..(channel + 1) * BINS]
    }
    /// Waveform cell weights. Divide by DENSITY_SCALE to get sample units.
    pub fn density(&self, channel: usize) -> &[u32] {
        let offset = DENSITY_OFFSET + channel * BINS * BINS;
        &self.counts[offset..offset + BINS * BINS]
    }
    pub fn outside(&self, channel: usize) -> [u32; 3] {
        [
            self.counts[OUTSIDE_OFFSET + channel],
            self.counts[OUTSIDE_OFFSET + 4 + channel],
            self.counts[OUTSIDE_OFFSET + 8 + channel],
        ]
    }
}

/// [left, top, right, bottom] in cropped, rotated output pixels.
/// Right and bottom are excluded.
pub type Region = [u32; 4];

pub(crate) fn analyze(
    gpu: &Gpu,
    image: &RenderedImage,
    region: Option<Region>,
    profile: &mut Profile,
) -> Result<Scopes> {
    let mut params = image.params();
    let [width, height] = image.view().extent;
    let [left, top, right, bottom] = region.unwrap_or([0, 0, width, height]);
    if left >= right || top >= bottom || right > width || bottom > height {
        return Err("scope region must be nonempty and inside the oriented image".into());
    }
    let [w, h] = [right - left, bottom - top];
    let region = gpu.uniform(&[
        left as f32 / width as f32,
        top as f32 / height as f32,
        right as f32 / width as f32,
        bottom as f32 / height as f32,
    ]);
    let factor = (512. / w as f32).min(512. / h as f32).min(1.);
    let samples = [
        (w as f32 * factor).round().max(1.) as u32,
        (h as f32 * factor).round().max(1.) as u32,
    ];
    let space = image.description().space;
    params.output = [
        samples[0],
        samples[1],
        match space {
            Space::CorrectedSensorRgb
            | Space::CorrectedSensorBlue
            | Space::SensorRgb
            | Space::SensorBlue => 4,
            Space::SensorLayers
            | Space::NormalizedCodeRgb
            | Space::LinearCodeRgb
            | Space::SignedCodeRgb
            | Space::CalibratedLayers
            | Space::Monochrome
            | Space::LinearRgb(_) => 1,
            Space::SignedDetail => 2,
            Space::LuminanceRatio => 3,
            _ => 0,
        },
        match space {
            Space::LinearRgb(space) | Space::ToneMappedRgb(space) => space as u32,
            _ => 0,
        },
    ];
    let uniform = gpu.uniform(&params);
    let mut counts = gpu.storage("Scope counts", COUNT as u64 * 4)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::Scopes,
        &mut [
            (0, Read(image.buffer())),
            (1, Read(&uniform)),
            (2, Write(counts.write())),
            (3, Read(&region)),
        ],
        [samples[0].div_ceil(16), samples[1].div_ceil(16)],
        &mut trace,
    );
    let bytes = profile.measure("scope count readback", || gpu.readback(&counts))?;
    gpu.finish_trace(&trace, profile)?;
    let counts = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect();
    Ok(Scopes {
        space,
        samples,
        counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_sensor_planes_have_one_scope_channel() {
        for (space, expected) in [
            (Space::SensorBlue, 1),
            (Space::CorrectedSensorBlue, 1),
            (Space::Monochrome, 1),
            (Space::SignedDetail, 1),
            (Space::SensorRgb, 3),
            (Space::CorrectedSensorRgb, 3),
            (Space::ToneMappedRgb(crate::image::ColorSpace::Srgb), 3),
        ] {
            let scopes = Scopes {
                space,
                samples: [1, 1],
                counts: Vec::new(),
            };
            assert_eq!(scopes.channels(), expected, "{space:?}");
        }
    }
}
