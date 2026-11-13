//! Retains native and reduced processing sessions in one foreground-aware history.

use super::{Engine, ReducedRender, ReducedSession, RenderSession, Settings};
use crate::{
    profile::Profile,
    recipe::RetainedBytes,
    source::{Metadata, Result, Source, SourceOrigin},
};
use std::sync::Arc;

/// The source representation retained for a photograph's processing session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub enum SessionKind {
    Native,
    Reduced,
}

/// A photograph may own one session of each kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub struct SessionKey {
    pub photo: u64,
    pub kind: SessionKind,
}

/// Only work demanded by the user promotes and protects a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionUse {
    Foreground,
    Preparation,
}

#[allow(clippy::large_enum_variant)] // Native sessions avoid a separate allocation.
enum Session {
    Native(RenderSession),
    Reduced(Box<ReducedSession>),
}
struct Entry {
    key: SessionKey,
    session: Session,
}

/// One foreground session and a cold-to-hot history, governed by the shared memory budget.
#[derive(Default)]
pub struct ProcessingWorkingSet {
    entries: Vec<Entry>,
    active: Option<SessionKey>,
}
impl ProcessingWorkingSet {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn active_key(&self) -> Option<SessionKey> {
        self.active
    }
    pub fn contains(&self, key: SessionKey) -> bool {
        self.entries.iter().any(|entry| entry.key == key)
    }
    fn key(photo: u64, kind: SessionKind) -> SessionKey {
        SessionKey { photo, kind }
    }
    fn get(&self, key: SessionKey) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.key == key)
    }
    fn get_mut(&mut self, key: SessionKey) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|entry| entry.key == key)
    }
    pub fn native(&self, photo: u64) -> Option<&RenderSession> {
        match &self.get(Self::key(photo, SessionKind::Native))?.session {
            Session::Native(session) => Some(session),
            _ => None,
        }
    }
    pub fn native_mut(&mut self, photo: u64) -> Option<&mut RenderSession> {
        match &mut self.get_mut(Self::key(photo, SessionKind::Native))?.session {
            Session::Native(session) => Some(session),
            _ => None,
        }
    }
    pub fn reduced(&self, photo: u64) -> Option<&ReducedSession> {
        match &self.get(Self::key(photo, SessionKind::Reduced))?.session {
            Session::Reduced(session) => Some(session),
            _ => None,
        }
    }
    pub fn reduced_mut(&mut self, photo: u64) -> Option<&mut ReducedSession> {
        match &mut self
            .get_mut(Self::key(photo, SessionKind::Reduced))?
            .session
        {
            Session::Reduced(session) => Some(session),
            _ => None,
        }
    }
    /// A preparation may use an existing session without changing foreground ownership or recency.
    pub fn activate(&mut self, key: SessionKey, usage: SessionUse) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.key == key) else {
            return false;
        };
        if usage == SessionUse::Preparation {
            return true;
        }
        if self.active != Some(key) {
            self.park_active();
        }
        let entry = self.entries.remove(index);
        self.entries.push(entry);
        self.active = Some(key);
        true
    }
    pub fn park_active(&mut self) {
        if let Some(key) = self.active.take() {
            if let Some(entry) = self.get_mut(key) {
                entry.session.cache_mut().park();
            }
        }
    }
    fn insert(&mut self, key: SessionKey, session: Session, usage: SessionUse) {
        if usage == SessionUse::Preparation && self.contains(key) {
            return;
        }
        self.remove(key);
        if usage == SessionUse::Foreground {
            self.park_active();
            self.entries.push(Entry { key, session });
            self.active = Some(key);
        } else {
            self.entries.insert(0, Entry { key, session });
        }
    }
    pub fn insert_native(
        &mut self,
        photo: u64,
        mut session: RenderSession,
        engine: &Engine,
        usage: SessionUse,
    ) {
        session.cache.bind(&engine.owner);
        self.insert(
            Self::key(photo, SessionKind::Native),
            Session::Native(session),
            usage,
        );
    }
    pub fn insert_reduced(
        &mut self,
        photo: u64,
        session: Box<ReducedSession>,
        engine: &Engine,
        usage: SessionUse,
    ) -> Result<()> {
        if !Arc::ptr_eq(&session.owner, &engine.owner) {
            return Err("reduced inputs belong to another engine".into());
        }
        self.insert(
            Self::key(photo, SessionKind::Reduced),
            Session::Reduced(session),
            usage,
        );
        Ok(())
    }
    pub fn remove(&mut self, key: SessionKey) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.key != key);
        if self.active == Some(key) {
            self.active = None;
        }
        self.entries.len() != before
    }
    pub fn remove_photo(&mut self, photo: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.key.photo != photo);
        if self.active.is_some_and(|key| key.photo == photo) {
            self.active = None;
        }
        self.entries.len() != before
    }
    pub fn reduced_settings(&self, photo: u64) -> Option<&Settings> {
        self.reduced(photo)
            .and_then(|session| session.last_settings.as_deref())
    }
    pub fn reduced_bytes(&self, photo: u64) -> Option<(u64, u64)> {
        self.reduced(photo)
            .map(|session| (session.cached_bytes(), session.retained_cpu_bytes()))
    }
    pub fn reduced_resources(&self, photo: u64) -> Option<super::SessionResources> {
        self.reduced(photo).map(ReducedSession::resources)
    }
    pub fn active_bytes(&self) -> u64 {
        self.active
            .and_then(|key| self.get(key))
            .map_or(0, |entry| {
                entry.session.retained_cpu_bytes() + entry.session.cached_bytes()
            })
    }
    pub fn historical_bytes(&self) -> u64 {
        let entries = self
            .entries
            .iter()
            .filter(|entry| Some(entry.key) != self.active);
        let mut retained = RetainedBytes::default();
        let containers: u64 = entries
            .clone()
            .map(|entry| entry.session.account_memory(&mut retained))
            .sum();
        containers
            + retained.bytes() as u64
            + super::reduced::unique_buffer_bytes(entries.flat_map(|entry| entry.session.buffers()))
    }
    pub fn retained_bytes(&self, engine: &Engine) -> u64 {
        engine
            .memory_resources(self, &super::MemoryReservation::default())
            .total_bytes
    }
    pub(super) fn account_memory(&self, retained: &mut RetainedBytes) -> u64 {
        self.entries
            .iter()
            .map(|entry| entry.session.account_memory(retained))
            .sum()
    }
    pub(super) fn memory_buffers(&self) -> impl Iterator<Item = &crate::gpu::ReadGuard> {
        self.entries
            .iter()
            .flat_map(|entry| entry.session.buffers())
    }
    pub(super) fn active_metadata(&self) -> Option<&Arc<Metadata>> {
        self.get(self.active?).map(|entry| entry.session.metadata())
    }
    pub(super) fn active_origin(&self) -> Option<&SourceOrigin> {
        self.get(self.active?).map(|entry| entry.session.origin())
    }
    pub(super) fn trim_optional(&mut self, obsolete_only: bool) -> bool {
        let active_key = self.active;
        self.entries.iter_mut().any(|entry| {
            let active = Some(entry.key) == active_key;
            let cache = entry.session.cache_mut();
            let trimmed = if active {
                cache.trim_active_optional(obsolete_only)
            } else {
                cache.trim_optional(obsolete_only)
            };
            if trimmed && !active {
                cache.park();
            }
            trimmed
        })
    }
    pub(super) fn evict_oldest(&mut self) -> Option<SessionKey> {
        let index = self
            .entries
            .iter()
            .position(|entry| Some(entry.key) != self.active)?;
        let entry = self.entries.remove(index);
        crate::diagnostics::event(
            crate::diagnostics::Category::Cache,
            "cache.processing.entry_released",
            |fields| {
                fields
                    .u64("session", entry.key.photo)
                    .str(
                        "kind",
                        match entry.key.kind {
                            SessionKind::Native => "native",
                            SessionKind::Reduced => "reduced",
                        },
                    )
                    .u64("gpu_bytes", entry.session.cached_bytes());
            },
        );
        Some(entry.key)
    }
    /// Refresh only a verified original, without changing use order for preparation.
    #[allow(clippy::too_many_arguments)]
    pub fn refresh_reduced(
        &mut self,
        engine: &Engine,
        photo: u64,
        source: &Source,
        settings: &Settings,
        usage: SessionUse,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<Option<()>> {
        let Some(session) = self.reduced(photo) else {
            return Ok(None);
        };
        if session.origin != *source.origin() {
            return Err("refreshed source is not the same verified original".into());
        }
        if !Arc::ptr_eq(&session.owner, &engine.owner) {
            return Err("camera dependencies belong to another engine".into());
        }
        self.activate(Self::key(photo, SessionKind::Reduced), usage);
        engine.refresh_reduced_session(
            self.reduced_mut(photo).unwrap(),
            source,
            settings,
            cancel,
            profile,
        )
    }
    pub fn render_reduced(
        &mut self,
        engine: &Engine,
        photo: u64,
        settings: &Settings,
        usage: SessionUse,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<Option<ReducedRender>> {
        self.render_observed(engine, photo, settings, usage, cancel, profile, None)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_reduced_with_progress(
        &mut self,
        engine: &Engine,
        photo: u64,
        settings: &Settings,
        usage: SessionUse,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: impl Fn(super::RenderProgress),
    ) -> Result<Option<ReducedRender>> {
        self.render_observed(
            engine,
            photo,
            settings,
            usage,
            cancel,
            profile,
            Some(&progress),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn render_observed(
        &mut self,
        engine: &Engine,
        photo: u64,
        settings: &Settings,
        usage: SessionUse,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: Option<&dyn Fn(super::RenderProgress)>,
    ) -> Result<Option<ReducedRender>> {
        if !self.activate(Self::key(photo, SessionKind::Reduced), usage) {
            return Ok(None);
        }
        engine
            .render_reduced_session_observed(
                self.reduced_mut(photo).unwrap(),
                settings,
                &["final"],
                cancel,
                profile,
                progress,
            )
            .map(Some)
    }
}
impl Session {
    fn metadata(&self) -> &Arc<Metadata> {
        match self {
            Self::Native(s) => s.metadata(),
            Self::Reduced(s) => s.metadata(),
        }
    }
    fn origin(&self) -> &SourceOrigin {
        match self {
            Self::Native(s) => s.source().origin(),
            Self::Reduced(s) => &s.origin,
        }
    }
    fn cache_mut(&mut self) -> &mut crate::interactive::Cache {
        match self {
            Self::Native(s) => &mut s.cache,
            Self::Reduced(s) => &mut s.cache,
        }
    }
    fn cached_bytes(&self) -> u64 {
        match self {
            Self::Native(s) => s.cached_bytes(),
            Self::Reduced(s) => s.cached_bytes(),
        }
    }
    fn retained_cpu_bytes(&self) -> u64 {
        match self {
            Self::Native(s) => s.retained_cpu_bytes(),
            Self::Reduced(s) => s.retained_cpu_bytes(),
        }
    }
    fn account_memory(&self, retained: &mut RetainedBytes) -> u64 {
        match self {
            Self::Native(s) => s.account_memory(retained),
            Self::Reduced(s) => {
                retained.owner(s.metadata.clone(), s.metadata.retained_bytes() as usize);
                s.owned_cpu_bytes() - s.metadata.retained_bytes() + s.account_keys(retained) as u64
            }
        }
    }
    fn cache(&self) -> &crate::interactive::Cache {
        match self {
            Self::Native(s) => &s.cache,
            Self::Reduced(s) => &s.cache,
        }
    }
    fn buffers(&self) -> impl Iterator<Item = &crate::gpu::ReadGuard> {
        self.cache().buffers()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Metadata, SourceStorage};

    fn source() -> Arc<Source> {
        Arc::new(Source {
            origin: Default::default(),
            metadata: Arc::new(Metadata {
                width: 1,
                height: 1,
                rotation: 0,
                processing_version: None,
                properties: Default::default(),
                matrices: Default::default(),
                lists: Default::default(),
            }),
            pixels: vec![1., 2., 3.],
            storage: SourceStorage::InterleavedRgb,
            camf: vec![],
            timings: vec![],
        })
    }

    #[test]
    fn preparation_does_not_promote_foreground_or_pin_cold_history() {
        let source = source();
        let native = |photo| SessionKey {
            photo,
            kind: SessionKind::Native,
        };
        let mut working = ProcessingWorkingSet::new();
        working.insert(
            native(1),
            Session::Native(RenderSession::new(source.clone())),
            SessionUse::Foreground,
        );
        working.insert(
            native(2),
            Session::Native(RenderSession::new(source.clone())),
            SessionUse::Foreground,
        );
        working.insert(
            native(3),
            Session::Native(RenderSession::new(source.clone())),
            SessionUse::Preparation,
        );
        assert!(working.activate(native(1), SessionUse::Preparation));
        assert_eq!(working.active_key(), Some(native(2)));
        assert_eq!(working.evict_oldest(), Some(native(3)));
        assert_eq!(working.evict_oldest(), Some(native(1)));
        assert!(working.contains(native(2)));
        assert_eq!(working.evict_oldest(), None);
        working.park_active();
        assert_eq!(working.evict_oldest(), Some(native(2)));
    }

    #[test]
    fn foreground_reactivation_changes_eviction_order() {
        let source = source();
        let native = |photo| SessionKey {
            photo,
            kind: SessionKind::Native,
        };
        let mut working = ProcessingWorkingSet::new();
        for photo in [1, 2, 3] {
            working.insert(
                native(photo),
                Session::Native(RenderSession::new(source.clone())),
                SessionUse::Foreground,
            );
        }
        assert!(working.activate(native(1), SessionUse::Foreground));
        assert_eq!(working.evict_oldest(), Some(native(2)));
        assert_eq!(working.evict_oldest(), Some(native(3)));
        assert_eq!(working.evict_oldest(), None);
        working.park_active();
        assert_eq!(working.evict_oldest(), Some(native(1)));
    }
}
