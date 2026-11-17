//! Retains native and reduced sessions with separate foreground recency and execution protection.

use super::{Engine, ReducedSession, RenderSession, SessionRender, Settings};
use crate::{
    profile::Profile,
    recipe::RetainedBytes,
    source::{Metadata, Result, Source, SourceOrigin},
};
use std::sync::{Arc, Weak};

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

/// Foreground use promotes a session and protects its retained restart inputs.
/// Execution claims protect running sessions independently of this recency policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionUse {
    Foreground,
    Preparation,
}

#[allow(clippy::large_enum_variant)] // Native sessions avoid a separate allocation.
pub(super) enum Session {
    Native(RenderSession),
    Reduced(Box<ReducedSession>),
}
struct Entry {
    key: SessionKey,
    session: Session,
    execution: Option<Weak<()>>,
    park_pending: bool,
}

impl Entry {
    fn executing(&self) -> bool {
        self.execution
            .as_ref()
            .is_some_and(|execution| execution.strong_count() != 0)
    }

    fn park(&mut self) {
        if self.executing() {
            self.park_pending = true;
        } else {
            self.session.cache_mut().park();
            self.park_pending = false;
        }
    }
}

/// An executing session must retire before its owner is replaced or cleared.
#[derive(Debug)]
pub struct SessionInUse {
    pub key: SessionKey,
}

impl std::fmt::Display for SessionInUse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "processing session {:?} is executing", self.key)
    }
}

impl std::error::Error for SessionInUse {}

/// The entry keeps only a weak identity so errors release protection naturally.
/// The execution owner must drain submitted work before dropping this claim.
#[derive(Debug)]
pub(super) struct ExecutionClaim {
    key: SessionKey,
    identity: Arc<()>,
}

/// One foreground session and independently protected executions share the same history and budget.
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
    fn require_idle(&self, photo: u64) -> Result<()> {
        if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.key.photo == photo && entry.executing())
        {
            return Err(Box::new(SessionInUse { key: entry.key }));
        }
        Ok(())
    }
    pub(super) fn retire_executions(&mut self) {
        for entry in &mut self.entries {
            if entry.execution.is_some() && !entry.executing() {
                entry.execution = None;
                if entry.park_pending && Some(entry.key) != self.active {
                    entry.park();
                }
                entry.park_pending = false;
            }
        }
    }
    pub(super) fn claim_execution(&mut self, key: SessionKey) -> Result<ExecutionClaim> {
        self.require_idle(key.photo)?;
        self.retire_executions();
        let entry = self
            .get_mut(key)
            .ok_or("processing session is not retained")?;
        let identity = Arc::new(());
        entry.execution = Some(Arc::downgrade(&identity));
        Ok(ExecutionClaim { key, identity })
    }
    pub(super) fn execution(&self, claim: &ExecutionClaim) -> &Session {
        let entry = self.get(claim.key).expect("claimed session is retained");
        assert!(entry
            .execution
            .as_ref()
            .is_some_and(|identity| identity.ptr_eq(&Arc::downgrade(&claim.identity))));
        &entry.session
    }
    pub(super) fn execution_mut(&mut self, claim: &ExecutionClaim) -> &mut Session {
        let entry = self
            .get_mut(claim.key)
            .expect("claimed session is retained");
        assert!(entry
            .execution
            .as_ref()
            .is_some_and(|identity| identity.ptr_eq(&Arc::downgrade(&claim.identity))));
        &mut entry.session
    }
    pub fn native(&self, photo: u64) -> Option<&RenderSession> {
        match &self.get(Self::key(photo, SessionKind::Native))?.session {
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
    /// A preparation may use an existing session without changing foreground ownership or recency.
    pub fn activate(&mut self, key: SessionKey, usage: SessionUse) -> bool {
        self.retire_executions();
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
        self.entries.last_mut().unwrap().park_pending = false;
        true
    }
    pub fn park_active(&mut self) {
        self.retire_executions();
        if let Some(key) = self.active.take() {
            if let Some(entry) = self.get_mut(key) {
                entry.park();
            }
        }
    }
    fn insert(&mut self, key: SessionKey, session: Session, usage: SessionUse) -> Result<()> {
        self.require_idle(key.photo)?;
        if usage == SessionUse::Preparation && self.contains(key) {
            return Ok(());
        }
        self.remove(key)?;
        let entry = Entry {
            key,
            session,
            execution: None,
            park_pending: false,
        };
        if usage == SessionUse::Foreground {
            self.park_active();
            self.entries.push(entry);
            self.active = Some(key);
        } else {
            self.entries.insert(0, entry);
        }
        Ok(())
    }
    pub fn insert_native(
        &mut self,
        photo: u64,
        mut session: RenderSession,
        engine: &Engine,
        usage: SessionUse,
    ) -> Result<()> {
        session.cache.bind(&engine.owner);
        self.insert(
            Self::key(photo, SessionKind::Native),
            Session::Native(session),
            usage,
        )
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
        )
    }
    /// Removal is rejected while either representation of this photo executes.
    pub fn remove(&mut self, key: SessionKey) -> Result<bool> {
        self.require_idle(key.photo)?;
        self.retire_executions();
        let before = self.entries.len();
        self.entries.retain(|entry| entry.key != key);
        if self.active == Some(key) {
            self.active = None;
        }
        Ok(self.entries.len() != before)
    }
    /// Check both representations before removing either one.
    pub fn remove_photo(&mut self, photo: u64) -> Result<bool> {
        self.require_idle(photo)?;
        self.retire_executions();
        let before = self.entries.len();
        self.entries.retain(|entry| entry.key.photo != photo);
        if self.active.is_some_and(|key| key.photo == photo) {
            self.active = None;
        }
        Ok(self.entries.len() != before)
    }
    /// The execution owner must drain and retire all claims before clearing.
    pub fn clear(&mut self) -> Result<()> {
        if let Some(entry) = self.entries.iter().find(|entry| entry.executing()) {
            return Err(Box::new(SessionInUse { key: entry.key }));
        }
        self.entries.clear();
        self.active = None;
        Ok(())
    }
    /// Execute one synchronous native operation under session protection.
    /// The operation must finish its submitted work before returning.
    pub fn with_native<T>(
        &mut self,
        photo: u64,
        operation: impl FnOnce(&mut RenderSession) -> Result<T>,
    ) -> Result<Option<T>> {
        let key = Self::key(photo, SessionKind::Native);
        if !self.contains(key) {
            return Ok(None);
        }
        let claim = self.claim_execution(key)?;
        let Session::Native(session) = self.execution_mut(&claim) else {
            unreachable!()
        };
        let result = operation(session);
        drop(claim);
        self.retire_executions();
        result.map(Some)
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
        let entries = self.entries.capacity() * size_of::<Entry>();
        entries as u64
            + self
                .entries
                .iter()
                .map(|entry| {
                    let inline = match &entry.session {
                        Session::Native(_) => size_of::<RenderSession>() as u64,
                        Session::Reduced(_) => 0,
                    };
                    entry.session.account_memory(retained) - inline
                })
                .sum::<u64>()
    }
    pub(super) fn memory_buffers(&self) -> impl Iterator<Item = &crate::gpu::ReadGuard> {
        self.entries
            .iter()
            .flat_map(|entry| entry.session.buffers())
    }
    pub(super) fn protected_inputs(&self) -> impl Iterator<Item = (&Arc<Metadata>, &SourceOrigin)> {
        self.entries
            .iter()
            .filter(|entry| Some(entry.key) == self.active || entry.executing())
            .map(|entry| (entry.session.metadata(), entry.session.origin()))
    }
    pub(super) fn trim_optional(&mut self, obsolete_only: bool) -> bool {
        self.retire_executions();
        let active_key = self.active;
        self.entries.iter_mut().any(|entry| {
            if entry.executing() {
                return false;
            }
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
        self.retire_executions();
        let index = self
            .entries
            .iter()
            .position(|entry| Some(entry.key) != self.active && !entry.executing())?;
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
        let claim = self.claim_execution(Self::key(photo, SessionKind::Reduced))?;
        self.activate(claim.key, usage);
        let Session::Reduced(session) = self.execution_mut(&claim) else {
            unreachable!()
        };
        let result = engine.refresh_reduced_session(session, source, settings, cancel, profile);
        drop(claim);
        self.retire_executions();
        result
    }
    pub fn render_reduced(
        &mut self,
        engine: &Engine,
        photo: u64,
        settings: &Settings,
        usage: SessionUse,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<Option<SessionRender>> {
        self.render_observed(engine, photo, settings, usage, cancel, profile, None, None)
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
        budget: Option<super::ExecutionBudget<'_>>,
    ) -> Result<Option<SessionRender>> {
        self.render_observed(
            engine,
            photo,
            settings,
            usage,
            cancel,
            profile,
            Some(&progress),
            budget,
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
        budget: Option<super::ExecutionBudget<'_>>,
    ) -> Result<Option<SessionRender>> {
        super::driver::render_session(
            engine,
            self,
            Self::key(photo, SessionKind::Reduced),
            settings,
            &["final"],
            usage,
            &cancel,
            profile,
            progress,
            budget,
        )
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
    pub(super) fn cache_mut(&mut self) -> &mut crate::interactive::Cache {
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
    pub(super) fn cache(&self) -> &crate::interactive::Cache {
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

    fn native(photo: u64) -> SessionKey {
        SessionKey {
            photo,
            kind: SessionKind::Native,
        }
    }

    fn retained_photos() -> ProcessingWorkingSet {
        let mut working = ProcessingWorkingSet::new();
        for photo in [1, 2, 3] {
            working
                .insert(
                    native(photo),
                    Session::Native(RenderSession::new(source())),
                    SessionUse::Preparation,
                )
                .unwrap();
        }
        working
    }

    #[test]
    fn execution_claims_protect_several_photos_without_changing_foreground() {
        let mut working = retained_photos();
        working.activate(native(3), SessionUse::Foreground);
        let first = working.claim_execution(native(1)).unwrap();
        let second = working.claim_execution(native(2)).unwrap();
        assert_eq!(working.active_key(), Some(native(3)));
        assert_eq!(working.protected_inputs().count(), 3);
        working.park_active();
        assert_eq!(working.protected_inputs().count(), 2);
        assert_eq!(working.evict_oldest(), Some(native(3)));
        assert_eq!(working.evict_oldest(), None);
        drop(first);
        assert_eq!(working.evict_oldest(), Some(native(1)));
        drop(second);
        assert_eq!(working.evict_oldest(), Some(native(2)));
    }

    #[test]
    fn busy_mutations_leave_source_identity_and_other_entries_unchanged() {
        let mut working = retained_photos();
        working.activate(native(2), SessionUse::Foreground);
        let original = working.native(1).unwrap().shared_source();
        let claim = working.claim_execution(native(1)).unwrap();
        assert!(working
            .claim_execution(native(1))
            .unwrap_err()
            .is::<SessionInUse>());
        assert!(working
            .with_native(1, |_| Ok(()))
            .unwrap_err()
            .is::<SessionInUse>());
        assert!(working.remove(native(1)).unwrap_err().is::<SessionInUse>());
        assert!(working.remove_photo(1).unwrap_err().is::<SessionInUse>());
        assert!(working.clear().unwrap_err().is::<SessionInUse>());
        assert!(working
            .insert(
                native(1),
                Session::Native(RenderSession::new(source())),
                SessionUse::Foreground,
            )
            .unwrap_err()
            .is::<SessionInUse>());
        assert!(Arc::ptr_eq(
            &original,
            &working.native(1).unwrap().shared_source()
        ));
        assert_eq!(working.active_key(), Some(native(2)));
        assert_eq!(working.len(), 3);
        assert!(working.native(2).is_some());
        assert!(working.native(3).is_some());
        drop(claim);
        assert!(working.remove_photo(1).unwrap());
        working.clear().unwrap();
        assert!(working.is_empty());
    }

    #[test]
    fn parking_waits_for_execution_retirement() {
        use crate::{
            graph::{DataType, Graph, Operation},
            interactive::Execution,
        };
        let mut working = retained_photos();
        let mut graph = Graph::default();
        let output = graph
            .add(
                Operation::RenderedInput {
                    data: DataType::SensorLayers,
                },
                &[],
            )
            .unwrap();
        let name = graph.outputs[output].cache_name();
        working
            .with_native(1, |session| {
                Execution {
                    cache: Some(&mut session.cache),
                    ..Default::default()
                }
                .plan(&graph, &[output])
            })
            .unwrap()
            .unwrap();
        assert!(working
            .native(1)
            .unwrap()
            .cache
            .diagnostic_version(&name)
            .is_some());
        working.activate(native(1), SessionUse::Foreground);
        let claim = working.claim_execution(native(1)).unwrap();
        working.activate(native(2), SessionUse::Foreground);
        assert!(working
            .native(1)
            .unwrap()
            .cache
            .diagnostic_version(&name)
            .is_some());
        drop(claim);
        assert!(working
            .with_native(1, |session| Ok(session.cache.diagnostic_version(&name)))
            .unwrap()
            .unwrap()
            .is_none());
    }

    #[test]
    fn native_operation_releases_protection_after_each_outcome() {
        let mut working = retained_photos();
        assert_eq!(working.with_native(1, |_| Ok(42)).unwrap(), Some(42));
        assert!(working.claim_execution(native(1)).is_ok());
        assert_eq!(
            working.with_native(1, |_| Ok(None::<()>)).unwrap(),
            Some(None)
        );
        let failed: Result<Option<()>> = working.with_native(1, |_| Err("operation failed".into()));
        assert_eq!(failed.unwrap_err().to_string(), "operation failed");
        assert!(working.remove_photo(1).unwrap());
        assert!(working.with_native(1, |_| Ok(())).unwrap().is_none());
        working.clear().unwrap();
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn claimed_sessions_keep_checkpoints_and_accounting_under_pressure() {
        use crate::{
            inspection::Content,
            render::{ReducedPreparation, Scale},
        };
        let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let engine = Engine::new().unwrap();
        let mut working = ProcessingWorkingSet::new();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.working_scale = Scale::new(8).unwrap();
        let mut images = Vec::new();
        for (photo, name) in [(1, "blue-parking-sign.x3f"), (2, "merrill-hazy-coast.x3f")] {
            let source = Arc::new(Source::open(root.join(name)).unwrap());
            let ReducedPreparation::Prepared(session) = engine
                .prepare_reduced_session(
                    source.clone(),
                    &settings,
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
            else {
                panic!("prepared inputs");
            };
            working
                .insert_reduced(photo, session, &engine, SessionUse::Foreground)
                .unwrap();
            let Some(SessionRender::Rendered(artifacts)) = working
                .render_reduced(
                    &engine,
                    photo,
                    &settings,
                    SessionUse::Foreground,
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
            else {
                panic!("rendered image");
            };
            let Content::Image(image) = artifacts.into_iter().next().unwrap().content else {
                panic!("image");
            };
            images.push(engine.readback_image(&image).unwrap());
            if photo == 1 {
                working
                    .insert_native(
                        3,
                        RenderSession::new(source),
                        &engine,
                        SessionUse::Preparation,
                    )
                    .unwrap();
            }
        }
        let key = |photo| SessionKey {
            photo,
            kind: SessionKind::Reduced,
        };
        let buffers = |working: &ProcessingWorkingSet, photo| {
            working
                .reduced(photo)
                .unwrap()
                .cache
                .buffers()
                .map(|buffer| (buffer.identity(), buffer.size()))
                .collect::<Vec<_>>()
        };
        let before = engine.memory_resources(&working, &Default::default());
        let expected = [buffers(&working, 1), buffers(&working, 2)];
        let first = working.claim_execution(key(1)).unwrap();
        let second = working.claim_execution(key(2)).unwrap();
        assert_eq!(
            engine.memory_resources(&working, &Default::default()),
            before
        );
        working.park_active();
        let trim = engine.trim_memory(&mut working, 0, &Default::default());
        assert_eq!(trim.evicted_sessions, vec![native(3)]);
        assert!(trim.over_budget_bytes > 0);
        assert_eq!(buffers(&working, 1), expected[0]);
        assert_eq!(buffers(&working, 2), expected[1]);
        for (index, claim) in [&first, &second].into_iter().enumerate() {
            let Session::Reduced(session) = working.execution_mut(claim) else {
                unreachable!()
            };
            let SessionRender::Rendered(artifacts) = engine
                .render_reduced_session(
                    session,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
            else {
                panic!("source-free retained render");
            };
            let Content::Image(image) = artifacts.into_iter().next().unwrap().content else {
                panic!("image");
            };
            assert_eq!(engine.readback_image(&image).unwrap(), images[index]);
        }
        drop(first);
        drop(second);
        assert!(matches!(
            working
                .render_reduced(
                    &engine,
                    1,
                    &settings,
                    SessionUse::Preparation,
                    || true,
                    &mut Profile::default(),
                )
                .unwrap(),
            Some(SessionRender::Superseded)
        ));
        assert!(working.claim_execution(key(1)).is_ok());
        let mut unavailable = settings.clone();
        unavailable.color_mode = "unavailable-test-color-mode".into();
        assert!(working
            .render_reduced(
                &engine,
                1,
                &unavailable,
                SessionUse::Preparation,
                || false,
                &mut Profile::default(),
            )
            .is_err());
        assert!(working.claim_execution(key(1)).is_ok());
        assert_eq!(buffers(&working, 1), expected[0]);
        let Some(SessionRender::Rendered(artifacts)) = working
            .render_reduced(
                &engine,
                1,
                &settings,
                SessionUse::Preparation,
                || false,
                &mut Profile::default(),
            )
            .unwrap()
        else {
            panic!("retry keeps prior inputs");
        };
        let Content::Image(image) = artifacts.into_iter().next().unwrap().content else {
            panic!("image");
        };
        assert_eq!(engine.readback_image(&image).unwrap(), images[0]);
        let trim = engine.trim_memory(&mut working, 0, &Default::default());
        assert_eq!(trim.evicted_sessions.len(), 2);
        assert!(working.is_empty());
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn one_photo_cannot_execute_both_representations_or_be_partly_invalidated() {
        use crate::render::{ReducedPreparation, Scale};
        let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f");
        let source = Arc::new(Source::open(path).unwrap());
        let engine = Engine::new().unwrap();
        let mut working = ProcessingWorkingSet::new();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.working_scale = Scale::new(8).unwrap();
        working
            .insert_native(
                1,
                RenderSession::new(source.clone()),
                &engine,
                SessionUse::Foreground,
            )
            .unwrap();
        let ReducedPreparation::Prepared(session) = engine
            .prepare_reduced_session(source, &settings, || false, &mut Profile::default())
            .unwrap()
        else {
            panic!("prepared inputs");
        };
        working
            .insert_reduced(1, session, &engine, SessionUse::Preparation)
            .unwrap();
        let claim = working.claim_execution(native(1)).unwrap();
        let reduced = SessionKey {
            photo: 1,
            kind: SessionKind::Reduced,
        };
        assert!(working
            .claim_execution(reduced)
            .unwrap_err()
            .is::<SessionInUse>());
        assert!(working
            .render_reduced(
                &engine,
                1,
                &settings,
                SessionUse::Foreground,
                || false,
                &mut Profile::default()
            )
            .err()
            .unwrap()
            .is::<SessionInUse>());
        assert_eq!(working.active_key(), Some(native(1)));
        assert!(working.remove_photo(1).unwrap_err().is::<SessionInUse>());
        assert!(working.clear().unwrap_err().is::<SessionInUse>());
        assert!(working.native(1).is_some());
        assert!(working.reduced(1).is_some());
        drop(claim);
        assert!(working.remove_photo(1).unwrap());
        assert!(working.is_empty());
    }

    #[test]
    fn preparation_does_not_promote_foreground_or_pin_cold_history() {
        let source = source();
        let native = |photo| SessionKey {
            photo,
            kind: SessionKind::Native,
        };
        let mut working = ProcessingWorkingSet::new();
        working
            .insert(
                native(1),
                Session::Native(RenderSession::new(source.clone())),
                SessionUse::Foreground,
            )
            .unwrap();
        working
            .insert(
                native(2),
                Session::Native(RenderSession::new(source.clone())),
                SessionUse::Foreground,
            )
            .unwrap();
        working
            .insert(
                native(3),
                Session::Native(RenderSession::new(source.clone())),
                SessionUse::Preparation,
            )
            .unwrap();
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
            working
                .insert(
                    native(photo),
                    Session::Native(RenderSession::new(source.clone())),
                    SessionUse::Foreground,
                )
                .unwrap();
        }
        assert!(working.activate(native(1), SessionUse::Foreground));
        assert_eq!(working.evict_oldest(), Some(native(2)));
        assert_eq!(working.evict_oldest(), Some(native(3)));
        assert_eq!(working.evict_oldest(), None);
        working.park_active();
        assert_eq!(working.evict_oldest(), Some(native(1)));
    }
}
