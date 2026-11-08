//! Owns source-free camera dependency cuts and bounds multi-photo editing resources.

use super::{recipes, Engine, Scale, Settings};
use crate::{
    inspection::Artifact,
    interactive::{Cache, Execution, Superseded},
    profile::Profile,
    source::{Metadata, Result, Source, SourceOrigin},
};
use std::sync::Arc;

/// The caller recovers authoritative native samples outside the processing core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeedsOriginal {
    UpstreamChanged,
    EngineChanged,
}

pub enum ReducedPreparation {
    Prepared(Box<ReducedSession>),
    Superseded,
}
pub enum ReducedRender {
    Rendered(Vec<Artifact>),
    NeedsOriginal(NeedsOriginal),
    Superseded,
}

/// Retains immutable recipe-declared camera inputs, never decoded source samples.
/// The complete camera and photographic recipes still author every render.
pub struct ReducedSession {
    metadata: Arc<Metadata>,
    origin: SourceOrigin,
    owner: Arc<()>,
    scale: Scale,
    cache: Cache,
    preparation: super::source_preparation::SourcePreparation,
    metadata_bytes: u64,
    last_settings: Option<Box<Settings>>,
}

/// Parameter allocations attributed once to their earliest retained output version.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ParameterResources {
    pub output: &'static str,
    pub versions: usize,
    pub payload_bytes: u64,
}

/// Logical retained owners, excluding allocator/driver overhead and delivered images.
/// Per-session payloads may also be shared with Engine preparation; working-set
/// totals deduplicate those allocations rather than summing these diagnostics.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SessionResources {
    pub metadata_bytes: u64,
    pub owner_bytes: u64,
    pub cache_gpu_bytes: u64,
    pub cache_structure_bytes: u64,
    pub parameter_payload_bytes: u64,
    pub preparation_bytes: u64,
    pub source_bytes: u64,
    pub output_parameters: Vec<ParameterResources>,
}
impl ReducedSession {
    /// Inspect resource ownership outside timed editing interactions.
    pub fn resources(&self) -> SessionResources {
        let mut retained = crate::recipe::RetainedBytes::default();
        let cache_containers = self.cache.account_keys(&mut retained);
        let payloads = retained.payload_bytes();
        let cache_bytes = retained.bytes();
        let scalar_containers = self.preparation.account(&mut retained);
        SessionResources {
            source_bytes: 0,
            metadata_bytes: self.metadata_bytes,
            owner_bytes: self.owned_cpu_bytes() - self.metadata_bytes,
            cache_gpu_bytes: self.cached_bytes(),
            cache_structure_bytes: (cache_containers + cache_bytes - payloads) as u64,
            parameter_payload_bytes: payloads as u64,
            preparation_bytes: (scalar_containers + retained.bytes() - cache_bytes) as u64,
            output_parameters: self.cache.parameter_resources(),
        }
    }
    pub fn metadata(&self) -> &Arc<Metadata> {
        &self.metadata
    }
    pub fn scale(&self) -> Scale {
        self.scale
    }
    pub fn cached_bytes(&self) -> u64 {
        unique_buffer_bytes(self.buffers())
    }
    /// Logical metadata, recipe identity and container payloads, excluding allocator overhead.
    pub fn retained_cpu_bytes(&self) -> u64 {
        let mut retained = crate::recipe::RetainedBytes::default();
        let containers = self.account_keys(&mut retained);
        self.owned_cpu_bytes() + containers as u64 + retained.bytes() as u64
    }
    fn account_keys(&self, retained: &mut crate::recipe::RetainedBytes) -> usize {
        self.cache.account_keys(retained) + self.preparation.account(retained)
    }
    fn owned_cpu_bytes(&self) -> u64 {
        self.metadata_bytes
            + self.origin.allocated_bytes() as u64
            + std::mem::size_of::<Self>() as u64
            + self.last_settings.as_ref().map_or(0, |settings| {
                std::mem::size_of::<Settings>()
                    + settings.white_balance.capacity()
                    + settings.color_mode.capacity()
            }) as u64
    }
    fn buffers(&self) -> impl Iterator<Item = &crate::gpu::ReadGuard> {
        self.cache.buffers()
    }
}

fn unique_buffer_bytes<'a>(buffers: impl Iterator<Item = &'a crate::gpu::ReadGuard>) -> u64 {
    let mut unique: Vec<&crate::gpu::ReadGuard> = Vec::new();
    for buffer in buffers {
        if !unique.iter().any(|prior| prior.shares_storage(buffer)) {
            unique.push(buffer);
        }
    }
    unique.into_iter().map(|buffer| buffer.size()).sum()
}

impl Engine {
    /// Consume native samples after materializing recipe-declared immutable inputs.
    /// Other owners, such as a pending disk-cache write, may temporarily share them.
    pub fn prepare_reduced_session(
        &self,
        source: impl Into<Arc<Source>>,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<ReducedPreparation> {
        let source = source.into();
        settings.validate_values()?;
        profile.mode.check_gpu_features(self.device().features())?;
        let preparation = super::source_preparation::SourcePreparation::default();
        let mut cache = Cache::default();
        cache.bind(&self.owner);
        cache.bind_source(source.origin());
        let mut execution = Execution {
            cache: Some(&mut cache),
            cancel: Some(&cancel),
            ..Execution::default()
        };
        let prepared = (|| -> Result<()> {
            execution.check()?;
            let graph = recipes::sealing_inputs(
                self,
                recipes::RecipeInput::Preparing {
                    source: &source,
                    preparation: &preparation,
                },
                settings,
                profile,
                &mut execution,
            )?;
            let roots = &graph.sealed_outputs;
            if roots.is_empty() {
                return Err("camera recipe declared no immutable sensor dependencies".into());
            }
            execution.plan_seal(&graph, roots)?;
            self.execute_controlled(
                super::Input::Source(&source),
                &graph,
                roots,
                profile,
                &mut execution,
            )?;
            Ok(())
        })();
        let retained = cache.finish_seal();
        self.release_native_preparation(&source.metadata);
        match prepared {
            Ok(()) if !retained => {
                Err("required camera dependencies exceed the retained input budget".into())
            }
            Ok(()) => Ok(ReducedPreparation::Prepared(Box::new(ReducedSession {
                metadata_bytes: source.metadata.retained_bytes(),
                metadata: source.metadata.clone(),
                origin: source.origin().clone(),
                owner: self.owner.clone(),
                scale: settings.working_scale,
                cache,
                preparation,
                last_settings: None,
            }))),
            Err(error) if error.downcast_ref::<Superseded>().is_some() => {
                Ok(ReducedPreparation::Superseded)
            }
            Err(error) => Err(error),
        }
    }

    /// Recover missing native dependencies while preserving every still-valid cut.
    /// The caller supplies a freshly verified load of the same original; metadata
    /// remains the numerical owner and is never replaced to forge source identity.
    pub fn refresh_reduced_session(
        &self,
        session: &mut ReducedSession,
        source: &Source,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<Option<()>> {
        settings.validate_values()?;
        if !Arc::ptr_eq(&session.owner, &self.owner) {
            return Err("camera dependencies belong to another engine".into());
        }
        if session.origin != *source.origin() {
            return Err("refreshed source is not the same verified original".into());
        }
        session.cache.bind(&self.owner);
        session.cache.bind_source(source.origin());
        let mut execution = Execution {
            cache: Some(&mut session.cache),
            cancel: Some(&cancel),
            ..Execution::default()
        };
        let prepared = (|| -> Result<()> {
            execution.check()?;
            let graph = recipes::sealing_inputs(
                self,
                recipes::RecipeInput::Preparing {
                    source,
                    preparation: &session.preparation,
                },
                settings,
                profile,
                &mut execution,
            )?;
            execution.plan_seal(&graph, &graph.sealed_outputs)?;
            self.execute_controlled(
                super::Input::Source(source),
                &graph,
                &graph.sealed_outputs,
                profile,
                &mut execution,
            )?;
            Ok(())
        })();
        let retained = session.cache.finish_seal();
        self.release_native_preparation(&source.metadata);
        match prepared {
            Ok(()) if !retained => {
                Err("required camera dependencies exceed the retained input budget".into())
            }
            Ok(()) => {
                session.metadata_bytes = source.metadata.retained_bytes();
                session.metadata = source.metadata.clone();
                session.scale = settings.working_scale;
                Ok(Some(()))
            }
            Err(error) if error.downcast_ref::<Superseded>().is_some() => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn render_reduced_session(
        &self,
        session: &mut ReducedSession,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<ReducedRender> {
        self.render_reduced_session_observed(session, settings, names, cancel, profile, None)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_reduced_session_with_progress(
        &self,
        session: &mut ReducedSession,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: impl Fn(super::RenderProgress),
    ) -> Result<ReducedRender> {
        self.render_reduced_session_observed(
            session,
            settings,
            names,
            cancel,
            profile,
            Some(&progress),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn render_reduced_session_observed(
        &self,
        session: &mut ReducedSession,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: Option<&dyn Fn(super::RenderProgress)>,
    ) -> Result<ReducedRender> {
        settings.validate_values()?;
        if !Arc::ptr_eq(&session.owner, &self.owner) {
            return Ok(ReducedRender::NeedsOriginal(NeedsOriginal::EngineChanged));
        }
        session.cache.bind(&self.owner);
        session.cache.bind_source(&session.origin);
        let mut execution = Execution {
            cache: Some(&mut session.cache),
            cancel: Some(&cancel),
            progress,
            ..Execution::default()
        };
        let result = self.render_recipe(
            recipes::RecipeInput::Metadata {
                metadata: &session.metadata,
                origin: &session.origin,
                preparation: &session.preparation,
            },
            settings,
            recipes::Recipe::Photographic(names),
            profile,
            &mut execution,
        );
        if result.is_ok() {
            session.cache.commit();
        }
        session.cache.finish();
        match result {
            Ok(artifacts) => {
                session.scale = settings.working_scale;
                session.last_settings = Some(Box::new(settings.clone()));
                Ok(ReducedRender::Rendered(artifacts))
            }
            Err(error)
                if error
                    .downcast_ref::<super::execute::SourceRequired>()
                    .is_some() =>
            {
                Ok(ReducedRender::NeedsOriginal(NeedsOriginal::UpstreamChanged))
            }
            Err(error) if error.downcast_ref::<Superseded>().is_some() => {
                Ok(ReducedRender::Superseded)
            }
            Err(error) => Err(error),
        }
    }
}

struct Entry {
    id: u64,
    session: Box<ReducedSession>,
}
/// One active source-free owner and a byte-bounded history of inactive photos.
/// Activating a photo transfers ownership rather than copying its camera inputs.
pub struct ReducedWorkingSet {
    limit: u64,
    active: Option<Entry>,
    entries: Vec<Entry>,
}
impl ReducedWorkingSet {
    pub fn new(limit: u64) -> Self {
        Self {
            limit,
            active: None,
            entries: Vec::new(),
        }
    }
    pub fn limit_bytes(&self) -> u64 {
        self.limit
    }
    /// Apply a changed historical budget through the ordinary trimming policy.
    /// The active photograph remains independently bounded by its graph cache.
    pub fn set_limit(&mut self, limit: u64) -> Vec<u64> {
        self.limit = limit;
        self.enforce()
    }
    fn all(&self) -> impl Iterator<Item = &Entry> {
        self.active.iter().chain(self.entries.iter())
    }
    pub fn contains(&self, id: u64) -> bool {
        self.all().any(|entry| entry.id == id)
    }
    pub fn metadata(&self, id: u64) -> Option<&Arc<Metadata>> {
        self.all()
            .find(|entry| entry.id == id)
            .map(|entry| entry.session.metadata())
    }
    pub fn origin(&self, id: u64) -> Option<&SourceOrigin> {
        self.all()
            .find(|entry| entry.id == id)
            .map(|entry| &entry.session.origin)
    }
    pub fn last_settings(&self, id: u64) -> Option<&Settings> {
        self.all()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.session.last_settings.as_deref())
    }
    pub fn session_bytes(&self, id: u64) -> Option<(u64, u64)> {
        self.all().find(|entry| entry.id == id).map(|entry| {
            (
                entry.session.cached_bytes(),
                entry.session.retained_cpu_bytes(),
            )
        })
    }
    pub fn session_resources(&self, id: u64) -> Option<SessionResources> {
        self.all()
            .find(|entry| entry.id == id)
            .map(|entry| entry.session.resources())
    }
    pub fn len(&self) -> usize {
        self.entries.len() + usize::from(self.active.is_some())
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.active.is_none()
    }
    pub fn active_bytes(&self) -> u64 {
        self.active.as_ref().map_or(0, |entry| {
            entry.session.cached_bytes() + entry.session.retained_cpu_bytes()
        })
    }
    /// The active graph cache's GPU/key bound. Metadata and scalar preparation are inventoried separately.
    pub fn active_cache_limit_bytes(&self) -> u64 {
        Cache::LIMIT
    }
    pub fn historical_bytes(&self) -> u64 {
        unique_buffer_bytes(
            self.entries
                .iter()
                .flat_map(|entry| entry.session.buffers()),
        ) + Self::account_cpu_for(
            self.entries.iter(),
            &mut crate::recipe::RetainedBytes::default(),
        )
    }
    pub fn retained_gpu_bytes(&self) -> u64 {
        unique_buffer_bytes(self.all().flat_map(|entry| entry.session.buffers()))
    }
    pub fn retained_cpu_bytes(&self) -> u64 {
        Self::account_cpu_for(self.all(), &mut crate::recipe::RetainedBytes::default())
    }
    fn account_cpu_for<'a>(
        entries: impl Iterator<Item = &'a Entry>,
        retained: &mut crate::recipe::RetainedBytes,
    ) -> u64 {
        let mut metadata = Vec::new();
        let containers: u64 = entries
            .map(|entry| {
                let session = &entry.session;
                let mut bytes = session.owned_cpu_bytes() + session.account_keys(retained) as u64;
                if metadata
                    .iter()
                    .any(|old| Arc::ptr_eq(old, session.metadata()))
                {
                    bytes -= session.metadata_bytes;
                } else {
                    metadata.push(session.metadata().clone());
                }
                bytes
            })
            .sum();
        containers + retained.bytes() as u64
    }
    pub(super) fn account_memory(&self, retained: &mut crate::recipe::RetainedBytes) -> u64 {
        self.all()
            .map(|entry| {
                let session = &entry.session;
                retained.owner(session.metadata.clone(), session.metadata_bytes as usize);
                session.owned_cpu_bytes() - session.metadata_bytes
                    + session.account_keys(retained) as u64
            })
            .sum()
    }
    pub(super) fn memory_buffers(&self) -> impl Iterator<Item = &crate::gpu::ReadGuard> {
        self.all().flat_map(|entry| entry.session.buffers())
    }
    pub(super) fn active_metadata(&self) -> Option<&Arc<Metadata>> {
        self.active.as_ref().map(|entry| entry.session.metadata())
    }
    pub(super) fn active_origin(&self) -> Option<&SourceOrigin> {
        self.active.as_ref().map(|entry| &entry.session.origin)
    }
    pub(super) fn trim_optional(&mut self, obsolete_only: bool) -> bool {
        self.entries.iter_mut().any(|entry| {
            let trimmed = entry.session.cache.trim_optional(obsolete_only);
            if trimmed {
                entry.session.cache.park();
            }
            trimmed
        }) || self
            .active
            .as_mut()
            .is_some_and(|entry| entry.session.cache.trim_active_optional(obsolete_only))
    }
    pub(super) fn evict_oldest(&mut self) -> Option<u64> {
        (!self.entries.is_empty()).then(|| {
            let entry = self.entries.remove(0);
            crate::diagnostics::event(
                crate::diagnostics::Category::Cache,
                "cache.processing.entry_released",
                |fields| {
                    fields.u64("session", entry.id);
                    fields.u64("gpu_bytes", entry.session.cached_bytes());
                },
            );
            entry.id
        })
    }
    /// Deduplicated total of active, history and Engine preparation. Delivered images and scratch are separate.
    pub fn retained_bytes(&self, engine: &Engine) -> u64 {
        let mut retained = crate::recipe::RetainedBytes::default();
        engine.account_preparation(&mut retained);
        self.retained_gpu_bytes() + Self::account_cpu_for(self.all(), &mut retained)
    }
    /// Native/Smart callers park the reduced owner before taking over the active slot.
    pub fn park_active(&mut self) -> Vec<u64> {
        if let Some(mut entry) = self.active.take() {
            entry.session.cache.park();
            self.entries.retain(|old| old.id != entry.id);
            self.entries.push(entry);
        }
        self.enforce()
    }
    fn activate(&mut self, id: u64) -> bool {
        if self.active.as_ref().is_some_and(|entry| entry.id == id) {
            return true;
        }
        let Some(index) = self.entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        let entry = self.entries.remove(index);
        self.park_active();
        self.active = Some(entry);
        true
    }
    pub fn insert(
        &mut self,
        id: u64,
        session: Box<ReducedSession>,
        engine: &Engine,
    ) -> Result<Vec<u64>> {
        if !Arc::ptr_eq(&session.owner, &engine.owner) {
            return Err("reduced inputs belong to another engine".into());
        }
        self.remove(id);
        let evicted = self.park_active();
        self.active = Some(Entry { id, session });
        Ok(evicted)
    }
    /// Speculative loads enter colder than photographs that have actually been
    /// edited. They never replace or promote the active photograph.
    pub fn prefetch(
        &mut self,
        id: u64,
        session: Box<ReducedSession>,
        engine: &Engine,
    ) -> Result<Vec<u64>> {
        if !Arc::ptr_eq(&session.owner, &engine.owner) {
            return Err("reduced inputs belong to another engine".into());
        }
        if self.contains(id) {
            return Ok(Vec::new());
        }
        self.entries.insert(0, Entry { id, session });
        Ok(self.enforce())
    }
    /// Refresh only a verified original. Validation precedes active/history ownership changes.
    pub fn refresh(
        &mut self,
        engine: &Engine,
        id: u64,
        source: &Source,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<Option<()>> {
        let Some(entry) = self.all().find(|entry| entry.id == id) else {
            return Ok(None);
        };
        if entry.session.origin != *source.origin() {
            return Err("refreshed source is not the same verified original".into());
        }
        if !Arc::ptr_eq(&entry.session.owner, &engine.owner) {
            return Err("camera dependencies belong to another engine".into());
        }
        self.activate(id);
        let result = engine.refresh_reduced_session(
            &mut self.active.as_mut().unwrap().session,
            source,
            settings,
            cancel,
            profile,
        );
        self.enforce();
        result
    }
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.len();
        if self.active.as_ref().is_some_and(|entry| entry.id == id) {
            self.active = None;
        }
        self.entries.retain(|entry| entry.id != id);
        self.len() != before
    }
    pub fn render(
        &mut self,
        engine: &Engine,
        id: u64,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<Option<ReducedRender>> {
        self.render_observed(engine, id, settings, cancel, profile, None)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_with_progress(
        &mut self,
        engine: &Engine,
        id: u64,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: impl Fn(super::RenderProgress),
    ) -> Result<Option<ReducedRender>> {
        self.render_observed(engine, id, settings, cancel, profile, Some(&progress))
    }
    #[allow(clippy::too_many_arguments)]
    fn render_observed(
        &mut self,
        engine: &Engine,
        id: u64,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: Option<&dyn Fn(super::RenderProgress)>,
    ) -> Result<Option<ReducedRender>> {
        if !self.activate(id) {
            return Ok(None);
        }
        let result = engine.render_reduced_session_observed(
            &mut self.active.as_mut().unwrap().session,
            settings,
            &["final"],
            cancel,
            profile,
            progress,
        );
        self.enforce();
        result.map(Some)
    }
    fn enforce(&mut self) -> Vec<u64> {
        let mut evicted = Vec::new();
        // Preserve photos before optional cached versions. Pass one removes old
        // alternatives everywhere; pass two can shed less useful current
        // frontiers, still keeping each photo's committed immutable inputs.
        for obsolete_only in [true, false] {
            while self.historical_bytes() > self.limit {
                let trimmed = self.entries.iter_mut().any(|entry| {
                    let trimmed = entry.session.cache.trim_optional(obsolete_only);
                    if trimmed {
                        entry.session.cache.park();
                    }
                    trimmed
                });
                if !trimmed {
                    break;
                }
            }
        }
        while !self.entries.is_empty() && self.historical_bytes() > self.limit {
            let id = self.evict_oldest().expect("nonempty reduced history");
            crate::diagnostics::event(
                crate::diagnostics::Category::Cache,
                "cache.processing.evicted",
                |fields| {
                    fields.u64("session", id);
                    fields.str("reason", "historical_memory_budget");
                    fields.u64("budget_bytes", self.limit);
                },
            );
            evicted.push(id);
        }
        evicted
    }
}

#[cfg(test)]
#[path = "reduced_test.rs"]
mod tests;
