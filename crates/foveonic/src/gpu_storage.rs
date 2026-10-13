//! Guards GPU buffer contents with shared read ownership, exclusive writes and allocation generations.

use std::{
    fmt,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

#[cfg(test)]
#[path = "gpu_storage_test.rs"]
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

/// A claim on this allocation's current contents. Cloning shares the claim,
/// never an untracked raw handle. Borrowing it prevents exclusive writes.
#[derive(Clone, Debug)]
pub(crate) struct ReadGuard(Arc<Allocation>);

/// Exclusive permission to modify storage. Cannot be cloned or outlive the
/// mutable owner borrow; dispatch consumes the binding before returning.
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
    pub fn readers(&self) -> usize {
        Arc::strong_count(&self.0)
    }
    pub fn exclusive(&self) -> bool {
        self.readers() == 1
    }

    /// The mutable borrow excludes borrowed readers; Arc ownership excludes
    /// independently retained graph/cache/image guards. Failure is a bug, not
    /// an invitation to silently allocate and hide an invalid write.
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
    // Only the parent GPU module can unwrap raw handles. In particular, there
    // is no Deref/AsRef/escape hatch available to processing code.
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
