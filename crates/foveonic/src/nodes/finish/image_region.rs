//! Copies a rectangular numerical signal on the GPU while preserving every component bit.

use crate::{
    gpu::{Gpu, ReadGuard},
    recipe::Key,
    source::Result,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Prepared {
    source: [u32; 2],
    rect: [u32; 4],
    components: u32,
    reserved: u32,
}
impl Prepared {
    pub(crate) fn new(source: [u32; 2], rect: [u32; 4], components: u32) -> Result<Self> {
        if rect[0] >= rect[2]
            || rect[1] >= rect[3]
            || rect[2] > source[0]
            || rect[3] > source[1]
            || ![1, 3].contains(&components)
            || u64::from(source[0]) * u64::from(source[1])
                > u64::from(u32::MAX) / u64::from(components)
        {
            return Err("invalid numerical region geometry".into());
        }
        Ok(Self {
            source,
            rect,
            components,
            reserved: 0,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.rect[2] - self.rect[0], self.rect[3] - self.rect[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(self)
    }
}
pub(crate) fn apply(gpu: &Gpu, input: &ReadGuard, prepared: &Prepared) -> Result<ReadGuard> {
    let pixel_bytes = u64::from(prepared.components) * std::mem::size_of::<f32>() as u64;
    if input.size() != u64::from(prepared.source[0]) * u64::from(prepared.source[1]) * pixel_bytes {
        return Err("numerical region input geometry mismatch".into());
    }
    let [width, height] = prepared.extent();
    let row_bytes = u64::from(width) * pixel_bytes;
    let mut output = gpu.storage("Regional numerical signal", row_bytes * u64::from(height))?;
    gpu.copy_ranges(
        input,
        output.write(),
        (0..height).map(|y| {
            [
                (u64::from(y + prepared.rect[1]) * u64::from(prepared.source[0])
                    + u64::from(prepared.rect[0]))
                    * pixel_bytes,
                u64::from(y) * row_bytes,
                row_bytes,
            ]
        }),
    );
    Ok(output)
}
