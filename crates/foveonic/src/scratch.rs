//! Reuses exclusively owned, fully overwritten buffers within one ordered graph execution.

use crate::gpu::Gpu;
use crate::source::Result;

#[derive(Default)]
pub(crate) struct Scratch {
    buffers: Vec<wgpu::Buffer>,
    pub allocations: u32,
    pub reuses: u32,
}

impl Scratch {
    pub fn take(&mut self, gpu: &Gpu, size: u64) -> Result<wgpu::Buffer> {
        if let Some(index) = self.buffers.iter().position(|buffer| buffer.size() == size) {
            self.reuses += 1;
            Ok(self.buffers.swap_remove(index))
        } else {
            self.allocations += 1;
            gpu.storage("Image scratch", size)
        }
    }

    /// Only release internal storage after its final read has been submitted. Submissions use
    /// the same queue, so a later overwrite follows those reads without a per-node CPU fence.
    /// Imported images, retained outputs, and partially written/atomic buffers are ineligible.
    pub fn release(&mut self, buffer: wgpu::Buffer) {
        self.buffers.push(buffer);
    }
}
