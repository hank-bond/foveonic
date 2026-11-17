//! Recycles guarded storage only after the last shared reader releases it on the ordered GPU queue.

use crate::gpu::Gpu;
use crate::source::Result;
use std::collections::HashSet;

#[derive(Default)]
pub(crate) struct Scratch {
    buffers: Vec<crate::gpu::ReadGuard>,
    pub allocations: u32,
    pub reuses: u32,
}

impl Scratch {
    /// GPU storage is inventoried separately by allocation identity.
    pub(crate) fn account(&self, retained: &mut crate::recipe::RetainedBytes) {
        retained.add_bytes(self.buffers.capacity() * size_of::<crate::gpu::ReadGuard>());
    }

    pub(crate) fn buffers(&self) -> impl Iterator<Item = &crate::gpu::ReadGuard> {
        self.buffers.iter()
    }
    /// Trim only after GPU completion and retirement of that stage's consumers.
    /// This caps spare storage, not live outputs, checkpoints or total process memory.
    pub fn trim_completed(&mut self, limit_bytes: u64) {
        let mut retained_bytes = 0;
        let mut sizes = HashSet::new();
        self.buffers.retain(|buffer| {
            let size = buffer.size();
            if size > limit_bytes.saturating_sub(retained_bytes) || !sizes.insert(size) {
                return false;
            }
            retained_bytes += size;
            true
        });
    }

    pub fn take(&mut self, gpu: &Gpu, size: u64) -> Result<crate::gpu::ReadGuard> {
        if let Some(index) = self.buffers.iter().position(|buffer| buffer.size() == size) {
            self.reuses += 1;
            Ok(self.buffers.swap_remove(index))
        } else {
            self.allocations += 1;
            gpu.storage("Image scratch", size)
        }
    }

    /// Call only after submitting the last read. Retained images and cache entries
    /// still block reuse, even if graph bookkeeping releases a value too early.
    /// A new user must overwrite the whole buffer because its old contents
    /// are not guaranteed to be zero.
    pub fn release(&mut self, buffer: crate::gpu::ReadGuard) {
        if buffer.exclusive() {
            self.buffers.push(buffer);
        }
    }
}

#[cfg(test)]
#[path = "scratch_test.rs"]
mod tests;
