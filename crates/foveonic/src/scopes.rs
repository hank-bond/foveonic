//! Samples the current floating GPU buffer into compact histogram and waveform counts.

use crate::{
    gpu::{Gpu, Kernel},
    image::{RenderedImage, Space},
    profile::Profile,
    source::Result,
};

pub const BINS: usize = 256;
const DENSITY_OFFSET: usize = 4 * BINS;
const OUTSIDE_OFFSET: usize = DENSITY_OFFSET + 4 * BINS * BINS;
const COUNT: usize = OUTSIDE_OFFSET + 12;

/// Counts describe an oriented, aspect-preserving sample grid.
/// RGB values remain in the buffer's working primaries and transfer state. Sensor codes use 0–4095.
pub struct Scopes {
    pub space: Space,
    pub samples: [u32; 2],
    pub counts: Vec<u32>,
}
impl Scopes {
    pub fn channels(&self) -> usize {
        if self.space == Space::ScalarField {
            1
        } else {
            3
        }
    }
    pub fn histogram(&self, channel: usize) -> &[u32] {
        &self.counts[channel * BINS..(channel + 1) * BINS]
    }
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

pub(crate) fn analyze(gpu: &Gpu, image: &RenderedImage, profile: &mut Profile) -> Result<Scopes> {
    let mut params = image.params();
    let [w, h] = image.view().extent;
    let factor = (512. / w as f32).min(512. / h as f32).min(1.);
    let samples = [
        (w as f32 * factor).round().max(1.) as u32,
        (h as f32 * factor).round().max(1.) as u32,
    ];
    let space = image.description().space;
    params.output = [
        samples[0],
        samples[1],
        u32::from(matches!(
            space,
            Space::SensorLayers | Space::CalibratedLayers
        )),
        u32::from(matches!(
            space,
            Space::LinearRgb(crate::image::ColorSpace::AdobeRgb)
                | Space::ToneMappedRgb(crate::image::ColorSpace::AdobeRgb)
        )),
    ];
    let uniform = gpu.uniform(&params);
    let counts = gpu.storage("Scope counts", COUNT as u64 * 4)?;
    let mut trace = gpu.trace(profile.enabled, 1);
    gpu.dispatch(
        Kernel::Scopes,
        &[(0, image.buffer()), (1, &uniform), (2, &counts)],
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
