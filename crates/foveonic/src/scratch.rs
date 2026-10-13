//! Recycles guarded storage only after the last shared reader releases it on the ordered GPU queue.

use crate::gpu::Gpu;
use crate::source::Result;

#[derive(Default)]
pub(crate) struct Scratch {
    buffers: Vec<crate::gpu::ReadGuard>,
    pub allocations: u32,
    pub reuses: u32,
}

impl Scratch {
    pub fn take(&mut self, gpu: &Gpu, size: u64) -> Result<crate::gpu::ReadGuard> {
        if let Some(index) = self.buffers.iter().position(|buffer| buffer.size() == size) {
            self.reuses += 1;
            Ok(self.buffers.swap_remove(index))
        } else {
            self.allocations += 1;
            gpu.storage("Image scratch", size)
        }
    }

    /// The caller has submitted its last read. Shared guards (including cache
    /// entries and retained images) prevent recycling even if graph liveness
    /// bookkeeping mistakenly releases a value early. Scratch consumers must
    /// fully overwrite storage, never depend on its previous contents/zeroes.
    pub fn release(&mut self, buffer: crate::gpu::ReadGuard) {
        if buffer.exclusive() {
            self.buffers.push(buffer);
        }
    }
}
