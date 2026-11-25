//! Retains native and reduced working photos in recency order and protects each one while a render holds its guard.

use super::{Engine, NativePhoto, PhotoRender, ReducedPhoto, Settings};
use crate::{
    priority::Priority,
    profile::Profile,
    recipe::RetainedBytes,
    source::{Metadata, Result, Source, SourceOrigin},
};
use std::sync::{Arc, Weak};

/// The resolution of a working photo: the native sensor grid or a reduced one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Resolution {
    Native,
    Reduced,
}

/// A photograph may have one working photo at each resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub struct PhotoKey {
    pub photo: u64,
    pub resolution: Resolution,
}

/// What a job is for. It sets the job's priority and how its use of a working
/// photo counts toward keeping it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// The edit shown in a viewport.
    Edit,
    /// Other viewport work, such as Before images and color picks.
    View,
    /// Navigator images and scopes.
    Auxiliary,
    Thumbnail,
    Export,
    Speculative,
}

impl Purpose {
    pub fn priority(self) -> Priority {
        match self {
            Self::Edit | Self::View => Priority::Viewport,
            Self::Auxiliary => Priority::Auxiliary,
            Self::Thumbnail => Priority::Thumbnail,
            Self::Export => Priority::Export,
            Self::Speculative => Priority::Speculative,
        }
    }
}

/// One order of uses shared by every photo, so trimming can compare a photo's
/// last use with any edit's restart inputs.
fn next_use() -> u64 {
    static USES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    USES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// How far one use counts toward keeping a working photo.
#[derive(PartialEq)]
enum Counts {
    /// The photo and the inputs its edit restarts from.
    Edit,
    Photo,
    Nothing,
}

/// The eviction policy for every use. Export and speculative work never keep a
/// photo warm, and only the edit decides which restart inputs are newest, so a
/// Before image cannot displace the current edit's inputs.
fn counts(purpose: Purpose) -> Counts {
    match purpose {
        Purpose::Edit => Counts::Edit,
        Purpose::View | Purpose::Auxiliary | Purpose::Thumbnail => Counts::Photo,
        Purpose::Export | Purpose::Speculative => Counts::Nothing,
    }
}

#[allow(clippy::large_enum_variant)] // Native working photos avoid a separate allocation.
pub(super) enum WorkingPhoto {
    Native(NativePhoto),
    Reduced(Box<ReducedPhoto>),
}
struct Entry {
    key: PhotoKey,
    session: WorkingPhoto,
    execution: Option<Weak<()>>,
    /// Latest use that counts (see `counts`), on the same scale as restart-input
    /// recency. Zero when no counting use happened.
    last_use: u64,
}

impl Entry {
    fn executing(&self) -> bool {
        self.execution
            .as_ref()
            .is_some_and(|execution| execution.strong_count() != 0)
    }
}

/// A guarded working photo must retire before it is replaced or cleared.
#[derive(Debug)]
pub struct PhotoInUse {
    pub key: PhotoKey,
}

impl std::fmt::Display for PhotoInUse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "working photo {:?} is executing", self.key)
    }
}

impl std::error::Error for PhotoInUse {}

/// The entry keeps only a weak identity so errors release protection naturally.
/// The execution owner must drain submitted work before dropping this guard.
#[derive(Debug)]
pub(super) struct Guard {
    key: PhotoKey,
    identity: Arc<()>,
}

/// Working photos share one memory target.
#[derive(Default)]
pub struct WorkingSet {
    entries: Vec<Entry>,
    /// Working photos that trimming never evicts.
    eviction_protected: std::collections::HashSet<PhotoKey>,
}

/// What one trimming step dropped.
pub(super) enum Trimmed {
    RestartInput,
    Photo(PhotoKey),
}
impl WorkingSet {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn native_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.key.resolution == Resolution::Native)
            .count()
    }
    pub fn set_eviction_protected(&mut self, keys: impl IntoIterator<Item = PhotoKey>) {
        self.eviction_protected = keys.into_iter().collect();
    }
    pub fn contains(&self, key: PhotoKey) -> bool {
        self.entries.iter().any(|entry| entry.key == key)
    }
    fn key(photo: u64, resolution: Resolution) -> PhotoKey {
        PhotoKey { photo, resolution }
    }
    fn get(&self, key: PhotoKey) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.key == key)
    }
    fn get_mut(&mut self, key: PhotoKey) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|entry| entry.key == key)
    }
    fn require_idle(&self, photo: u64) -> Result<()> {
        if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.key.photo == photo && entry.executing())
        {
            return Err(Box::new(PhotoInUse { key: entry.key }));
        }
        Ok(())
    }
    pub(super) fn guard(&mut self, key: PhotoKey) -> Result<Guard> {
        self.require_idle(key.photo)?;
        let entry = self.get_mut(key).ok_or("working photo is not retained")?;
        let identity = Arc::new(());
        entry.execution = Some(Arc::downgrade(&identity));
        Ok(Guard { key, identity })
    }
    pub(super) fn execution(&self, guard: &Guard) -> &WorkingPhoto {
        let entry = self
            .get(guard.key)
            .expect("guarded working photo is retained");
        assert!(entry
            .execution
            .as_ref()
            .is_some_and(|identity| identity.ptr_eq(&Arc::downgrade(&guard.identity))));
        &entry.session
    }
    pub(super) fn execution_mut(&mut self, guard: &Guard) -> &mut WorkingPhoto {
        let entry = self
            .get_mut(guard.key)
            .expect("guarded working photo is retained");
        assert!(entry
            .execution
            .as_ref()
            .is_some_and(|identity| identity.ptr_eq(&Arc::downgrade(&guard.identity))));
        &mut entry.session
    }
    pub fn native(&self, photo: u64) -> Option<&NativePhoto> {
        match &self.get(Self::key(photo, Resolution::Native))?.session {
            WorkingPhoto::Native(session) => Some(session),
            _ => None,
        }
    }
    pub fn reduced(&self, photo: u64) -> Option<&ReducedPhoto> {
        match &self.get(Self::key(photo, Resolution::Reduced))?.session {
            WorkingPhoto::Reduced(session) => Some(session),
            _ => None,
        }
    }
    /// Record that a job used a working photo. Returns the stamp for the edit's
    /// restart inputs when the use counts as an edit.
    pub fn record_use(&mut self, key: PhotoKey, purpose: Purpose) -> Option<u64> {
        let entry = self.get_mut(key)?;
        let counts = counts(purpose);
        if counts == Counts::Nothing {
            return None;
        }
        entry.last_use = next_use();
        (counts == Counts::Edit).then_some(entry.last_use)
    }
    /// A use that counts for nothing keeps an existing working photo and drops
    /// `session`. Callers check `contains` first when that matters.
    fn insert(&mut self, key: PhotoKey, session: WorkingPhoto, purpose: Purpose) -> Result<()> {
        self.require_idle(key.photo)?;
        // A use that does not count must not replace a photo that does.
        if counts(purpose) == Counts::Nothing && self.contains(key) {
            return Ok(());
        }
        self.remove(key)?;
        self.entries.push(Entry {
            key,
            session,
            execution: None,
            last_use: 0,
        });
        self.record_use(key, purpose);
        Ok(())
    }
    pub fn insert_native(
        &mut self,
        photo: u64,
        mut session: NativePhoto,
        engine: &Engine,
        purpose: Purpose,
    ) -> Result<()> {
        session.cache.bind(&engine.owner);
        self.insert(
            Self::key(photo, Resolution::Native),
            WorkingPhoto::Native(session),
            purpose,
        )
    }
    pub fn insert_reduced(
        &mut self,
        photo: u64,
        session: Box<ReducedPhoto>,
        engine: &Engine,
        purpose: Purpose,
    ) -> Result<()> {
        if !Arc::ptr_eq(&session.owner, &engine.owner) {
            return Err("reduced inputs belong to another engine".into());
        }
        self.insert(
            Self::key(photo, Resolution::Reduced),
            WorkingPhoto::Reduced(session),
            purpose,
        )
    }
    /// Removal is rejected while either representation of this photo executes.
    pub fn remove(&mut self, key: PhotoKey) -> Result<bool> {
        self.require_idle(key.photo)?;
        let before = self.entries.len();
        self.entries.retain(|entry| entry.key != key);
        Ok(self.entries.len() != before)
    }
    /// Check both representations before removing either one.
    pub fn remove_photo(&mut self, photo: u64) -> Result<bool> {
        self.require_idle(photo)?;
        let before = self.entries.len();
        self.entries.retain(|entry| entry.key.photo != photo);
        Ok(self.entries.len() != before)
    }
    /// The execution owner must drain and retire all guards before clearing.
    pub fn clear(&mut self) -> Result<()> {
        if let Some(entry) = self.entries.iter().find(|entry| entry.executing()) {
            return Err(Box::new(PhotoInUse { key: entry.key }));
        }
        self.entries.clear();
        Ok(())
    }
    /// Execute one synchronous native operation while holding its guard.
    /// The operation must finish its submitted work before returning.
    pub fn with_native<T>(
        &mut self,
        photo: u64,
        operation: impl FnOnce(&mut NativePhoto) -> Result<T>,
    ) -> Result<Option<T>> {
        let key = Self::key(photo, Resolution::Native);
        if !self.contains(key) {
            return Ok(None);
        }
        let guard = self.guard(key)?;
        let WorkingPhoto::Native(session) = self.execution_mut(&guard) else {
            unreachable!()
        };
        let result = operation(session);
        drop(guard);
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
    pub fn reduced_resources(&self, photo: u64) -> Option<super::PhotoResources> {
        self.reduced(photo).map(ReducedPhoto::resources)
    }
    fn most_recent(&self) -> Option<usize> {
        (0..self.entries.len()).max_by_key(|index| self.entries[*index].last_use)
    }
    pub fn most_recent_bytes(&self) -> u64 {
        self.most_recent().map_or(0, |index| {
            let session = &self.entries[index].session;
            session.retained_cpu_bytes() + session.cached_bytes()
        })
    }
    pub fn older_bytes(&self) -> u64 {
        let recent = self.most_recent();
        let entries = (0..self.entries.len())
            .filter(move |index| Some(*index) != recent)
            .map(|index| &self.entries[index]);
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
                        WorkingPhoto::Native(_) => size_of::<NativePhoto>() as u64,
                        WorkingPhoto::Reduced(_) => 0,
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
    /// Shared preparation belongs to retained working photos and is released with them.
    pub(super) fn retained_inputs(&self) -> impl Iterator<Item = (&Arc<Metadata>, &SourceOrigin)> {
        self.entries
            .iter()
            .map(|entry| (entry.session.metadata(), entry.session.origin()))
    }
    /// An upper estimate of what trimming could release here: the optional
    /// versions of an idle photo, and all of one that is not eviction
    /// protected. Shared storage is counted more than once.
    pub(super) fn releasable_bytes(&self) -> u64 {
        self.entries
            .iter()
            .filter(|entry| !entry.executing())
            .map(|entry| {
                if self.eviction_protected.contains(&entry.key) {
                    entry.session.cache().optional_bytes()
                } else {
                    entry.session.cached_bytes() + entry.session.retained_cpu_bytes()
                }
            })
            .sum()
    }
    pub(super) fn trim_optional(&mut self) -> bool {
        self.entries
            .iter_mut()
            .any(|entry| !entry.executing() && entry.session.cache_mut().trim_optional())
    }
    /// Drop whatever was used longest ago: a restart input of a photo's latest
    /// edit or a whole working photo. Eviction-protected photos keep both.
    pub(super) fn trim_least_recent(&mut self) -> Option<Trimmed> {
        let idle = || {
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| !entry.executing())
                .filter(|(_, entry)| !self.eviction_protected.contains(&entry.key))
        };
        let input = idle()
            .filter_map(|(index, entry)| Some((entry.session.cache().restart_use()?, index)))
            .min();
        let photo = idle().map(|(index, entry)| (entry.last_use, index)).min();
        if let Some((edit_use, index)) = input {
            // A photo is never older than its own inputs, so on a tie its inputs
            // go first and its sensor data, which needs a decode to rebuild, last.
            if photo.is_none_or(|(last_use, _)| edit_use <= last_use) {
                self.entries[index].session.cache_mut().drop_restart_input();
                return Some(Trimmed::RestartInput);
            }
        }
        let (_, index) = photo?;
        Some(Trimmed::Photo(self.evict(index)))
    }
    /// Trim until a whole working photo leaves, for tests about photo order.
    #[cfg(test)]
    pub(super) fn evict_least_recent(&mut self) -> Option<PhotoKey> {
        loop {
            match self.trim_least_recent()? {
                Trimmed::RestartInput => {}
                Trimmed::Photo(key) => return Some(key),
            }
        }
    }
    fn evict(&mut self, index: usize) -> PhotoKey {
        let entry = self.entries.remove(index);
        crate::diagnostics::event(
            crate::diagnostics::Category::Cache,
            "cache.processing.entry_released",
            |fields| {
                fields
                    .u64("session", entry.key.photo)
                    .str(
                        "kind",
                        match entry.key.resolution {
                            Resolution::Native => "native",
                            Resolution::Reduced => "reduced",
                        },
                    )
                    .u64("gpu_bytes", entry.session.cached_bytes());
            },
        );
        entry.key
    }
    /// Refresh only a verified original.
    #[allow(clippy::too_many_arguments)]
    pub fn refresh_reduced(
        &mut self,
        engine: &Engine,
        photo: u64,
        source: &Source,
        settings: &Settings,
        purpose: Purpose,
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
        let guard = self.guard(Self::key(photo, Resolution::Reduced))?;
        self.record_use(guard.key, purpose);
        let WorkingPhoto::Reduced(session) = self.execution_mut(&guard) else {
            unreachable!()
        };
        let result = engine.refresh_reduced_photo(session, source, settings, cancel, profile);
        drop(guard);
        result
    }
    pub fn render_reduced(
        &mut self,
        engine: &Engine,
        photo: u64,
        settings: &Settings,
        purpose: Purpose,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<Option<PhotoRender>> {
        self.render_observed(engine, photo, settings, purpose, cancel, profile, None)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_reduced_with_progress(
        &mut self,
        engine: &Engine,
        photo: u64,
        settings: &Settings,
        purpose: Purpose,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: impl Fn(super::RenderProgress),
    ) -> Result<Option<PhotoRender>> {
        self.render_observed(
            engine,
            photo,
            settings,
            purpose,
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
        purpose: Purpose,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: Option<&dyn Fn(super::RenderProgress)>,
    ) -> Result<Option<PhotoRender>> {
        super::driver::render_working_photo(
            engine,
            self,
            Self::key(photo, Resolution::Reduced),
            settings,
            &["final"],
            purpose,
            &cancel,
            profile,
            progress,
        )
    }
}
impl WorkingPhoto {
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
            pixels: vec![1., 2., 3.].into(),
            storage: SourceStorage::InterleavedRgb,
            camf: vec![],
            timings: vec![],
        })
    }

    fn native(photo: u64) -> PhotoKey {
        PhotoKey {
            photo,
            resolution: Resolution::Native,
        }
    }

    fn retained_photos() -> WorkingSet {
        let mut working = WorkingSet::new();
        for photo in [1, 2, 3] {
            working
                .insert(
                    native(photo),
                    WorkingPhoto::Native(NativePhoto::new(source())),
                    Purpose::Speculative,
                )
                .unwrap();
        }
        working
    }

    /// Two photos whose last uses carry explicit stamps. Photo 1 was edited
    /// before photo 2 was last used and viewed after it.
    fn stamped_photos(gpu: &crate::gpu::Gpu, protected: bool) -> WorkingSet {
        let mut working = WorkingSet::new();
        for (photo, last_use, inputs) in [(1, 4, &["old", "newer"][..]), (2, 2, &[][..])] {
            let mut session = NativePhoto::new(source());
            session.cache = crate::interactive::cache_tests::restart_inputs(gpu, inputs, 1);
            working
                .insert(
                    native(photo),
                    WorkingPhoto::Native(session),
                    Purpose::Speculative,
                )
                .unwrap();
            working.get_mut(native(photo)).unwrap().last_use = last_use;
        }
        if protected {
            working.set_eviction_protected([native(1)]);
        }
        working
    }

    #[test]
    #[ignore = "requires a GPU"]
    fn one_recency_pass_orders_restart_inputs_and_photos() {
        let gpu = crate::gpu::Gpu::test_executor();
        let mut working = stamped_photos(&gpu, false);
        for _ in 0..2 {
            assert!(
                matches!(working.trim_least_recent(), Some(Trimmed::RestartInput)),
                "photo 1's edit predates photo 2's last use"
            );
        }
        assert!(
            matches!(working.trim_least_recent(), Some(Trimmed::Photo(key)) if key == native(2)),
            "photo 2 predates photo 1's last use"
        );
        assert!(
            matches!(working.trim_least_recent(), Some(Trimmed::Photo(key)) if key == native(1))
        );
        assert!(working.trim_least_recent().is_none());

        let mut working = stamped_photos(&gpu, true);
        assert!(
            matches!(working.trim_least_recent(), Some(Trimmed::Photo(key)) if key == native(2))
        );
        assert!(
            working.trim_least_recent().is_none(),
            "an eviction-protected photo keeps its latest edit's inputs"
        );
        assert_eq!(working.native(1).unwrap().cache.restart_use(), Some(1));
    }

    #[test]
    fn purposes_decide_what_a_use_keeps() {
        let mut working = retained_photos();
        let last_use = |working: &WorkingSet, photo| working.get(native(photo)).unwrap().last_use;
        for purpose in [Purpose::Export, Purpose::Speculative] {
            assert_eq!(working.record_use(native(1), purpose), None);
            assert_eq!(
                last_use(&working, 1),
                0,
                "{purpose:?} does not count as use"
            );
        }
        for purpose in [Purpose::View, Purpose::Auxiliary, Purpose::Thumbnail] {
            let before = last_use(&working, 2);
            assert_eq!(working.record_use(native(2), purpose), None);
            assert!(
                last_use(&working, 2) > before,
                "{purpose:?} keeps its photo"
            );
        }
        let stamp = working.record_use(native(3), Purpose::Edit).unwrap();
        assert_eq!(last_use(&working, 3), stamp);
        let next = working.record_use(native(1), Purpose::Edit).unwrap();
        assert!(next > stamp, "one order of uses across photos");
    }

    #[test]
    fn guards_protect_several_photos_and_recency_orders_eviction() {
        let mut working = retained_photos();
        working.record_use(native(3), Purpose::Edit);
        working.set_eviction_protected([native(3)]);
        let first = working.guard(native(1)).unwrap();
        let second = working.guard(native(2)).unwrap();
        assert_eq!(working.retained_inputs().count(), 3);
        assert_eq!(
            working.evict_least_recent(),
            None,
            "the eviction-protected photo stays and the others are guarded"
        );
        drop(first);
        assert_eq!(working.evict_least_recent(), Some(native(1)));
        drop(second);
        assert_eq!(working.evict_least_recent(), Some(native(2)));
        assert_eq!(working.evict_least_recent(), None);
    }

    #[test]
    fn busy_mutations_leave_source_identity_and_other_entries_unchanged() {
        let mut working = retained_photos();
        working.record_use(native(2), Purpose::Edit);
        let original = working.native(1).unwrap().shared_source();
        let guard = working.guard(native(1)).unwrap();
        assert!(working.guard(native(1)).unwrap_err().is::<PhotoInUse>());
        assert!(working
            .with_native(1, |_| Ok(()))
            .unwrap_err()
            .is::<PhotoInUse>());
        assert!(working.remove(native(1)).unwrap_err().is::<PhotoInUse>());
        assert!(working.remove_photo(1).unwrap_err().is::<PhotoInUse>());
        assert!(working.clear().unwrap_err().is::<PhotoInUse>());
        assert!(working
            .insert(
                native(1),
                WorkingPhoto::Native(NativePhoto::new(source())),
                Purpose::Edit,
            )
            .unwrap_err()
            .is::<PhotoInUse>());
        assert!(Arc::ptr_eq(
            &original,
            &working.native(1).unwrap().shared_source()
        ));
        assert_eq!(working.len(), 3);
        assert!(working.native(2).is_some());
        assert!(working.native(3).is_some());
        drop(guard);
        assert!(working.remove_photo(1).unwrap());
        working.clear().unwrap();
        assert!(working.is_empty());
    }

    #[test]
    fn native_operation_releases_protection_after_each_outcome() {
        let mut working = retained_photos();
        assert_eq!(working.with_native(1, |_| Ok(42)).unwrap(), Some(42));
        assert!(working.guard(native(1)).is_ok());
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
    fn guarded_photos_keep_checkpoints_and_accounting_under_pressure() {
        use crate::{
            inspection::Content,
            render::{ReducedPreparation, Scale},
        };
        let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let engine = Engine::new().unwrap();
        let mut working = WorkingSet::new();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.working_scale = Scale::new(8).unwrap();
        let mut images = Vec::new();
        for (photo, name) in [(1, "blue-parking-sign.x3f"), (2, "merrill-hazy-coast.x3f")] {
            let source = Arc::new(Source::open(root.join(name)).unwrap());
            let ReducedPreparation::Prepared(session) = engine
                .prepare_reduced_photo(source.clone(), &settings, || false, &mut Profile::default())
                .unwrap()
            else {
                panic!("prepared inputs");
            };
            working
                .insert_reduced(photo, session, &engine, Purpose::Edit)
                .unwrap();
            let Some(PhotoRender::Rendered(artifacts)) = working
                .render_reduced(
                    &engine,
                    photo,
                    &settings,
                    Purpose::Edit,
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
                    .insert_native(3, NativePhoto::new(source), &engine, Purpose::Speculative)
                    .unwrap();
            }
        }
        let key = |photo| PhotoKey {
            photo,
            resolution: Resolution::Reduced,
        };
        let buffers = |working: &WorkingSet, photo| {
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
        let first = working.guard(key(1)).unwrap();
        let second = working.guard(key(2)).unwrap();
        assert_eq!(
            engine.memory_resources(&working, &Default::default()),
            before
        );
        let trim = engine.trim_memory(&mut working, 0, &Default::default());
        assert_eq!(trim.evicted_photos, vec![native(3)]);
        assert!(trim.over_target_bytes > 0);
        assert_eq!(buffers(&working, 1), expected[0]);
        assert_eq!(buffers(&working, 2), expected[1]);
        for (index, guard) in [&first, &second].into_iter().enumerate() {
            let WorkingPhoto::Reduced(session) = working.execution_mut(guard) else {
                unreachable!()
            };
            let PhotoRender::Rendered(artifacts) = engine
                .render_reduced_photo(
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
                    Purpose::Speculative,
                    || true,
                    &mut Profile::default(),
                )
                .unwrap(),
            Some(PhotoRender::Superseded)
        ));
        assert!(working.guard(key(1)).is_ok());
        let mut unavailable = settings.clone();
        unavailable.color_mode = "unavailable-test-color-mode".into();
        assert!(working
            .render_reduced(
                &engine,
                1,
                &unavailable,
                Purpose::Speculative,
                || false,
                &mut Profile::default(),
            )
            .is_err());
        assert!(working.guard(key(1)).is_ok());
        assert_eq!(buffers(&working, 1), expected[0]);
        let Some(PhotoRender::Rendered(artifacts)) = working
            .render_reduced(
                &engine,
                1,
                &settings,
                Purpose::Speculative,
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
        // An eviction-protected photo stays even when nothing fits.
        working.set_eviction_protected([key(1)]);
        let trim = engine.trim_memory(&mut working, 0, &Default::default());
        assert_eq!(trim.evicted_photos, vec![key(2)]);
        assert!(working.reduced(1).is_some());
        assert!(trim.over_target_bytes > 0);
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn one_photo_cannot_execute_both_representations_or_be_partly_invalidated() {
        use crate::render::{ReducedPreparation, Scale};
        let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f");
        let source = Arc::new(Source::open(path).unwrap());
        let engine = Engine::new().unwrap();
        let mut working = WorkingSet::new();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.working_scale = Scale::new(8).unwrap();
        working
            .insert_native(1, NativePhoto::new(source.clone()), &engine, Purpose::Edit)
            .unwrap();
        let ReducedPreparation::Prepared(session) = engine
            .prepare_reduced_photo(source, &settings, || false, &mut Profile::default())
            .unwrap()
        else {
            panic!("prepared inputs");
        };
        working
            .insert_reduced(1, session, &engine, Purpose::Speculative)
            .unwrap();
        let guard = working.guard(native(1)).unwrap();
        let reduced = PhotoKey {
            photo: 1,
            resolution: Resolution::Reduced,
        };
        assert!(working.guard(reduced).unwrap_err().is::<PhotoInUse>());
        assert!(working
            .render_reduced(
                &engine,
                1,
                &settings,
                Purpose::Edit,
                || false,
                &mut Profile::default()
            )
            .err()
            .unwrap()
            .is::<PhotoInUse>());
        assert!(working.remove_photo(1).unwrap_err().is::<PhotoInUse>());
        assert!(working.clear().unwrap_err().is::<PhotoInUse>());
        assert!(working.native(1).is_some());
        assert!(working.reduced(1).is_some());
        drop(guard);
        assert!(working.remove_photo(1).unwrap());
        assert!(working.is_empty());
    }

    #[test]
    fn preparation_does_not_count_as_use() {
        let source = source();
        let native = |photo| PhotoKey {
            photo,
            resolution: Resolution::Native,
        };
        let mut working = WorkingSet::new();
        working
            .insert(
                native(1),
                WorkingPhoto::Native(NativePhoto::new(source.clone())),
                Purpose::Edit,
            )
            .unwrap();
        working
            .insert(
                native(2),
                WorkingPhoto::Native(NativePhoto::new(source.clone())),
                Purpose::Edit,
            )
            .unwrap();
        working
            .insert(
                native(3),
                WorkingPhoto::Native(NativePhoto::new(source.clone())),
                Purpose::Speculative,
            )
            .unwrap();
        assert!(working.contains(native(1)));
        assert_eq!(working.evict_least_recent(), Some(native(3)));
        assert_eq!(working.evict_least_recent(), Some(native(1)));
        assert_eq!(working.evict_least_recent(), Some(native(2)));
        assert_eq!(working.evict_least_recent(), None);
    }

    #[test]
    fn foreground_reactivation_changes_eviction_order() {
        let source = source();
        let native = |photo| PhotoKey {
            photo,
            resolution: Resolution::Native,
        };
        let mut working = WorkingSet::new();
        for photo in [1, 2, 3] {
            working
                .insert(
                    native(photo),
                    WorkingPhoto::Native(NativePhoto::new(source.clone())),
                    Purpose::Edit,
                )
                .unwrap();
        }
        assert!(working.record_use(native(1), Purpose::Edit).is_some());
        assert_eq!(working.evict_least_recent(), Some(native(2)));
        assert_eq!(working.evict_least_recent(), Some(native(3)));
        assert_eq!(working.evict_least_recent(), Some(native(1)));
        assert_eq!(working.evict_least_recent(), None);
    }
}
