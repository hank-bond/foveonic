//! Pools corrected numerical signals with complete edge cells and categorical provenance.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Signal {
    Rgb,
    Blue,
    Mask,
    Flags,
}

/// Independent-noise sigma after a complete d×d area mean. Edge cells can
/// contain fewer native samples. Native correction correlations are excluded.
pub(crate) fn independent_noise_std(scale: crate::render::Scale) -> f32 {
    scale.factor()
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Prepared {
    shape: [u32; 4],
    // divisor, component count, mean/max/union/signed-mean, declared scale
    controls: [u32; 4],
    origin: [u32; 4],
}
impl Prepared {
    /// Means use only samples covered by the final partial cell. Mask coverage
    /// uses maximum and categorical flags use bitwise union.
    pub(crate) fn new(extent: [u32; 2], divisor: u32, signal: Signal) -> Result<Self> {
        if !divisor.is_power_of_two()
            || extent.contains(&0)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
        {
            return Err("invalid corrected-signal pooling geometry".into());
        }
        let logical = extent.map(|v| v.div_ceil(divisor));
        Ok(Self {
            shape: [extent[0], extent[1], logical[0], logical[1]],
            controls: [
                divisor,
                if matches!(signal, Signal::Rgb) { 3 } else { 1 },
                match signal {
                    Signal::Mask => 1,
                    Signal::Flags => 2,
                    _ => 0,
                },
                0,
            ],
            origin: [0, 0, logical[0], logical[1]],
        })
    }
    /// Classic signed codes narrow once after the completed two-dimensional mean.
    pub(crate) fn with_signed_codes(mut self) -> Self {
        self.controls[2] = 3;
        self
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }
    pub(crate) fn with_extent(mut self, extent: [u32; 2]) -> Result<Self> {
        if extent
            .iter()
            .zip(self.logical_extent())
            .any(|(&extent, logical)| extent < logical)
            || u64::from(extent[0]) * u64::from(extent[1])
                > u64::from(u32::MAX) / u64::from(self.controls[1])
        {
            return Err("invalid corrected-signal padding".into());
        }
        self.shape[2..].copy_from_slice(&extent);
        Ok(self)
    }
    pub(crate) fn with_origin(mut self, origin: [u32; 2]) -> Self {
        self.origin[..2].copy_from_slice(&origin);
        self
    }
    pub(crate) fn with_working_scale(mut self, scale: crate::render::Scale) -> Self {
        self.controls[3] = scale.divisor();
        self
    }
    pub(crate) fn working_scale(&self) -> Option<crate::render::Scale> {
        crate::render::Scale::new(self.controls[3]).ok()
    }
    pub(crate) fn logical_extent(&self) -> [u32; 2] {
        [self.origin[2], self.origin[3]]
    }
    pub(crate) fn output_extent(&self) -> [u32; 2] {
        [self.shape[2], self.shape[3]]
    }
    pub(crate) fn largest_cell_area(&self) -> u64 {
        self.extent()
            .into_iter()
            .map(|side| u64::from(side.min(self.controls[0])))
            .product()
    }
    pub(crate) fn valid_output_geometry(&self) -> bool {
        self.origin[..2]
            .iter()
            .zip(self.output_extent())
            .zip(self.logical_extent())
            .all(|((&origin, extent), logical)| {
                origin.checked_add(logical).is_some_and(|end| end <= extent)
            })
    }
    pub(crate) fn map_view(&self, mut view: crate::image::View) -> crate::image::View {
        let scale = crate::render::Scale::new(self.controls[0]).expect("validated pooling divisor");
        view.crop = scale.bounds(view.crop);
        view.extent = scale.extent(view.extent);
        for axis in 0..2 {
            view.crop[axis] += self.origin[axis];
            view.crop[axis + 2] += self.origin[axis];
        }
        view
    }
    pub(crate) fn signal(&self) -> Signal {
        match self.controls[2] {
            1 => Signal::Mask,
            2 => Signal::Flags,
            _ if self.controls[1] == 3 => Signal::Rgb,
            _ => Signal::Blue,
        }
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(self)
    }
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let pixel_bytes = u64::from(prepared.controls[1]) * 4;
    let [width, height] = prepared.extent();
    let [out_width, out_height] = prepared.output_extent();
    if input.size() != u64::from(width) * u64::from(height) * pixel_bytes
        || !prepared.valid_output_geometry()
    {
        return Err("corrected-signal pooling input or origin geometry mismatch".into());
    }
    if prepared.controls[0] == 1
        && [out_width, out_height] == [width, height]
        && prepared.origin[..2] == [0, 0]
    {
        return Ok(input.clone());
    }
    let uniform = gpu.uniform(prepared);
    let mut output = gpu.storage(
        "Pooled working signal",
        u64::from(out_width) * u64::from(out_height) * pixel_bytes,
    )?;
    let [logical_width, logical_height] = prepared.logical_extent();
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::PoolSignal,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        [logical_width.div_ceil(8), logical_height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod tests;
