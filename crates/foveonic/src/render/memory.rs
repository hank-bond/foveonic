//! Inventories shared processing owners and trims optional history under one caller-set budget.

use super::{Engine, ProcessingWorkingSet, SessionKey};
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
        for source in &other.sources {
            self.add_source(source);
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
    /// Current sensor/edit inputs, decoded source and reserved pools may exceed the budget.
    pub over_budget_bytes: u64,
    pub evicted_sessions: Vec<SessionKey>,
}

impl Engine {
    pub fn memory_resources(
        &self,
        working: &ProcessingWorkingSet,
        reservation: &MemoryReservation,
    ) -> MemoryResources {
        let mut retained = RetainedBytes::default();
        let containers = working.account_memory(&mut retained);
        self.account_preparation(&mut retained);
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
        working: &mut ProcessingWorkingSet,
        limit_bytes: u64,
        reservation: &MemoryReservation,
    ) -> MemoryTrim {
        let _scope = crate::diagnostics::scope(crate::diagnostics::Category::Memory, "memory.trim");
        let mut resources = self.memory_resources(working, reservation);
        let mut evicted_sessions = Vec::new();
        for obsolete_only in [true, false] {
            while resources.total_bytes > limit_bytes {
                let trimmed = working.trim_optional(obsolete_only);
                if !trimmed {
                    break;
                }
                resources = self.memory_resources(working, reservation);
            }
        }
        while resources.total_bytes > limit_bytes {
            let mut protected = Vec::new();
            let mut origins = Vec::new();
            if let Some(metadata) = working.active_metadata() {
                protected.push(metadata);
            }
            if let Some(origin) = working.active_origin() {
                origins.push(origin);
            }
            if !self.trim_cold_preparation(&protected, &origins) {
                let Some(key) = working.evict_oldest() else {
                    break;
                };
                crate::diagnostics::event(
                    crate::diagnostics::Category::Cache,
                    "cache.processing.evicted",
                    |fields| {
                        fields.u64("session", key.photo);
                        fields.str(
                            "kind",
                            match key.kind {
                                super::SessionKind::Native => "native",
                                super::SessionKind::Reduced => "reduced",
                            },
                        );
                        fields.str("reason", "shared_memory_budget");
                        fields.u64("budget_bytes", limit_bytes);
                    },
                );
                evicted_sessions.push(key);
            }
            resources = self.memory_resources(working, reservation);
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
            "memory.budget_bytes",
            limit_bytes as f64,
        );
        crate::diagnostics::counter(
            crate::diagnostics::Category::Memory,
            "memory.over_budget_bytes",
            resources.total_bytes.saturating_sub(limit_bytes) as f64,
        );
        MemoryTrim {
            resources,
            over_budget_bytes: resources.total_bytes.saturating_sub(limit_bytes),
            evicted_sessions,
        }
    }
}

#[cfg(test)]
#[path = "memory_test.rs"]
mod tests;
