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

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Prepared {
    shape: [u32; 4],
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
    let [width, height, out_width, out_height] = prepared.shape;
    let [logical_width, logical_height] = prepared.logical_extent();
    if input.size() != u64::from(width) * u64::from(height) * pixel_bytes
        || prepared.origin[..2]
            .iter()
            .zip(prepared.output_extent())
            .zip(prepared.logical_extent())
            .any(|((&origin, extent), logical)| {
                origin.checked_add(logical).is_none_or(|end| end > extent)
            })
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
    let mut trace = gpu.trace(profile.mode.gpu(), 3);
    let logical = if prepared.controls[0] == 1 {
        input.clone()
    } else {
        let mut horizontal = gpu.storage(
            "Horizontal pooled signal",
            u64::from(logical_width) * u64::from(height) * pixel_bytes,
        )?;
        let mut logical = gpu.storage(
            "Logical pooled signal",
            u64::from(logical_width) * u64::from(logical_height) * pixel_bytes,
        )?;
        gpu.dispatch(
            Kernel::PoolSignalHorizontal,
            &mut [
                (0, Read(input)),
                (1, Read(&uniform)),
                (2, Write(horizontal.write())),
            ],
            [logical_width.div_ceil(8), height.div_ceil(8)],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::PoolSignalVertical,
            &mut [
                (0, Read(&horizontal)),
                (1, Read(&uniform)),
                (2, Write(logical.write())),
            ],
            [logical_width.div_ceil(8), logical_height.div_ceil(8)],
            &mut trace,
        );
        logical
    };
    let result = if [logical_width, logical_height] == [out_width, out_height]
        && prepared.origin[..2] == [0, 0]
    {
        logical
    } else {
        let mut output = gpu.storage(
            "Padded pooled signal",
            u64::from(out_width) * u64::from(out_height) * pixel_bytes,
        )?;
        gpu.dispatch(
            Kernel::PoolSignalPad,
            &mut [
                (0, Read(&logical)),
                (1, Read(&uniform)),
                (2, Write(output.write())),
            ],
            [out_width.div_ceil(8), out_height.div_ceil(8)],
            &mut trace,
        );
        output
    };
    gpu.finish_trace(&trace, profile)?;
    Ok(result)
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod tests;
