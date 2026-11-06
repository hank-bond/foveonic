//! Inventories shared processing owners and trims optional history under one caller-set budget.

use super::{Engine, ReducedWorkingSet, RenderSession};
use crate::recipe::RetainedBytes;
use std::sync::Arc;

/// A snapshot of frontend-retained pixels and metadata. Shared handles enter the
/// same inventory as processing owners; constructing this snapshot copies no pixels.
#[derive(Clone, Default)]
pub struct MemoryReservation {
    buffers: Vec<crate::gpu::ReadGuard>,
    metadata: Vec<Arc<crate::source::Metadata>>,
    /// Other uniquely counted pools, such as display textures and browser thumbnails.
    pub exclusive_bytes: u64,
}
impl MemoryReservation {
    /// Merge shared owners from another current snapshot. Exclusive pools need
    /// their caller's own identity-based inventory and are deliberately omitted.
    pub fn extend_shared(&mut self, other: &Self) {
        for buffer in &other.buffers {
            if !self
                .buffers
                .iter()
                .any(|prior| prior.shares_storage(buffer))
            {
                self.buffers.push(buffer.clone());
            }
        }
        for metadata in &other.metadata {
            self.add_metadata(metadata);
        }
    }
    pub fn add_image(&mut self, image: &crate::image::RenderedImage) {
        if !self
            .buffers
            .iter()
            .any(|prior| prior.shares_storage(&image.buffer))
        {
            self.buffers.push(image.buffer.clone());
        }
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
    /// Current sensor/edit inputs, decoded source and reserved pools may exceed the budget.
    pub over_budget_bytes: u64,
    pub evicted_photos: Vec<u64>,
}

impl Engine {
    pub fn memory_resources(
        &self,
        native: Option<&RenderSession>,
        reduced: &ReducedWorkingSet,
        reservation: &MemoryReservation,
    ) -> MemoryResources {
        let mut retained = RetainedBytes::default();
        let containers = native.map_or(0, |session| session.account_memory(&mut retained))
            + reduced.account_memory(&mut retained);
        self.account_preparation(&mut retained);
        for metadata in &reservation.metadata {
            retained.owner(metadata.clone(), metadata.retained_bytes() as usize);
        }
        let mut buffers = Vec::new();
        for buffer in native
            .into_iter()
            .flat_map(|session| session.cache.buffers())
            .chain(reduced.memory_buffers())
            .chain(reservation.buffers.iter())
        {
            if !buffers
                .iter()
                .any(|prior: &&crate::gpu::ReadGuard| prior.shares_storage(buffer))
            {
                buffers.push(buffer);
            }
        }
        let cpu_bytes = containers.saturating_add(retained.bytes() as u64);
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

    /// Trim stale alternatives before current optional frontiers, cold preparation,
    /// and inactive photos. Active sensor and successful edit dependencies stay protected.
    pub fn trim_memory(
        &self,
        mut native: Option<&mut RenderSession>,
        reduced: &mut ReducedWorkingSet,
        limit_bytes: u64,
        reservation: &MemoryReservation,
    ) -> MemoryTrim {
        let mut resources = self.memory_resources(native.as_deref(), reduced, reservation);
        let mut evicted_photos = Vec::new();
        for obsolete_only in [true, false] {
            while resources.total_bytes > limit_bytes {
                let trimmed = reduced.trim_optional(obsolete_only)
                    || native
                        .as_deref_mut()
                        .is_some_and(|session| session.cache.trim_active_optional(obsolete_only));
                if !trimmed {
                    break;
                }
                resources = self.memory_resources(native.as_deref(), reduced, reservation);
            }
        }
        while resources.total_bytes > limit_bytes {
            let mut protected = Vec::new();
            let mut origins = Vec::new();
            if let Some(session) = native.as_deref() {
                protected.push(session.metadata());
                origins.push(session.source().origin());
            }
            if let Some(metadata) = reduced.active_metadata() {
                protected.push(metadata);
            }
            if let Some(origin) = reduced.active_origin() {
                origins.push(origin);
            }
            if !self.trim_cold_preparation(&protected, &origins) {
                let Some(id) = reduced.evict_oldest() else {
                    break;
                };
                evicted_photos.push(id);
            }
            resources = self.memory_resources(native.as_deref(), reduced, reservation);
        }
        MemoryTrim {
            resources,
            over_budget_bytes: resources.total_bytes.saturating_sub(limit_bytes),
            evicted_photos,
        }
    }
}

#[cfg(test)]
#[path = "memory_test.rs"]
mod tests;
