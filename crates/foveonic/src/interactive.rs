//! Reuses active-edit GPU inputs and bounded repair preparation while preserving earlier images.

use crate::{
    diagnostics::{self, Category},
    graph::{DataType, Graph},
    recipe::Key,
    render::{Settings, Value},
    source::Source,
};
use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

/// Keep the original photo and the inputs needed to restart the active edit.
/// Retaining every stage would spend memory without helping that edit.
pub struct RenderSession {
    pub(crate) source: Arc<Source>,
    pub(crate) cache: Cache,
    pub(crate) last_settings: Option<Settings>,
}
impl RenderSession {
    /// Read-only scalar checkpoint inventory, intended for use outside timed rendering.
    pub fn checkpoints(&self) -> crate::profile::CacheSnapshot {
        let resources = self.resources();
        crate::profile::CacheSnapshot {
            entries: self.cache.checkpoints(),
            versions: self.cache.checkpoint_versions(),
            cache_structure_bytes: resources.cache_structure_bytes,
            parameter_payload_bytes: resources.parameter_payload_bytes,
            preparation_bytes: resources.preparation_bytes,
            output_parameters: resources.output_parameters,
        }
    }
    /// Inspect all retained processing owners outside timed interactions.
    pub fn resources(&self) -> crate::render::SessionResources {
        let mut retained = crate::recipe::RetainedBytes::default();
        let containers = self.cache.account_keys(&mut retained);
        let payloads = retained.payload_bytes();
        crate::render::SessionResources {
            source_bytes: self.source.retained_source_bytes(),
            metadata_bytes: self.source.metadata.retained_bytes(),
            owner_bytes: self.owner_bytes(),
            cache_gpu_bytes: self.cached_bytes(),
            cache_structure_bytes: (containers + retained.bytes() - payloads) as u64,
            parameter_payload_bytes: payloads as u64,
            preparation_bytes: self.cached_preparation_bytes() as u64,
            output_parameters: self.cache.parameter_resources(),
        }
    }
    pub fn retained_cpu_bytes(&self) -> u64 {
        let mut retained = crate::recipe::RetainedBytes::default();
        let containers = self.cache.account_keys(&mut retained);
        self.source.retained_source_bytes()
            + self.source.metadata.retained_bytes()
            + self.owner_bytes()
            + self.cached_preparation_bytes() as u64
            + containers as u64
            + retained.bytes() as u64
    }
    fn owner_bytes(&self) -> u64 {
        (std::mem::size_of::<Self>()
            + self.last_settings.as_ref().map_or(0, |settings| {
                settings.white_balance.capacity() + settings.color_mode.capacity()
            })) as u64
    }
    pub(crate) fn account_memory(&self, retained: &mut crate::recipe::RetainedBytes) -> u64 {
        retained.owner(
            self.source.clone(),
            self.source.retained_source_bytes() as usize,
        );
        retained.owner(
            self.source.metadata.clone(),
            self.source.metadata.retained_bytes() as usize,
        );
        if let Some(repair) = &self.cache.repair {
            retained.owner(repair.prepared.clone(), repair.bytes);
        }
        self.owner_bytes() + self.cache.account_keys(retained) as u64
    }
    pub fn new(source: impl Into<Arc<Source>>) -> Self {
        Self {
            source: source.into(),
            cache: Cache::default(),
            last_settings: None,
        }
    }
    /// Exports need the original samples, not the cached result of a slider edit.
    pub fn source(&self) -> &Source {
        &self.source
    }
    /// Borrow the immutable decoded input without moving the active native session.
    pub fn shared_source(&self) -> Arc<Source> {
        self.source.clone()
    }

    /// Release native editing checkpoints while transferring decoded-source ownership.
    pub fn into_source(self) -> Arc<Source> {
        self.source
    }

    pub fn metadata(&self) -> &Arc<crate::source::Metadata> {
        &self.source.metadata
    }
    pub fn cached_bytes(&self) -> u64 {
        self.cache.bytes()
    }
    /// Logical CPU bytes retained for prepared repair, excluding allocator overhead.
    pub fn cached_preparation_bytes(&self) -> usize {
        self.cache.repair.as_ref().map_or(0, |entry| entry.bytes)
    }
    /// These identities help replace pending requests. Processing stages still
    /// decide which parameter changes invalidate their own results.
    pub fn changed_parameters(&self, settings: &Settings) -> Vec<&'static str> {
        self.last_settings
            .as_ref()
            .map_or_else(Vec::new, |old| old.changed_parameters(settings))
    }
}
impl Settings {
    pub fn changed_parameters(&self, next: &Self) -> Vec<&'static str> {
        let mut changes = Vec::new();
        macro_rules! changed {
            ($test:expr,$name:expr) => {
                if $test {
                    changes.push($name);
                }
            };
        }
        changed!(self.dehaze.amount != next.dehaze.amount, "dehaze-amount");
        changed!(self.dehaze.shadow != next.dehaze.shadow, "dehaze-shadow");
        changed!(self.working_scale != next.working_scale, "working-scale");
        changed!(self.preview_region != next.preview_region, "preview-region");
        changed!(self.rendering != next.rendering, "rendering");
        changed!(self.photographic != next.photographic, "photographic");
        changed!(
            self.adaptive_tone_tuning != next.adaptive_tone_tuning,
            "adaptive-tone-limits"
        );
        changed!(self.monochrome.mix != next.monochrome.mix, "mono-mix");
        changed!(
            self.monochrome.detail_enabled != next.monochrome.detail_enabled
                || self.monochrome.texture != next.monochrome.texture,
            "mono-detail"
        );
        changed!(self.monochrome.noise != next.monochrome.noise, "mono-noise");
        changed!(
            self.monochrome_smoothness != next.monochrome_smoothness,
            "mono-smoothness"
        );
        changed!(
            self.monochrome.banding != next.monochrome.banding,
            "mono-banding"
        );
        changed!(self.white_balance != next.white_balance, "white-balance");
        changed!(
            self.white_balance_bias != next.white_balance_bias,
            "white-balance-bias"
        );
        changed!(self.color_mode != next.color_mode, "color-mode");
        changed!(self.repair != next.repair, "repair");
        changed!(
            self.highlight_restoration != next.highlight_restoration,
            "highlight-restoration"
        );
        changed!(
            self.highlight_threshold_ev.map(f32::to_bits)
                != next.highlight_threshold_ev.map(f32::to_bits),
            "highlight-threshold"
        );
        changed!(self.highlight_mode != next.highlight_mode, "highlight-mode");
        changed!(
            self.highlight_amount != next.highlight_amount,
            "highlight-amount"
        );
        changed!(self.color_field != next.color_field, "color-field");
        changed!(self.despeckle != next.despeckle, "despeckle");
        changed!(
            self.merrill_speckles != next.merrill_speckles,
            "merrill-speckles"
        );
        changed!(
            self.neutralize_shadows != next.neutralize_shadows,
            "neutralize-shadows"
        );
        changed!(self.row_correction != next.row_correction, "row-correction");
        changed!(
            self.merrill_noise_method != next.merrill_noise_method,
            "merrill-noise-method"
        );
        changed!(
            self.merrill_banding != next.merrill_banding,
            "merrill-banding"
        );
        changed!(
            self.color_dq_gain.map(f32::to_bits) != next.color_dq_gain.map(f32::to_bits),
            "color-dq"
        );
        changed!(
            self.sharpen_amount.map(f32::to_bits) != next.sharpen_amount.map(f32::to_bits),
            "sharpen"
        );
        changed!(
            self.exposure.to_bits() != next.exposure.to_bits()
                || self.exposure_auto != next.exposure_auto
                || self.exposure_auto_compensation != next.exposure_auto_compensation,
            "exposure"
        );
        changed!(
            self.saturation.to_bits() != next.saturation.to_bits(),
            "saturation"
        );
        changed!(
            self.tone.contrast.to_bits() != next.tone.contrast.to_bits(),
            "contrast"
        );
        changed!(
            self.tone.shadows.to_bits() != next.tone.shadows.to_bits(),
            "shadows"
        );
        changed!(
            self.tone.highlights.to_bits() != next.tone.highlights.to_bits(),
            "highlights"
        );
        changed!(
            self.tonal_ranges.blacks != next.tonal_ranges.blacks,
            "tonal-blacks"
        );
        changed!(
            self.tonal_ranges.shadows != next.tonal_ranges.shadows,
            "tonal-shadows"
        );
        changed!(
            self.tonal_ranges.highlights != next.tonal_ranges.highlights,
            "tonal-highlights"
        );
        changed!(
            self.tonal_ranges.whites != next.tonal_ranges.whites,
            "tonal-whites"
        );
        changed!(self.output_size != next.output_size, "output-size");
        changed!(self.rotation != next.rotation, "rotation");
        changed!(
            self.tiff_color_space != next.tiff_color_space,
            "tiff-color-space"
        );
        changed!(self.tone_enabled != next.tone_enabled, "tone-enabled");
        changed!(
            self.saturation_enabled != next.saturation_enabled,
            "saturation-enabled"
        );
        changed!(
            self.color_editor_enabled != next.color_editor_enabled,
            "color-editor-enabled"
        );
        changed!(
            self.sharpening_enabled != next.sharpening_enabled,
            "refinement-enabled"
        );
        match (self.reconstruction, next.reconstruction) {
            (Some(a), Some(b)) => {
                changed!(a.noise_enabled != b.noise_enabled, "noise-enabled");
                changed!(a.detail_enabled != b.detail_enabled, "detail-enabled");
                changed!(a.detail.to_bits() != b.detail.to_bits(), "detail");
                changed!(
                    a.texture.fine_size.to_bits() != b.texture.fine_size.to_bits(),
                    "fine-size"
                );
                changed!(
                    a.texture.coarse_size.to_bits() != b.texture.coarse_size.to_bits(),
                    "coarse-size"
                );
                changed!(
                    a.texture.fine.to_bits() != b.texture.fine.to_bits(),
                    "fine-detail"
                );
                changed!(
                    a.texture.coarse.to_bits() != b.texture.coarse.to_bits(),
                    "coarse-detail"
                );
                changed!(a.chroma != b.chroma, "chroma-noise");
                changed!(a.luminance != b.luminance, "luma-noise");
                changed!(a.banding != b.banding, "banding-noise");
            }
            (None, None) => {}
            _ => changes.push("reconstruction"),
        }
        changes
    }
}

#[derive(PartialEq, Eq)]
struct Recipe {
    operation: std::mem::Discriminant<crate::graph::Operation>,
    parameters: Key,
    data: DataType,
    role: &'static str,
    view: Option<crate::image::View>,
    inputs: Vec<u64>,
}
struct Version {
    recipe: Recipe,
    generation: u64,
    // A retained result keeps its exact ancestry alive without retaining pixels upstream.
    parents: Vec<Arc<Version>>,
}
struct Entry {
    version: Arc<Version>,
    value: Value,
    pinned: bool,
    used: u64,
}
struct RepairSlot {
    threshold: crate::nodes::sensor::repair::Threshold,
    prepared: Arc<crate::nodes::sensor::camera_repair::Prepared>,
    bytes: usize,
}

#[derive(Default)]
struct WorkingSet {
    versions: HashMap<String, Arc<Version>>,
    selected: Vec<String>,
    primary: Vec<String>,
    cuts: HashSet<u64>,
    restart: HashSet<u64>,
}
#[derive(Default)]
pub(crate) struct Cache {
    diagnostic_id: std::sync::OnceLock<u64>,
    owner: Option<Arc<()>>,
    source: Option<crate::source::SourceOrigin>,
    repair: Option<RepairSlot>,
    working: WorkingSet,
    identities: HashMap<u64, Vec<Weak<Version>>>,
    counter: u64,
    frame: u64,
    entries: Vec<Entry>,
    required_cuts: HashSet<u64>,
    active_restart: HashSet<u64>,
}
impl Cache {
    pub(crate) const LIMIT: u64 = 1024 * 1024 * 1024;

    pub(crate) fn diagnostic_id(&self) -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        *self
            .diagnostic_id
            .get_or_init(|| NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    pub(crate) fn diagnostic_version(&self, name: &str) -> Option<u64> {
        self.working
            .versions
            .get(name)
            .map(|version| version.generation)
    }

    pub(crate) fn trace_checkpoints(&self, graph_id: u64) {
        diagnostics::event(Category::Cache, "cache.checkpoint.snapshot", |fields| {
            self.diagnostic_source(fields);
            fields.u64("graph_id", graph_id).str(
                "entries",
                &serde_json::to_string(&self.checkpoints()).expect("scalar checkpoint inventory"),
            );
        });
    }

    fn checkpoint_versions(&self) -> Vec<crate::profile::CheckpointVersion> {
        self.retained_versions()
            .into_iter()
            .map(|version| {
                let mut names: Vec<_> = self
                    .working
                    .versions
                    .iter()
                    .filter(|(_, known)| known.generation == version.generation)
                    .map(|(name, _)| name.clone())
                    .collect();
                names.sort();
                crate::profile::CheckpointVersion {
                    version: version.generation,
                    role: version.recipe.role,
                    names,
                    parents: version
                        .parents
                        .iter()
                        .map(|parent| parent.generation)
                        .collect(),
                }
            })
            .collect()
    }

    fn checkpoints(&self) -> Vec<crate::profile::CheckpointEntry> {
        self.entries
            .iter()
            .map(|entry| {
                let generation = entry.version.generation;
                let mut names: Vec<_> = self
                    .working
                    .versions
                    .iter()
                    .filter(|(_, version)| version.generation == generation)
                    .map(|(name, _)| name.clone())
                    .collect();
                names.sort();
                let selected = names
                    .iter()
                    .any(|name| self.working.selected.contains(name));
                let primary = names.iter().any(|name| self.working.primary.contains(name));
                let (allocation_id, buffer_generation) = entry.value.buffer.identity();
                crate::profile::CheckpointEntry {
                    names,
                    role: entry.version.recipe.role,
                    version: generation,
                    parents: entry
                        .version
                        .parents
                        .iter()
                        .map(|parent| parent.generation)
                        .collect(),
                    allocation_id,
                    generation: buffer_generation,
                    byte_len: entry.value.buffer.size(),
                    working_scale: entry.value.working_scale.divisor(),
                    selected,
                    primary,
                    restart: self.active_restart.contains(&generation)
                        || self.working.restart.contains(&generation),
                    pinned: entry.pinned,
                    required_cut: self.required_cuts.contains(&generation)
                        || self.working.cuts.contains(&generation),
                    priority: self.current_priority(generation),
                    used_frame: entry.used,
                }
            })
            .collect()
    }
    const REPAIR_LIMIT: usize = 4 * 1024 * 1024;

    fn retain_repair(
        &mut self,
        threshold: crate::nodes::sensor::repair::Threshold,
        prepared: &Arc<crate::nodes::sensor::camera_repair::Prepared>,
    ) {
        let bytes = prepared
            .retained_bytes()
            .and_then(|n| n.checked_add(2 * std::mem::size_of::<usize>()))
            .and_then(|n| n.checked_add(std::mem::size_of::<Option<RepairSlot>>()));
        self.repair = bytes
            .filter(|&n| n <= Self::REPAIR_LIMIT)
            .map(|bytes| RepairSlot {
                threshold,
                prepared: prepared.clone(),
                bytes,
            });
    }
    pub(crate) fn bind(&mut self, owner: &Arc<()>) {
        if self
            .owner
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, owner))
        {
            self.observe_invalidation("engine-owner");
            *self = Self {
                owner: Some(owner.clone()),
                ..Default::default()
            };
        }
    }
    /// Source provenance stays alive independently of decoded samples and reload metadata.
    pub(crate) fn bind_source(&mut self, source: &crate::source::SourceOrigin) {
        if self.source.as_ref().is_none_or(|old| old != source) {
            self.observe_invalidation("source-origin");
            let owner = self.owner.clone();
            *self = Self {
                owner,
                ..Default::default()
            };
        }
        self.source = Some(source.clone());
    }
    pub(crate) fn bytes(&self) -> u64 {
        self.entries
            .iter()
            .enumerate()
            .filter(|(i, entry)| {
                !self.entries[..*i]
                    .iter()
                    .any(|prior| prior.value.buffer.shares_storage(&entry.value.buffer))
            })
            .map(|(_, entry)| entry.value.buffer.size())
            .sum()
    }
    fn diagnostic_source(&self, fields: &mut diagnostics::Fields<'_>) {
        fields.u64("cache_id", self.diagnostic_id());
        match &self.source {
            Some(crate::source::SourceOrigin::File(identity)) => {
                fields.str("cache_source", &String::from_utf8_lossy(&identity.location));
            }
            Some(crate::source::SourceOrigin::Memory(_)) => {
                fields.str("cache_source_kind", "memory");
            }
            None => {
                fields.str("cache_source_kind", "unbound");
            }
        }
        fields.u64("cache_frame", self.frame);
    }
    fn observe_invalidation(&self, reason: &'static str) {
        if self.entries.is_empty() && self.working.versions.is_empty() {
            return;
        }
        diagnostics::event(Category::Cache, "cache.checkpoint.invalidated", |fields| {
            self.diagnostic_source(fields);
            fields
                .str("reason", reason)
                .u64("entries", self.entries.len() as u64)
                .u64("cache_gpu_reference_bytes", self.bytes());
        });
    }
    fn observe_checkpoint(
        &self,
        event: &'static str,
        reason: &'static str,
        version: &Version,
        bytes: u64,
        allocation: (u64, u64),
    ) {
        diagnostics::event(Category::Cache, event, |fields| {
            self.diagnostic_source(fields);
            fields
                .str("reason", reason)
                .str("role", version.recipe.role)
                .u64("generation", version.generation)
                .u64("buffer_reference_bytes", bytes)
                .u64("allocation_id", allocation.0)
                .u64("allocation_generation", allocation.1)
                .bool(
                    "selected",
                    self.current_priority(version.generation).is_some(),
                )
                .bool(
                    "sensor_dependency",
                    self.required_cuts.contains(&version.generation)
                        || self.working.cuts.contains(&version.generation),
                )
                .bool(
                    "restart_input",
                    self.active_restart.contains(&version.generation),
                );
        });
    }
    /// Referenced bytes identify the entry, not memory freed: display frames or
    /// another checkpoint may still share the same immutable buffer allocation.
    fn remove_entry(&mut self, index: usize, reason: &'static str) {
        let entry = self.entries.remove(index);
        self.observe_checkpoint(
            "cache.checkpoint.evicted",
            reason,
            &entry.version,
            entry.value.buffer.size(),
            entry.value.buffer.identity(),
        );
    }
    pub(crate) fn account_keys(&self, retained: &mut crate::recipe::RetainedBytes) -> usize {
        let mut bytes = self.entries.capacity() * std::mem::size_of::<Entry>()
            + (self.required_cuts.capacity()
                + self.active_restart.capacity()
                + self.working.cuts.capacity()
                + self.working.restart.capacity())
                * std::mem::size_of::<u64>()
            + self.working.selected.capacity() * std::mem::size_of::<String>()
            + self.working.primary.capacity() * std::mem::size_of::<String>();
        bytes += self
            .working
            .selected
            .iter()
            .chain(&self.working.primary)
            .map(String::capacity)
            .sum::<usize>();
        bytes += match &self.source {
            Some(crate::source::SourceOrigin::File(identity)) => identity.location.capacity(),
            Some(crate::source::SourceOrigin::Memory(_)) => 2 * std::mem::size_of::<usize>(),
            None => 0,
        };
        bytes += self
            .working
            .versions
            .keys()
            .map(|name| name.capacity() + std::mem::size_of::<(String, Arc<Version>)>())
            .sum::<usize>();
        bytes += self.identities.capacity() * std::mem::size_of::<(u64, Vec<Weak<Version>>)>()
            + self
                .identities
                .values()
                .map(|bucket| bucket.capacity() * std::mem::size_of::<Weak<Version>>())
                .sum::<usize>();
        for version in self.retained_versions() {
            bytes += std::mem::size_of::<Version>()
                + 2 * std::mem::size_of::<usize>()
                + version.parents.capacity() * std::mem::size_of::<Arc<Version>>()
                + version.recipe.inputs.capacity() * std::mem::size_of::<u64>();
            version.recipe.parameters.account(retained);
        }
        bytes
    }
    fn retained_versions(&self) -> Vec<Arc<Version>> {
        let mut seen = HashSet::new();
        let mut pending: Vec<_> = self
            .working
            .versions
            .values()
            .cloned()
            .chain(self.entries.iter().map(|entry| entry.version.clone()))
            .collect();
        let mut versions = Vec::new();
        while let Some(version) = pending.pop() {
            if seen.insert(Arc::as_ptr(&version)) {
                pending.extend(version.parents.iter().cloned());
                versions.push(version);
            }
        }
        versions.sort_by_key(|version| version.generation);
        versions
    }
    pub(crate) fn parameter_resources(&self) -> Vec<crate::render::ParameterResources> {
        let mut retained = crate::recipe::RetainedBytes::default();
        let mut roles = std::collections::BTreeMap::new();
        for version in self.retained_versions() {
            let before = retained.payload_bytes();
            version.recipe.parameters.account(&mut retained);
            let role =
                roles
                    .entry(version.recipe.role)
                    .or_insert(crate::render::ParameterResources {
                        output: version.recipe.role,
                        versions: 0,
                        payload_bytes: 0,
                    });
            role.versions += 1;
            role.payload_bytes += (retained.payload_bytes() - before) as u64;
        }
        roles.into_values().collect()
    }
    pub(crate) fn buffers(&self) -> impl Iterator<Item = &crate::gpu::ReadGuard> {
        self.entries.iter().map(|entry| &entry.value.buffer)
    }
    #[cfg(test)]
    pub(crate) fn sealed_buffers(&self) -> impl Iterator<Item = &crate::gpu::ReadGuard> {
        self.entries
            .iter()
            .filter(|entry| entry.pinned)
            .map(|entry| &entry.value.buffer)
    }
    fn room_for(&self, value: &Value) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.value.buffer.shares_storage(&value.buffer))
            || self.bytes() + value.buffer.size() <= Self::LIMIT
    }
    fn intern(&mut self, recipe: Recipe, parents: Vec<Arc<Version>>) -> Arc<Version> {
        let mut hasher = DefaultHasher::new();
        recipe.operation.hash(&mut hasher);
        std::mem::discriminant(&recipe.data).hash(&mut hasher);
        recipe.role.hash(&mut hasher);
        recipe.view.hash(&mut hasher);
        recipe.inputs.hash(&mut hasher);
        let bucket = self.identities.entry(hasher.finish()).or_default();
        bucket.retain(|identity| identity.strong_count() != 0);
        if let Some(existing) = bucket
            .iter()
            .filter_map(Weak::upgrade)
            .find(|existing| existing.recipe == recipe)
        {
            return existing;
        }
        self.counter += 1;
        let version = Arc::new(Version {
            recipe,
            generation: self.counter,
            parents,
        });
        bucket.push(Arc::downgrade(&version));
        version
    }
    fn resolve(&mut self, graph: &Graph) -> Vec<bool> {
        let previous = std::mem::take(&mut self.working.versions);
        let mut dirty = vec![false; graph.outputs.len()];
        for (id, output) in graph.outputs.iter().enumerate() {
            let parents: Vec<_> = output
                .dependencies
                .iter()
                .map(|input| self.working.versions[&graph.outputs[*input].cache_name()].clone())
                .collect();
            let recipe = Recipe {
                operation: std::mem::discriminant(&graph.nodes[output.producer].operation),
                parameters: graph.nodes[output.producer].operation.key(output.name),
                data: output.data,
                role: output.name,
                view: output.view,
                inputs: parents.iter().map(|parent| parent.generation).collect(),
            };
            dirty[id] = previous
                .get(&output.cache_name())
                .is_none_or(|old| old.recipe != recipe);
            let version = self.intern(recipe, parents);
            self.working.versions.insert(output.cache_name(), version);
        }
        dirty
    }
    /// Build frontiers from exact changed dependencies. Retained results from another
    /// root, scale or scope may satisfy them only when their complete identity matches.
    fn plan(&mut self, graph: &Graph, targets: &[usize]) -> crate::source::Result<Vec<usize>> {
        self.plan_for(graph, targets, false)
    }
    fn plan_for(
        &mut self,
        graph: &Graph,
        targets: &[usize],
        sealing: bool,
    ) -> crate::source::Result<Vec<usize>> {
        self.frame += 1;
        let required = graph.required(targets)?;
        let initial = self.working.versions.is_empty();
        let mut dirty = self.resolve(graph);
        let changed = required
            .iter()
            .zip(&dirty)
            .any(|(required, dirty)| *required && *dirty);
        if initial {
            let node = graph
                .nodes
                .iter()
                .find(|node| {
                    matches!(
                        node.operation,
                        crate::graph::Operation::Texture { .. }
                            | crate::graph::Operation::BlueGuidedReconstruct(_)
                    ) && required[node.outputs[0]]
                })
                .unwrap_or(&graph.nodes[graph.outputs[targets[0]].producer]);
            let pivot = graph.outputs[node.outputs[0]].producer;
            for (id, output) in graph.outputs.iter().enumerate() {
                dirty[id] = output.producer == pivot
                    || output.dependencies.iter().any(|input| dirty[*input]);
            }
        }
        let finishing: Vec<_> = graph
            .nodes
            .iter()
            .filter(|node| {
                matches!(
                    node.operation,
                    crate::graph::Operation::Tone { .. }
                        | crate::graph::Operation::TonalRanges { .. }
                        | crate::graph::Operation::SignedPhotographic(_)
                        | crate::graph::Operation::LinearSignedResponse(_)
                        | crate::graph::Operation::LinearCodeResponse { .. }
                        | crate::graph::Operation::CodeResponse { .. }
                ) && required[node.outputs[0]]
            })
            .flat_map(|node| {
                node.inputs
                    .iter()
                    .map(|&input| graph.outputs[input].cache_name())
            })
            .collect();
        if initial || changed {
            let mut frontier = Vec::new();
            for (_, output) in graph
                .outputs
                .iter()
                .enumerate()
                .filter(|(id, _)| required[*id] && dirty[*id])
            {
                for &input in &output.dependencies {
                    let name = graph.outputs[input].cache_name();
                    if !dirty[input] && required[input] && !frontier.contains(&name) {
                        frontier.push(name);
                    }
                }
            }
            // The dependency cut restarts this edit. Finishing shortcuts and
            // recipe hints added below are useful alternatives, not that cut.
            self.working.restart = frontier
                .iter()
                .map(|name| self.working.versions[name].generation)
                .collect();
            // Any new finishing identity can feed the next exposure edit. Its
            // geometry and dependencies decide reuse, including after a grid switch.
            for name in &finishing {
                if self.get(name).is_none() && !frontier.contains(name) {
                    frontier.push(name.clone());
                }
            }
            self.working.selected = frontier;
        }
        self.working.primary = finishing
            .into_iter()
            .filter(|name| self.working.selected.contains(name))
            .collect();
        self.working.cuts.clear();
        if sealing {
            self.working.selected.clear();
            self.working.primary.clear();
            self.working.restart.clear();
        }
        for &id in graph.session_outputs.iter().chain(&graph.sealed_outputs) {
            if sealing && !graph.sealed_outputs.contains(&id) {
                continue;
            }
            if !required[id] {
                continue;
            }
            let name = graph.outputs[id].cache_name();
            if graph.sealed_outputs.contains(&id) {
                self.working
                    .cuts
                    .insert(self.working.versions[&name].generation);
            }
            if !self.working.primary.contains(&name) {
                self.working.primary.push(name.clone());
            }
            if !self.working.selected.contains(&name) {
                self.working.selected.push(name);
            }
        }
        self.working.selected.sort_by_key(|name| {
            graph
                .session_outputs
                .iter()
                .rev()
                .position(|id| graph.outputs[*id].cache_name() == *name)
                .unwrap_or(usize::MAX)
        });
        for entry in &mut self.entries {
            if self.working.cuts.contains(&entry.version.generation) {
                entry.pinned = true;
            }
        }
        let cached: Vec<_> = graph
            .outputs
            .iter()
            .map(|output| self.get(&output.cache_name()).is_some())
            .collect();
        let needed = graph.required_until(targets, &cached)?;
        for (id, output) in graph.outputs.iter().enumerate() {
            if !needed[id] || !cached[id] {
                continue;
            }
            let generation = self.working.versions[&output.cache_name()].generation;
            if let Some(entry) = self
                .entries
                .iter_mut()
                .find(|entry| entry.version.generation == generation)
            {
                entry.used = self.frame;
            }
        }
        diagnostics::event(Category::Cache, "cache.checkpoint.plan", |fields| {
            self.diagnostic_source(fields);
            fields
                .bool("initial", initial)
                .bool("changed", changed)
                .bool("sealing", sealing)
                .u64("outputs", graph.outputs.len() as u64)
                .u64("targets", targets.len() as u64)
                .u64(
                    "cached_outputs",
                    cached.iter().filter(|cached| **cached).count() as u64,
                )
                .u64(
                    "reused_outputs",
                    needed
                        .iter()
                        .zip(&cached)
                        .filter(|(needed, cached)| **needed && **cached)
                        .count() as u64,
                )
                .u64("selected_inputs", self.working.selected.len() as u64)
                .u64(
                    "missing_selected_inputs",
                    self.working
                        .selected
                        .iter()
                        .filter(|name| {
                            self.working.versions.get(*name).is_some_and(|version| {
                                !self
                                    .entries
                                    .iter()
                                    .any(|entry| entry.version.generation == version.generation)
                            })
                        })
                        .count() as u64,
                )
                .u64(
                    "missing_restart_inputs",
                    self.working
                        .restart
                        .iter()
                        .filter(|generation| {
                            !self
                                .entries
                                .iter()
                                .any(|entry| entry.version.generation == **generation)
                        })
                        .count() as u64,
                );
        });
        Ok(graph
            .outputs
            .iter()
            .enumerate()
            .filter_map(|(id, output)| {
                (needed[id] && self.working.selected.contains(&output.cache_name())).then_some(id)
            })
            .collect())
    }
    fn get(&self, name: &str) -> Option<Value> {
        let generation = self.working.versions.get(name)?.generation;
        self.entries
            .iter()
            .find(|entry| entry.version.generation == generation)
            .map(|entry| entry.value.clone())
    }
    fn put(&mut self, name: &str, value: &Value) {
        if value.buffer.size() > Self::LIMIT {
            if self
                .working
                .selected
                .iter()
                .any(|selected| selected == name)
            {
                self.observe_checkpoint(
                    "cache.checkpoint.rejected",
                    "value-capacity",
                    &self.working.versions[name],
                    value.buffer.size(),
                    value.buffer.identity(),
                );
            }
            return;
        }
        if !self
            .working
            .selected
            .iter()
            .any(|selected| selected == name)
        {
            return;
        }
        let version = self.working.versions[name].clone();
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.version.generation == version.generation)
        {
            entry.used = self.frame;
            entry.pinned |= self.working.cuts.contains(&version.generation);
            let allocation = entry.value.buffer.identity();
            let bytes = entry.value.buffer.size();
            self.observe_checkpoint(
                "cache.checkpoint.admitted",
                "existing-version",
                &version,
                bytes,
                allocation,
            );
            return;
        }
        let pinned = self.working.cuts.contains(&version.generation);
        let priority = self
            .current_priority(version.generation)
            .unwrap_or((u8::MAX, usize::MAX));
        while !self.room_for(value) {
            let candidate = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| {
                    pinned
                        || (!entry.pinned
                            && self
                                .current_priority(entry.version.generation)
                                .is_none_or(|rank| rank > priority))
                })
                .max_by_key(|(_, entry)| {
                    (
                        u8::from(!entry.pinned),
                        self.current_priority(entry.version.generation)
                            .unwrap_or((u8::MAX, usize::MAX)),
                        std::cmp::Reverse(entry.used),
                    )
                })
                .map(|(index, _)| index);
            let Some(index) = candidate else {
                self.observe_checkpoint(
                    "cache.checkpoint.rejected",
                    "local-priority",
                    &version,
                    value.buffer.size(),
                    value.buffer.identity(),
                );
                return;
            };
            self.remove_entry(index, "local-pixel-capacity");
        }
        self.entries.push(Entry {
            version,
            value: value.clone(),
            pinned,
            used: self.frame,
        });
        let entry = self.entries.last().unwrap();
        self.observe_checkpoint(
            "cache.checkpoint.admitted",
            "new-version",
            &entry.version,
            entry.value.buffer.size(),
            entry.value.buffer.identity(),
        );
    }
    fn current_priority(&self, generation: u64) -> Option<(u8, usize)> {
        self.working
            .selected
            .iter()
            .enumerate()
            .find_map(|(rank, name)| {
                self.working
                    .versions
                    .get(name)
                    .filter(|version| version.generation == generation)?;
                let priority = if self.working.restart.contains(&generation) {
                    0
                } else if self.working.primary.contains(name) {
                    1
                } else {
                    2
                };
                Some((priority, rank))
            })
    }
    fn prune_identities(&mut self) {
        self.identities.retain(|_, bucket| {
            bucket.retain(|identity| identity.strong_count() != 0);
            !bucket.is_empty()
        });
    }
    fn logical_bytes(&self) -> u64 {
        let mut retained = crate::recipe::RetainedBytes::default();
        self.bytes() + self.account_keys(&mut retained) as u64 + retained.bytes() as u64
    }
    /// Commit the immutable inputs of a completed recipe. An unsuccessful
    /// request must not make the previous valid recipe's inputs expendable.
    pub(crate) fn commit(&mut self) {
        self.required_cuts.clone_from(&self.working.cuts);
        self.active_restart.clone_from(&self.working.restart);
    }
    /// A parked photo needs retained values and their exact ancestry. Uncached
    /// side-output descriptors from its last complete graph need no owner.
    pub(crate) fn park(&mut self) {
        self.working.versions.retain(|_, version| {
            self.entries
                .iter()
                .any(|entry| entry.version.generation == version.generation)
        });
        self.working
            .selected
            .retain(|name| self.working.versions.contains_key(name));
        self.working
            .primary
            .retain(|name| self.working.versions.contains_key(name));
        self.prune_identities();
    }
    /// Drop one optional version under an enclosing working-set budget.
    /// Older unselected versions go first; current restart inputs remain useful
    /// after every old alternative has been removed. Required cuts are never
    /// removed here, so pressure cannot silently turn a photo into a source miss.
    pub(crate) fn trim_optional(&mut self, obsolete_only: bool) -> bool {
        self.trim_optional_for(obsolete_only, false)
    }
    /// The active photo keeps its last successful edit's dependency cut.
    /// Opportunistic finishing inputs remain expendable under the same budget.
    pub(crate) fn trim_active_optional(&mut self, obsolete_only: bool) -> bool {
        self.trim_optional_for(obsolete_only, true)
    }
    fn trim_optional_for(&mut self, obsolete_only: bool, active: bool) -> bool {
        let candidate = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| !self.required_cuts.contains(&entry.version.generation))
            .filter(|(_, entry)| {
                !active || !self.active_restart.contains(&entry.version.generation)
            })
            .filter(|(_, entry)| {
                !obsolete_only || self.current_priority(entry.version.generation).is_none()
            })
            .max_by_key(|(_, entry)| {
                (
                    self.current_priority(entry.version.generation)
                        .unwrap_or((u8::MAX, usize::MAX)),
                    std::cmp::Reverse(entry.used),
                )
            })
            .map(|(index, _)| index);
        let Some(index) = candidate else { return false };
        self.remove_entry(
            index,
            if obsolete_only {
                "shared-obsolete-history"
            } else {
                "shared-optional-frontier"
            },
        );
        self.prune_identities();
        true
    }
    pub(crate) fn finish(&mut self) {
        self.prune_identities();
        // Small images may have much larger parameter ancestry. Bound both
        // owners so history cannot grow unbounded merely because pixels are tiny.
        while self.logical_bytes() > Self::LIMIT {
            let candidate = self
                .entries
                .iter()
                .enumerate()
                .max_by_key(|(_, entry)| {
                    (
                        u8::from(!entry.pinned),
                        self.current_priority(entry.version.generation)
                            .unwrap_or((u8::MAX, usize::MAX)),
                        std::cmp::Reverse(entry.used),
                    )
                })
                .map(|(index, _)| index);
            let Some(index) = candidate else {
                break;
            };
            self.remove_entry(index, "local-owner-capacity");
            self.prune_identities();
        }
    }
    /// A seal admits only declared cuts. A refresh preserves older valid optional
    /// inputs, while the initial empty owner carries no speculative native frames.
    pub(crate) fn finish_seal(&mut self) -> bool {
        let required = std::mem::take(&mut self.working.cuts);
        self.repair = None;
        self.working = WorkingSet::default();
        self.finish();
        let retained = required.iter().all(|generation| {
            self.entries
                .iter()
                .any(|entry| entry.pinned && entry.version.generation == *generation)
        });
        if retained {
            self.required_cuts = required;
            self.active_restart.clear();
        }
        retained
    }
}

#[derive(Debug)]
pub(crate) struct Superseded;
impl std::fmt::Display for Superseded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("render superseded")
    }
}
impl std::error::Error for Superseded {}
#[derive(Default)]
pub(crate) struct Execution<'a> {
    pub cache: Option<&'a mut Cache>,
    pub cancel: Option<&'a dyn Fn() -> bool>,
    pub checkpoints: Vec<usize>,
    pub progress: Option<&'a dyn Fn(crate::render::RenderProgress)>,
}
impl Execution<'_> {
    pub(crate) fn prepare_repair(
        &mut self,
        metadata: &crate::source::Metadata,
        threshold: crate::nodes::sensor::repair::Threshold,
    ) -> crate::source::Result<Arc<crate::nodes::sensor::camera_repair::Prepared>> {
        self.check()?;
        if let Some(entry) = self.cache.as_ref().and_then(|cache| cache.repair.as_ref()) {
            if entry.threshold == threshold {
                return Ok(entry.prepared.clone());
            }
        }
        let prepared = Arc::new(crate::nodes::sensor::camera_repair::prepare(
            metadata, threshold,
        )?);
        self.check()?;
        if let Some(cache) = &mut self.cache {
            cache.retain_repair(threshold, &prepared);
        }
        Ok(prepared)
    }

    pub fn check(&self) -> crate::source::Result<()> {
        if self.cancel.is_some_and(|cancel| cancel()) {
            Err(Box::new(Superseded))
        } else {
            Ok(())
        }
    }
    pub fn plan(&mut self, graph: &Graph, targets: &[usize]) -> crate::source::Result<()> {
        if let Some(cache) = &mut self.cache {
            self.checkpoints = cache.plan(graph, targets)?;
        }
        Ok(())
    }
    pub(crate) fn plan_seal(
        &mut self,
        graph: &Graph,
        targets: &[usize],
    ) -> crate::source::Result<()> {
        if let Some(cache) = &mut self.cache {
            self.checkpoints = cache.plan_for(graph, targets, true)?;
        }
        Ok(())
    }
    pub fn seed(&self, name: &str) -> Option<Value> {
        self.cache.as_ref()?.get(name)
    }
    pub fn store(&mut self, name: &str, value: &Value) {
        if let Some(cache) = &mut self.cache {
            cache.put(name, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_identity_is_lazy_and_follows_cache_replacement() {
        let mut first = Cache::default();
        let second = Cache::default();
        assert!(first.diagnostic_id.get().is_none());
        let id = first.diagnostic_id();
        assert_eq!(first.diagnostic_id(), id);
        assert_ne!(second.diagnostic_id(), id);
        first.bind(&Arc::new(()));
        assert!(first.diagnostic_id.get().is_none());
        assert_ne!(first.diagnostic_id(), id);
    }
    #[test]
    fn framing_metadata_is_part_of_the_exact_output_identity() {
        let mut cache = Cache::default();
        let build = |left| {
            let mut graph = Graph::default();
            let root = graph.add(crate::graph::Operation::Source, &[]).unwrap();
            graph.set_view(
                root,
                crate::image::View {
                    crop: [left, 0, 64, 64],
                    rotation: 0,
                    extent: [64 - left, 64],
                },
            );
            (graph, root)
        };
        let (first, root) = build(0);
        cache.plan(&first, &[root]).unwrap();
        let original = cache.working.versions["sensor.layers"].generation;
        let (same, root) = build(0);
        cache.plan(&same, &[root]).unwrap();
        assert_eq!(cache.working.versions["sensor.layers"].generation, original);
        let (changed, root) = build(1);
        cache.plan(&changed, &[root]).unwrap();
        assert_ne!(cache.working.versions["sensor.layers"].generation, original);
    }
    #[test]
    fn repair_slot_is_bounded_and_reset_for_another_engine() {
        use crate::nodes::sensor::{camera_repair::Prepared, repair::Threshold};
        let mut cache = Cache::default();
        let first_owner = Arc::new(());
        let second_owner = Arc::new(());
        cache.bind(&first_owner);
        let small = Arc::new(Prepared::default());
        cache.retain_repair(Threshold::Camera, &small);
        let slot = cache.repair.as_ref().unwrap();
        assert!(Arc::ptr_eq(&slot.prepared, &small));
        assert!(slot.bytes <= Cache::REPAIR_LIMIT);
        cache.bind(&first_owner);
        assert!(Arc::ptr_eq(
            &cache.repair.as_ref().unwrap().prepared,
            &small
        ));
        let oversized = Arc::new(Prepared::oversized_for_test());
        cache.retain_repair(Threshold::Manual(0), &oversized);
        assert!(cache.repair.is_none());
        assert_eq!(Arc::strong_count(&oversized), 1);
        cache.retain_repair(Threshold::Camera, &small);
        cache.bind(&second_owner);
        assert!(cache.repair.is_none());
    }

    #[test]
    fn repair_slot_error_and_cancel_preserve_valid_entry() {
        use crate::nodes::sensor::{camera_repair::Prepared, repair::Threshold};
        let mut cache = Cache::default();
        let saved = Arc::new(Prepared::default());
        cache.retain_repair(Threshold::Camera, &saved);
        let metadata = crate::source::Metadata {
            width: 1,
            height: 1,
            rotation: 0,
            processing_version: None,
            properties: Default::default(),
            matrices: Default::default(),
            lists: Default::default(),
        };
        let mut execution = Execution {
            cache: Some(&mut cache),
            ..Execution::default()
        };
        let hit = execution
            .prepare_repair(&metadata, Threshold::Camera)
            .unwrap();
        assert!(Arc::ptr_eq(&hit, &saved));
        let error = execution
            .prepare_repair(&metadata, Threshold::Manual(16))
            .err()
            .unwrap();
        assert!(error.to_string().contains("badness"));
        assert!(Arc::ptr_eq(
            &execution
                .cache
                .as_ref()
                .unwrap()
                .repair
                .as_ref()
                .unwrap()
                .prepared,
            &saved
        ));
        let cancel = || true;
        execution.cancel = Some(&cancel);
        assert!(execution
            .prepare_repair(&metadata, Threshold::Camera)
            .is_err());
        assert!(Arc::ptr_eq(
            &execution
                .cache
                .as_ref()
                .unwrap()
                .repair
                .as_ref()
                .unwrap()
                .prepared,
            &saved
        ));
    }

    fn graph_for_detail(amount: f32) -> (Graph, usize) {
        use crate::{graph::Operation as O, noise::Parameters};
        use bytemuck::Zeroable;
        let mut p = Parameters::zeroed();
        p.source = [64, 64, 1, 1];
        p.tile = [64, 64, 0, 3];
        p.reduction[0] = 1.;
        p = p.full_frame();
        let mut graph = Graph::default();
        let input = graph
            .add(
                O::RenderedInput {
                    data: DataType::CalibratedLayers,
                },
                &[],
            )
            .unwrap();
        let base = graph.add(O::NoiseBase(p), &[input]).unwrap();
        let raw = graph
            .add(
                O::Column {
                    params: p,
                    enabled: true,
                },
                &[input],
            )
            .unwrap();
        let ratio = graph.add(O::Ratio(p), &[raw, base]).unwrap();
        let prepared =
            crate::nodes::color::pattern::Prepared::new([64, 64, 1, 0], [0.; 4]).unwrap();
        let pattern = graph
            .add(
                O::Pattern {
                    params: p,
                    prepared,
                },
                &[ratio, base + 3],
            )
            .unwrap();
        let detail = graph
            .add(
                O::Texture {
                    params: p,
                    prepared: crate::nodes::color::texture::Prepared::new(
                        crate::noise::Controls {
                            texture: crate::noise::TextureControls {
                                fine: amount,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        1.,
                        Default::default(),
                    )
                    .unwrap(),
                },
                &[raw, base, pattern],
            )
            .unwrap();
        (graph, detail)
    }
    #[test]
    fn recipe_restart_boundaries_supplement_the_adjustable_frontier() {
        let (mut graph, root) = graph_for_detail(1.);
        let original = graph.nodes[0].outputs[0];
        let mut cache = Cache::default();
        assert!(!cache.plan(&graph, &[root]).unwrap().contains(&original));
        graph.retain_for_session(original);
        let frontier = cache.plan(&graph, &[root]).unwrap();
        assert!(frontier.contains(&original));
        assert!(
            frontier.len() > 1,
            "recipe hints must retain the editing frontier too"
        );
    }
    #[test]
    fn only_successful_dependency_frontiers_become_active_restart_inputs() {
        let (mut graph, root) = graph_for_detail(1.);
        let original = graph.nodes[0].outputs[0];
        graph.retain_for_session(original);
        let space = crate::color_space::ColorSpace::Srgb;
        let finishing_input = graph
            .add_scoped(
                crate::graph::Operation::RenderedInput {
                    data: DataType::LinearRgb(space),
                },
                &[],
                "finishing",
            )
            .unwrap();
        let finishing = graph
            .add(
                crate::graph::Operation::Tone {
                    curve: vec![0., 1.].into(),
                    exposure: 1.,
                    space,
                },
                &[finishing_input],
            )
            .unwrap();
        let mut cache = Cache::default();
        cache.plan(&graph, &[root, finishing]).unwrap();
        assert_eq!(cache.working.restart.len(), 3);
        assert!(!cache
            .working
            .restart
            .contains(&cache.working.versions[&graph.outputs[original].cache_name()].generation));
        let finishing_name = graph.outputs[finishing_input].cache_name();
        assert!(cache.working.selected.contains(&finishing_name));
        assert!(!cache
            .working
            .restart
            .contains(&cache.working.versions[&finishing_name].generation));
        let restart = *cache.working.restart.iter().next().unwrap();
        assert!(
            cache.current_priority(restart).unwrap()
                < cache
                    .current_priority(cache.working.versions[&finishing_name].generation)
                    .unwrap()
        );
        assert!(cache.active_restart.is_empty());
        cache.commit();
        let completed = cache.active_restart.clone();
        // Planning another root does not commit an interrupted request's cut.
        let mut other = Graph::default();
        let other_root = other.add(crate::graph::Operation::Source, &[]).unwrap();
        cache.plan(&other, &[other_root]).unwrap();
        assert!(cache.working.restart.is_empty());
        assert_eq!(cache.active_restart, completed);
    }
    #[test]
    #[ignore = "requires a GPU"]
    fn active_pressure_keeps_restart_inputs_and_the_complete_graph_identity() {
        let gpu = crate::gpu::Gpu::new().unwrap();
        let (mut graph, root) = graph_for_detail(1.);
        let original = graph.nodes[0].outputs[0];
        graph.retain_for_session(original);
        let mut cache = Cache::default();
        let checkpoints = cache.plan(&graph, &[root]).unwrap();
        let value = Value {
            view_baked: false,
            buffer: gpu.upload("restart input", &[1_f32; 3]).unwrap(),
            description: None,
            working_scale: crate::render::Scale::NATIVE,
            view: None,
        };
        for id in checkpoints {
            cache.put(&graph.outputs[id].cache_name(), &value);
        }
        cache.commit();
        let identities = cache.working.versions.len();
        let restart = cache.active_restart.clone();
        assert_eq!(cache.entries.len(), 4);
        assert!(
            cache.trim_active_optional(false),
            "the session hint is optional"
        );
        assert_eq!(cache.entries.len(), 3);
        assert!(
            !cache.trim_active_optional(false),
            "the active cut is essential"
        );
        assert_eq!(cache.working.versions.len(), identities);
        let (next, root) = graph_for_detail(1.2);
        cache.plan(&next, &[root]).unwrap();
        assert_eq!(cache.working.restart, restart);
        assert!(
            cache.trim_optional(false),
            "inactive history may shed the same cut"
        );
    }
    #[test]
    fn detail_changes_only_its_own_recipe_and_keeps_three_input_versions() {
        let mut cache = Cache::default();
        let (first, root) = graph_for_detail(1.);
        let frontier = cache.plan(&first, &[root]).unwrap();
        assert_eq!(frontier.len(), 3);
        let inputs: Vec<_> = frontier
            .iter()
            .map(|id| {
                (
                    first.outputs[*id].name,
                    cache.working.versions[first.outputs[*id].name].generation,
                )
            })
            .collect();
        let previous = cache.working.versions["texture.layers-tiles"].generation;
        let (second, root) = graph_for_detail(1.2);
        cache.plan(&second, &[root]).unwrap();
        for (name, version) in inputs {
            assert_eq!(cache.working.versions[name].generation, version);
        }
        assert_ne!(
            cache.working.versions["texture.layers-tiles"].generation,
            previous
        );
    }
    #[test]
    fn distinct_roots_share_matching_inputs_and_recover_retained_versions() {
        let mut cache = Cache::default();
        let (full, root) = graph_for_detail(1.);
        cache.plan(&full, &[root]).unwrap();
        let input = cache.working.versions["noise.tiles"].generation;
        let retained_full = cache.working.versions["texture.layers-tiles"].clone();
        let full_result = retained_full.generation;
        let (preview, root) = graph_for_detail(1.2);
        cache.plan(&preview, &[root]).unwrap();
        assert_eq!(cache.working.versions["noise.tiles"].generation, input);
        assert_ne!(
            cache.working.versions["texture.layers-tiles"].generation,
            full_result
        );
        cache.plan(&full, &[root]).unwrap();
        assert_eq!(
            cache.working.versions["texture.layers-tiles"].generation,
            full_result
        );
        // Invalidating an upstream recipe in Full must also invalidate preview
        // when it next runs, without falsely adopting an unrelated generation.
        let (mut changed, root) = graph_for_detail(1.);
        let crate::graph::Operation::NoiseBase(params) = &mut changed.nodes[1].operation else {
            panic!("expected noise base");
        };
        params.reduction[0] = 0.25;
        cache.plan(&changed, &[root]).unwrap();
        let new_input = cache.working.versions["noise.tiles"].generation;
        assert_ne!(new_input, input);
        cache.plan(&changed, &[root]).unwrap();
        assert_eq!(cache.working.versions["noise.tiles"].generation, new_input);
        assert_ne!(
            cache.working.versions["texture.layers-tiles"].generation,
            full_result
        );
    }
    #[test]
    fn texture_edits_invalidate_color_and_sharpening_but_reuse_filtering() {
        use crate::{graph::Operation as O, image::ColorSpace as C};
        use bytemuck::Zeroable;
        let (mut graph, detail) = graph_for_detail(1.);
        let detail_node = graph.outputs[detail].producer;
        let O::Texture { params, .. } = graph.nodes[detail_node].operation else {
            unreachable!()
        };
        let color = graph
            .add(
                O::TiledColor {
                    params,
                    color: crate::nodes::sensor::calibration::ColorParams::zeroed(),
                    space: C::Srgb,
                },
                &[detail],
            )
            .unwrap();
        let rgb = graph
            .add(
                O::ProjectColorTiles {
                    params,
                    space: C::Srgb,
                },
                &[color],
            )
            .unwrap();
        let signal = graph
            .add(
                O::SharpSignal {
                    space: C::Srgb,
                    scale: 1.,
                },
                &[rgb],
            )
            .unwrap();
        let sharpened = graph
            .add(
                O::Sharpen {
                    amount: 0.75,
                    space: C::Srgb,
                },
                &[rgb, signal],
            )
            .unwrap();
        let final_id = graph
            .add(
                O::Tone {
                    curve: vec![0., 1.].into(),
                    exposure: 0.,
                    space: C::Srgb,
                },
                &[sharpened],
            )
            .unwrap();
        let mut cache = Cache::default();
        let inputs = graph.nodes[detail_node].inputs.clone();
        let frontier = cache.plan(&graph, &[final_id]).unwrap();
        assert!(inputs.iter().all(|id| frontier.contains(id)));
        let old_signal = cache.working.versions["detail.signal"].generation;
        graph.nodes[detail_node].operation = O::Texture {
            params,
            prepared: crate::nodes::color::texture::Prepared::new(
                crate::noise::Controls {
                    texture: crate::noise::TextureControls {
                        fine: 1.4,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                1.,
                Default::default(),
            )
            .unwrap(),
        };
        let frontier = cache.plan(&graph, &[final_id]).unwrap();
        assert!(inputs.iter().all(|id| frontier.contains(id)));
        assert_ne!(
            old_signal,
            cache.working.versions["detail.signal"].generation
        );
        graph.nodes[graph.outputs[final_id].producer].operation = O::Tone {
            curve: vec![0., 1.].into(),
            exposure: 0.5,
            space: C::Srgb,
        };
        assert_eq!(cache.plan(&graph, &[final_id]).unwrap(), vec![sharpened]);
    }
    #[test]
    fn guided_reconstruction_keeps_both_planes_and_the_later_mask_branch() {
        use crate::{
            graph::Operation as O,
            nodes::color::{blue_guided_reconstruct, byte_mask_double, residual_finish},
        };
        let mut graph = Graph::default();
        let rgb = graph
            .add(
                O::RenderedInput {
                    data: DataType::CorrectedSensorRgb,
                },
                &[],
            )
            .unwrap();
        graph.outputs[rgb].name = "test.rgb";
        let blue = graph
            .add(
                O::RenderedInput {
                    data: DataType::CorrectedSensorBlue,
                },
                &[],
            )
            .unwrap();
        graph.outputs[blue].name = "test.blue";
        let original_mask = graph
            .add(
                O::RenderedInput {
                    data: DataType::HighlightMask,
                },
                &[],
            )
            .unwrap();
        graph.outputs[original_mask].name = "test.mask";
        let reconstructed = graph
            .add(
                O::BlueGuidedReconstruct(Arc::new(
                    blue_guided_reconstruct::Prepared::new([8, 8], 0., [0.; 3], None).unwrap(),
                )),
                &[rgb, rgb, rgb, blue],
            )
            .unwrap();
        let residual = graph
            .named_output(reconstructed, "reconstruction.residual")
            .unwrap();
        let mask = graph
            .add(
                O::ByteMaskDouble(byte_mask_double::Params::new([8, 8]).unwrap()),
                &[original_mask],
            )
            .unwrap();
        let finish = |detail| O::ResidualFinish {
            params: residual_finish::Params::new([16, 16], detail, 0., 0, 0., 0.).unwrap(),
            with_mask: true,
        };
        let root = graph
            .add(finish(1.), &[reconstructed, residual, mask])
            .unwrap();
        let mut cache = Cache::default();
        let frontier = cache.plan(&graph, &[root]).unwrap();
        assert_eq!(frontier, vec![rgb, blue, mask]);
        let previous = cache.working.versions["residual.rgb"].generation;
        graph.nodes[graph.outputs[root].producer].operation = finish(1.2);
        // A detail edit can use the initial checkpoints before reconstruction.
        assert_eq!(
            cache.plan(&graph, &[root]).unwrap(),
            vec![reconstructed, residual, mask]
        );
        assert_ne!(cache.working.versions["residual.rgb"].generation, previous);
        graph.nodes[graph.outputs[reconstructed].producer].operation = O::BlueGuidedReconstruct(
            Arc::new(blue_guided_reconstruct::Prepared::new([8, 8], 0.5, [0.; 3], None).unwrap()),
        );
        assert_eq!(cache.plan(&graph, &[root]).unwrap(), frontier);
    }

    #[test]
    fn shared_uniform_fields_have_stage_specific_keys() {
        use crate::noise::{Parameters, RecipeStage as S};
        use bytemuck::Zeroable;
        let a = Parameters::zeroed();
        let mut b = a;
        b.filter[3] = 0.9;
        assert!(a.key(S::Base) != b.key(S::Base));
        assert!(a.key(S::Column) == b.key(S::Column));
        b = a;
        b.mono[0] = 2.;
        assert!(a.key(S::Ratio) != b.key(S::Ratio));
        assert!(a.key(S::Base) == b.key(S::Base));
    }

    #[test]
    fn regional_keys_separate_global_context_tiles_and_projected_views() {
        use crate::{graph::Operation as O, noise::Parameters};
        use bytemuck::Zeroable;
        let mut p = Parameters::zeroed();
        p.source = [256, 256, 4, 4];
        p.tile = [192, 192, 64, 11];
        p.reduction[0] = 0.5;
        p = p.full_frame();
        let a = p.for_region([10, 10, 20, 20]).unwrap();
        let b = p.for_region([20, 20, 30, 30]).unwrap();
        let distant = p.for_region([200, 200, 210, 210]).unwrap();
        assert!(
            O::NoiseContext(a).key("noise.global-context")
                == O::NoiseContext(distant).key("noise.global-context")
        );
        assert!(O::NoiseBase(a).key("noise.tiles") == O::NoiseBase(b).key("noise.tiles"));
        assert!(O::NoiseBase(a).key("noise.layers") != O::NoiseBase(b).key("noise.layers"));
        assert!(O::NoiseBase(a).key("noise.tiles") != O::NoiseBase(distant).key("noise.tiles"));
        let prepared =
            crate::nodes::color::texture::Prepared::new(Default::default(), 1., Default::default())
                .unwrap();
        assert!(
            O::Texture {
                params: a,
                prepared
            }
            .key("texture.layers-tiles")
                == O::Texture {
                    params: b,
                    prepared
                }
                .key("texture.layers-tiles")
        );
        assert!(
            O::Texture {
                params: a,
                prepared
            }
            .key("texture.change")
                != O::Texture {
                    params: b,
                    prepared
                }
                .key("texture.change")
        );
    }
    #[test]
    fn coalescing_tracks_parameter_identity_not_recipe_invalidation() {
        let a = Settings::new("Sunlight", "Standard");
        let mut b = a.clone();
        b.exposure = 0.1;
        assert_eq!(a.changed_parameters(&b), vec!["exposure"]);
        b = a.clone();
        b.reconstruction = Some(Default::default());
        assert_eq!(a.changed_parameters(&b), vec!["reconstruction"]);
        let mut c = b.clone();
        c.reconstruction.as_mut().unwrap().detail = 1.1;
        assert_eq!(b.changed_parameters(&c), vec!["detail"]);
        b = a.clone();
        b.merrill_speckles.threshold = 0.25;
        assert_eq!(a.changed_parameters(&b), vec!["merrill-speckles"]);
        c = b.clone();
        c.neutralize_shadows.strength = 0.8;
        assert_eq!(b.changed_parameters(&c), vec!["neutralize-shadows"]);
    }
    #[test]
    #[ignore = "requires a GPU"]
    fn cached_aliases_count_one_allocation() {
        let gpu = crate::gpu::Gpu::new().unwrap();
        let buffer = gpu.upload("cache alias", &[0_f32; 16]).unwrap();
        let value = Value {
            view_baked: false,
            buffer,
            description: None,
            working_scale: crate::render::Scale::NATIVE,
            view: None,
        };
        let (graph, root) = graph_for_detail(1.);
        let mut cache = Cache::default();
        cache.plan(&graph, &[root]).unwrap();
        let version = cache.working.versions["noise.tiles"].clone();
        cache.entries = vec![
            Entry {
                version: version.clone(),
                value: value.clone(),
                pinned: false,
                used: 0,
            },
            Entry {
                version,
                value: value.clone(),
                pinned: false,
                used: 0,
            },
        ];
        assert_eq!(cache.bytes(), value.buffer.size());
        assert!(cache.room_for(&value));
    }

    #[test]
    #[ignore = "requires a GPU"]
    fn cached_edit_boundary_does_not_refill_evicted_upstream_checkpoints() {
        use crate::{
            color_space::ColorSpace, graph::Operation, nodes::finish::photographic::Params,
        };
        let gpu = crate::gpu::Gpu::new().unwrap();
        let recipe = |amount| {
            let mut graph = Graph::default();
            let input = graph
                .add(
                    Operation::RenderedInput {
                        data: DataType::ToneMappedRgb(ColorSpace::Srgb),
                    },
                    &[],
                )
                .unwrap();
            let mut params = Params::new([1, 1], ColorSpace::Srgb);
            let base = graph
                .add(
                    Operation::ImageAlias {
                        dehaze_basis: None,
                        data: DataType::ToneMappedRgb(ColorSpace::Srgb),
                        output: "finish.base",
                    },
                    &[input],
                )
                .unwrap();
            params.values = [amount, 1., 0., 0.];
            let target = graph
                .add(
                    Operation::PhotographicTone {
                        params,
                        space: ColorSpace::Srgb,
                    },
                    &[base],
                )
                .unwrap();
            graph.retain_for_session(input);
            graph.retain_for_session(base);
            (graph, input, base, target)
        };
        let mut cache = Cache::default();
        let (graph, input, base, target) = recipe(0.4);
        assert!(cache.plan(&graph, &[target]).unwrap().contains(&input));
        cache.put(
            "finish.base",
            &Value {
                view_baked: false,
                buffer: gpu.upload("retained edit input", &[0.3_f32; 3]).unwrap(),
                description: None,
                working_scale: crate::render::Scale::NATIVE,
                view: None,
            },
        );
        let (graph, _, _, target) = recipe(0.5);
        let checkpoints = cache.plan(&graph, &[target]).unwrap();
        assert!(checkpoints.contains(&base));
        assert!(!checkpoints.contains(&input), "an uncached input behind the retained boundary should not become another execution root");
        // Switching roots can need that upstream work again.
        assert!(cache.plan(&graph, &[input]).unwrap().contains(&input));
    }
}

#[cfg(test)]
#[path = "../examples/chroma_investigation/mod.rs"]
mod chroma_investigation;

#[cfg(test)]
#[path = "interactive_scale_test.rs"]
mod scale_tests;

#[cfg(test)]
#[path = "interactive_cache_test.rs"]
mod cache_tests;
