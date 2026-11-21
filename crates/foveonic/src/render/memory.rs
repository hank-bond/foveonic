//! Inventories shared processing owners and trims retained memory back to one caller-set target.

use super::{working_set::Trimmed, Engine, PhotoKey, WorkingSet};
use crate::recipe::RetainedBytes;
use std::sync::Arc;

/// A snapshot of frontend-retained pixels and metadata. Shared handles enter the
/// same inventory as processing owners; constructing this snapshot copies no pixels.
#[derive(Clone, Default)]
pub struct MemoryReservation {
    buffers: Vec<crate::gpu::ReadGuard>,
    metadata: Vec<Arc<crate::source::Metadata>>,
    sources: Vec<Arc<crate::source::Source>>,
    /// Other uniquely counted pools, such as display textures and browser thumbnails.
    pub exclusive_bytes: u64,
}
impl MemoryReservation {
    pub(super) fn add_buffer(&mut self, buffer: &crate::gpu::ReadGuard) {
        if !self
            .buffers
            .iter()
            .any(|prior| prior.shares_storage(buffer))
        {
            self.buffers.push(buffer.clone());
        }
    }
    /// Merge shared owners from another current snapshot. Exclusive pools need
    /// their caller's own identity-based inventory and are deliberately omitted.
    pub fn extend_shared(&mut self, other: &Self) {
        for buffer in &other.buffers {
            self.add_buffer(buffer);
        }
        for metadata in &other.metadata {
            self.add_metadata(metadata);
        }
        for source in &other.sources {
            self.add_source(source);
        }
    }
    /// Add a separate owner's memory: shared storage counts once and its
    /// exclusive bytes add.
    pub fn merge(&mut self, other: &Self) {
        self.extend_shared(other);
        self.exclusive_bytes = self.exclusive_bytes.saturating_add(other.exclusive_bytes);
    }
    pub fn add_image(&mut self, image: &crate::image::RenderedImage) {
        self.add_buffer(&image.buffer);
        self.add_metadata(&image.metadata);
    }
    pub fn add_metadata(&mut self, metadata: &Arc<crate::source::Metadata>) {
        if !self
            .metadata
            .iter()
            .any(|prior| Arc::ptr_eq(prior, metadata))
        {
            self.metadata.push(metadata.clone());
        }
    }
    /// Reserve shared decoded samples once, including their independently shared metadata.
    pub fn add_source(&mut self, source: &Arc<crate::source::Source>) {
        if !self.sources.iter().any(|prior| Arc::ptr_eq(prior, source)) {
            self.sources.push(source.clone());
        }
        self.add_metadata(&source.metadata);
    }
}

/// Logical retained processing allocations plus the caller's other retained pools.
/// Driver, allocator and temporary rendering overhead need separate headroom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct MemoryResources {
    pub cpu_bytes: u64,
    pub gpu_bytes: u64,
    /// Exclusive caller-owned pools; shared reserved images are included in CPU/GPU totals.
    pub reserved_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct MemoryTrim {
    pub resources: MemoryResources,
    /// Visible photos, sensor data and reserved pools may exceed the target.
    pub over_target_bytes: u64,
    pub evicted_photos: Vec<PhotoKey>,
}

impl Engine {
    pub fn memory_resources(
        &self,
        working: &WorkingSet,
        reservation: &MemoryReservation,
    ) -> MemoryResources {
        self.memory_resources_with(working, reservation, |_| {})
    }

    pub(super) fn memory_resources_with(
        &self,
        working: &WorkingSet,
        reservation: &MemoryReservation,
        account: impl FnOnce(&mut RetainedBytes),
    ) -> MemoryResources {
        let mut retained = RetainedBytes::default();
        let containers = working.account_memory(&mut retained);
        self.account_preparation(&mut retained);
        if let Some(calibration) = &self.noise_calibration {
            retained.owner(calibration.clone(), calibration.retained_bytes());
        }
        account(&mut retained);
        for metadata in &reservation.metadata {
            retained.owner(metadata.clone(), metadata.retained_bytes() as usize);
        }
        for source in &reservation.sources {
            retained.owner(source.clone(), source.retained_source_bytes() as usize);
        }
        let mut buffers = Vec::new();
        for buffer in working.memory_buffers().chain(reservation.buffers.iter()) {
            if !buffers
                .iter()
                .any(|prior: &&crate::gpu::ReadGuard| prior.shares_storage(buffer))
            {
                buffers.push(buffer);
            }
        }
        let buffer_owners: u64 = buffers
            .iter()
            .map(|buffer| buffer.owned_cpu_bytes() as u64)
            .sum();
        let cpu_bytes = containers
            .saturating_add(retained.bytes() as u64)
            .saturating_add(buffer_owners);
        let gpu_bytes = buffers.into_iter().map(|buffer| buffer.size()).sum();
        MemoryResources {
            cpu_bytes,
            gpu_bytes,
            reserved_bytes: reservation.exclusive_bytes,
            total_bytes: cpu_bytes
                .saturating_add(gpu_bytes)
                .saturating_add(reservation.exclusive_bytes),
        }
    }

    /// Trim obsolete, then optional checkpoints and preparation no retained photo
    /// owns. Then drop whatever was used longest ago: one edit's restart inputs or a
    /// whole working photo. Eviction-protected and guarded photos are never evicted. When they
    /// alone exceed the target, the overage is reported instead of rebuilding them
    /// after every render.
    pub fn trim_memory(
        &self,
        working: &mut WorkingSet,
        target_bytes: u64,
        reservation: &MemoryReservation,
    ) -> MemoryTrim {
        self.trim_memory_with(working, target_bytes, reservation, |_| {})
    }

    pub(super) fn trim_memory_with(
        &self,
        working: &mut WorkingSet,
        target_bytes: u64,
        reservation: &MemoryReservation,
        account: impl Fn(&mut RetainedBytes),
    ) -> MemoryTrim {
        let _scope = crate::diagnostics::scope(crate::diagnostics::Category::Memory, "memory.trim");
        let mut resources = self.memory_resources_with(working, reservation, &account);
        for obsolete_only in [true, false] {
            while resources.total_bytes > target_bytes {
                if !working.trim_optional(obsolete_only) {
                    break;
                }
                resources = self.memory_resources_with(working, reservation, &account);
            }
        }
        let mut evicted_photos = Vec::new();
        while resources.total_bytes > target_bytes {
            let (retained, origins): (Vec<_>, Vec<_>) = working.retained_inputs().unzip();
            if !self.trim_cold_preparation(&retained, &origins) {
                match working.trim_least_recent() {
                    None => break,
                    Some(Trimmed::RestartInput) => {}
                    Some(Trimmed::Photo(key)) => {
                        crate::diagnostics::event(
                            crate::diagnostics::Category::Cache,
                            "cache.processing.evicted",
                            |fields| {
                                fields.u64("session", key.photo);
                                fields.str(
                                    "kind",
                                    match key.resolution {
                                        super::Resolution::Native => "native",
                                        super::Resolution::Reduced => "reduced",
                                    },
                                );
                                fields.str("reason", "memory_target");
                                fields.u64("target_bytes", target_bytes);
                            },
                        );
                        evicted_photos.push(key);
                    }
                }
            }
            resources = self.memory_resources_with(working, reservation, &account);
        }
        crate::diagnostics::counter(
            crate::diagnostics::Category::Memory,
            "memory.retained_bytes",
            resources.total_bytes as f64,
        );
        crate::diagnostics::counter(
            crate::diagnostics::Category::Memory,
            "memory.gpu_bytes",
            resources.gpu_bytes as f64,
        );
        crate::diagnostics::counter(
            crate::diagnostics::Category::Memory,
            "memory.cpu_bytes",
            resources.cpu_bytes as f64,
        );
        crate::diagnostics::counter(
            crate::diagnostics::Category::Memory,
            "memory.reserved_bytes",
            resources.reserved_bytes as f64,
        );
        crate::diagnostics::counter(
            crate::diagnostics::Category::Memory,
            "memory.target_bytes",
            target_bytes as f64,
        );
        crate::diagnostics::counter(
            crate::diagnostics::Category::Memory,
            "memory.over_target_bytes",
            resources.total_bytes.saturating_sub(target_bytes) as f64,
        );
        MemoryTrim {
            resources,
            over_target_bytes: resources.total_bytes.saturating_sub(target_bytes),
            evicted_photos,
        }
    }
}

#[cfg(test)]
#[path = "memory_test.rs"]
mod tests;
