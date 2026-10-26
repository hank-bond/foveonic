//! Prevents a later GPU write from changing an image that still has readers.

use std::{
    fmt,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

#[cfg(test)]
#[path = "storage_test.rs"]
mod tests;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct Allocation {
    buffer: wgpu::Buffer,
    id: u64,
    generation: u64,
    label: String,
    last_writer: &'static str,
}
impl fmt::Debug for Allocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Allocation")
            .field("id", &self.id)
            .field("generation", &self.generation)
            .field("label", &self.label)
            .field("last_writer", &self.last_writer)
            .field("bytes", &self.buffer.size())
            .finish()
    }
}

/// Keep the current contents alive while a caller reads them. Clones keep
/// the same read claim, so none of them can be bypassed by a later write.
#[derive(Clone, Debug)]
pub(crate) struct ReadGuard(Arc<Allocation>);

/// A write needs sole access so retained images cannot change underneath readers.
/// This guard cannot be cloned or outlive the mutable borrow. Dispatch submits
/// its commands while the borrow is still held.
pub(crate) struct WriteGuard<'a>(&'a mut Allocation);

pub(crate) enum Binding<'a> {
    Read(&'a ReadGuard),
    Write(WriteGuard<'a>),
}

impl ReadGuard {
    pub(super) fn new(buffer: wgpu::Buffer, label: &str) -> Self {
        Self(Arc::new(Allocation {
            buffer,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            generation: 0,
            label: label.into(),
            last_writer: "allocation/upload",
        }))
    }
    pub fn size(&self) -> u64 {
        self.0.buffer.size()
    }
    #[cfg(test)]
    pub fn identity(&self) -> (u64, u64) {
        (self.0.id, self.0.generation)
    }
    /// Aliases retain one allocation and consume the cache budget only once.
    pub(crate) fn shares_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn readers(&self) -> usize {
        Arc::strong_count(&self.0)
    }
    pub fn exclusive(&self) -> bool {
        self.readers() == 1
    }

    /// Check both borrowed readers and separately retained images or cache entries.
    /// A failed check means an invalid write was attempted. Allocating another
    /// buffer here would hide the ownership bug rather than fix it.
    pub fn write(&mut self) -> WriteGuard<'_> {
        assert!(
            self.exclusive(),
            "GPU write with retained readers: {:?}; owners={}",
            self.0,
            self.readers()
        );
        let allocation = Arc::get_mut(&mut self.0).expect("exclusive GPU allocation");
        allocation.generation = allocation
            .generation
            .checked_add(1)
            .expect("GPU generation overflow");
        WriteGuard(allocation)
    }
    // Raw handles would let processing code bypass these ownership checks.
    // Keep access inside the GPU module rather than exposing Deref or AsRef.
    pub(super) fn raw(&self) -> &wgpu::Buffer {
        &self.0.buffer
    }
}
impl WriteGuard<'_> {
    pub(super) fn raw(&self) -> &wgpu::Buffer {
        &self.0.buffer
    }
    pub(super) fn submitted(&mut self, stage: &'static str) {
        self.0.last_writer = stage;
    }
}
impl Binding<'_> {
    pub(super) fn raw(&self) -> &wgpu::Buffer {
        match self {
            Self::Read(guard) => guard.raw(),
            Self::Write(guard) => guard.raw(),
        }
    }
    pub(super) fn submitted(&mut self, stage: &'static str) {
        if let Self::Write(guard) = self {
            guard.submitted(stage);
        }
    }
}
