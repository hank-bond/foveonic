//! Tracks which retained working photo belongs to which verified original, and prepares working photos for rendering.

use foveonic::{
    decode_cache::Cache,
    profile::Profile,
    render::{
        Engine, NativePhoto, PhotoKey, Purpose, ReducedPreparation, Resolution, Settings,
        WorkingSet,
    },
    source::{Metadata, Result, Source, SourceOrigin},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

type AcquireSource<'a> = dyn FnMut(&Path, u64) -> Result<Option<Arc<Source>>> + 'a;

fn source_identity(
    verified: Option<SourceOrigin>,
    loaded: Option<SourceOrigin>,
) -> Option<SourceOrigin> {
    verified.or(loaded)
}

/// Whether a reduced working photo is ready to render.
pub(super) enum Readiness {
    Ready,
    NeedsSource,
    Superseded,
}

struct Photo {
    path: PathBuf,
    identity: SourceOrigin,
    id: u64,
}
pub(super) struct Loaded {
    pub metadata: Arc<Metadata>,
    path: PathBuf,
    identity: SourceOrigin,
    id: u64,
    source: Option<Arc<Source>>,
    revision: u64,
}
impl Loaded {
    pub fn origin(&self) -> &SourceOrigin {
        &self.identity
    }
    pub fn key(&self, resolution: Resolution) -> PhotoKey {
        key(self.id, resolution)
    }
}
/// Maps each original's path to the working photos made from its verified identity.
#[derive(Default)]
pub(super) struct Photos {
    photos: Vec<Photo>,
    next_id: u64,
}
fn key(photo: u64, resolution: Resolution) -> PhotoKey {
    PhotoKey { photo, resolution }
}
impl Photos {
    pub fn id(&self, path: &Path) -> Option<u64> {
        self.photos
            .iter()
            .find(|photo| photo.path == path)
            .map(|photo| photo.id)
    }
    pub fn key(&self, path: &Path, resolution: Resolution) -> Option<PhotoKey> {
        Some(key(self.id(path)?, resolution))
    }
    fn prune(&mut self, working: &WorkingSet) {
        self.photos.retain(|photo| {
            working.contains(key(photo.id, Resolution::Native))
                || working.contains(key(photo.id, Resolution::Reduced))
        });
    }
    fn invalidate(&mut self, working: &mut WorkingSet, path: &Path, engine: &Engine) -> Result<()> {
        let Some(index) = self.photos.iter().position(|photo| photo.path == path) else {
            return Ok(());
        };
        let id = self.photos[index].id;
        let metadata = working.native(id).map(|native| native.metadata().clone());
        working.remove_photo(id)?;
        let photo = self.photos.remove(index);
        if let Some(metadata) = metadata {
            engine.release_native_preparation(&metadata);
        }
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Cache,
            "cache.processing.invalidated",
            |fields| {
                fields
                    .str("source", &path.to_string_lossy())
                    .u64("session", photo.id)
                    .str("reason", "source_identity_changed");
            },
        );
        Ok(())
    }
    fn verified_id(
        &mut self,
        working: &mut WorkingSet,
        path: &Path,
        cache: &Cache,
        engine: &Engine,
    ) -> Result<Option<u64>> {
        self.prune(working);
        if self.photos.iter().any(|photo| {
            photo.path == path
                && source_identity(cache.source_origin(path), Some(photo.identity.clone())).as_ref()
                    != Some(&photo.identity)
        }) {
            self.invalidate(working, path, engine)?;
        }
        Ok(self.id(path))
    }
    fn register(
        &mut self,
        working: &mut WorkingSet,
        path: &Path,
        identity: SourceOrigin,
        engine: &Engine,
    ) -> Result<u64> {
        if let Some(photo) = self.photos.iter().find(|photo| photo.path == path) {
            if photo.identity == identity {
                return Ok(photo.id);
            }
            self.invalidate(working, path, engine)?;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.photos.push(Photo {
            path: path.to_owned(),
            identity,
            id,
        });
        Ok(id)
    }
    pub fn native<'a>(&self, working: &'a WorkingSet, path: &Path) -> Option<&'a NativePhoto> {
        working.native(self.id(path)?)
    }
    pub fn with_native<T>(
        &self,
        working: &mut WorkingSet,
        path: &Path,
        operation: impl FnOnce(&mut NativePhoto) -> Result<T>,
    ) -> Result<T> {
        let id = self
            .id(path)
            .ok_or("native working photo is not retained")?;
        working
            .with_native(id, operation)?
            .ok_or_else(|| "native working photo is not retained".into())
    }
    pub fn shared_source(
        &self,
        working: &WorkingSet,
        path: &Path,
        origin: &SourceOrigin,
    ) -> Option<Arc<Source>> {
        self.native(working, path)
            .filter(|session| session.source().origin() == origin)
            .map(NativePhoto::shared_source)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn load_native(
        &mut self,
        working: &mut WorkingSet,
        path: &Path,
        revision: u64,
        cache: &Cache,
        engine: &Engine,
        purpose: Purpose,
        acquire: &mut AcquireSource<'_>,
    ) -> Result<bool> {
        let id = self.verified_id(working, path, cache, engine)?;
        if let Some(id) = id.filter(|id| working.native(*id).is_some()) {
            working.record_use(
                PhotoKey {
                    photo: id,
                    resolution: Resolution::Native,
                },
                purpose,
            );
            self.native_event(path, "hit", "same_source");
            return Ok(true);
        }
        let Some(source) = open_source(acquire, path, revision)? else {
            self.native_event(path, "pending", "not_retained");
            return Ok(false);
        };
        let id = self.register(working, path, source.origin().clone(), engine)?;
        working.insert_native(id, NativePhoto::new(source), engine, purpose)?;
        self.native_event(path, "loaded", "not_retained");
        Ok(true)
    }
    fn native_event(&self, path: &Path, decision: &str, reason: &str) {
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Cache,
            "cache.native_source",
            |fields| {
                fields
                    .str("decision", decision)
                    .str("reason", reason)
                    .u64("requested_photo", foveonic::diagnostics::photo_id(path));
                if let Some(id) = self.id(path) {
                    fields.u64("session", id);
                }
            },
        );
    }
    /// Whether the photo at `path` has a retained working photo at this resolution.
    pub fn resident(&self, working: &WorkingSet, path: &Path, resolution: Resolution) -> bool {
        self.key(path, resolution)
            .is_some_and(|key| working.contains(key))
    }
    pub fn reduced_sources(&self, working: &WorkingSet) -> Vec<(PathBuf, SourceOrigin)> {
        self.photos
            .iter()
            .filter(|photo| working.reduced(photo.id).is_some())
            .map(|photo| (photo.path.clone(), photo.identity.clone()))
            .collect()
    }
    pub fn contains_reduced(
        &self,
        working: &WorkingSet,
        path: &Path,
        origin: &SourceOrigin,
    ) -> bool {
        self.photos.iter().any(|photo| {
            photo.path == path && &photo.identity == origin && working.reduced(photo.id).is_some()
        })
    }
    /// Admit only the speculative candidate; pressure must not evict useful existing sessions.
    #[allow(clippy::too_many_arguments)]
    pub fn prefetch(
        &mut self,
        working: &mut WorkingSet,
        engine: &Engine,
        source: Arc<Source>,
        path: &Path,
        settings: &Settings,
        reservation: &foveonic::render::MemoryReservation,
        target: u64,
        cancel: &dyn Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<bool> {
        let origin = source.origin().clone();
        if self.contains_reduced(working, path, &origin)
            || cancel()
            || engine.memory_resources(working, reservation).total_bytes >= target
        {
            return Ok(false);
        }
        let ReducedPreparation::Prepared(mut session) =
            engine.prepare_reduced_photo(source, settings, cancel, profile)?
        else {
            return Ok(false);
        };
        if !matches!(
            engine.render_reduced_photo(&mut session, settings, &["final"], cancel, profile)?,
            foveonic::render::PhotoRender::Rendered(_)
        ) || cancel()
        {
            return Ok(false);
        }
        let id = self.register(working, path, origin, engine)?;
        working.insert_reduced(id, session, engine, Purpose::Speculative)?;
        let used = engine.memory_resources(working, reservation).total_bytes;
        let admitted = used <= target;
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Cache,
            "cache.processing.prefetch_admission",
            |fields| {
                fields
                    .str("source", &path.to_string_lossy())
                    .u64("used_bytes", used)
                    .u64("target_bytes", target)
                    .bool("admitted", admitted);
            },
        );
        if !admitted {
            working.remove(key(id, Resolution::Reduced))?;
        }
        self.prune(working);
        Ok(admitted)
    }
    pub fn trim_memory(
        &mut self,
        working: &mut WorkingSet,
        engine: &Engine,
        target: u64,
        reservation: &foveonic::render::MemoryReservation,
    ) -> foveonic::render::MemoryTrim {
        let result = engine.trim_memory(working, target, reservation);
        self.prune(working);
        result
    }
    pub fn clear(&mut self, working: &mut WorkingSet, engine: &Engine) -> Result<()> {
        working.clear()?;
        self.photos.clear();
        engine.clear_preparation();
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn load_reduced(
        &mut self,
        working: &mut WorkingSet,
        path: &Path,
        revision: u64,
        cache: &Cache,
        engine: &Engine,
        acquire: &mut AcquireSource<'_>,
    ) -> Result<Option<Loaded>> {
        let _scope = foveonic::diagnostics::scope_with(
            foveonic::diagnostics::Category::Cache,
            "cache.processing.lookup",
            |fields| {
                fields.str("source", &path.to_string_lossy());
            },
        );
        if let Some(id) = self.verified_id(working, path, cache, engine)? {
            if let Some(session) = working.reduced(id) {
                foveonic::diagnostics::event(
                    foveonic::diagnostics::Category::Cache,
                    "cache.processing.hit",
                    |fields| {
                        fields
                            .str("source", &path.to_string_lossy())
                            .u64("session", id);
                    },
                );
                return Ok(Some(Loaded {
                    metadata: session.metadata().clone(),
                    path: path.to_owned(),
                    identity: self
                        .photos
                        .iter()
                        .find(|p| p.id == id)
                        .unwrap()
                        .identity
                        .clone(),
                    id,
                    source: None,
                    revision,
                }));
            }
        }
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Cache,
            "cache.processing.miss",
            |fields| {
                fields
                    .str("source", &path.to_string_lossy())
                    .str("reason", "not_retained");
            },
        );
        let source = if let Some(session) = self.native(working, path) {
            session.shared_source()
        } else {
            let Some(source) = open_source(acquire, path, revision)? else {
                return Ok(None);
            };
            source
        };
        let identity = source.origin().clone();
        let id = self.register(working, path, identity.clone(), engine)?;
        Ok(Some(Loaded {
            metadata: source.metadata.clone(),
            path: path.to_owned(),
            identity,
            id,
            source: Some(source),
            revision,
        }))
    }
    /// Make sure a reduced working photo exists for these settings. `refresh`
    /// prepares sensor data again from the file after a render reported it stale.
    #[allow(clippy::too_many_arguments)]
    pub fn ready_reduced(
        &mut self,
        working: &mut WorkingSet,
        engine: &Engine,
        mut loaded: Loaded,
        settings: &Settings,
        purpose: Purpose,
        refresh: bool,
        cancel: &dyn Fn() -> bool,
        profile: &mut Profile,
        acquire: &mut AcquireSource<'_>,
    ) -> Result<Readiness> {
        let reduced = key(loaded.id, Resolution::Reduced);
        if working.contains(reduced) && !refresh {
            return Ok(Readiness::Ready);
        }
        if loaded.source.is_none() {
            loaded.source = self.shared_source(working, &loaded.path, &loaded.identity);
        }
        let original = if let Some(source) = loaded.source.take() {
            source
        } else {
            let Some(source) = open_source(acquire, &loaded.path, loaded.revision)? else {
                return Ok(Readiness::NeedsSource);
            };
            source
        };
        if working.contains(reduced) {
            foveonic::diagnostics::event(
                foveonic::diagnostics::Category::Cache,
                "cache.processing.refresh",
                |fields| {
                    fields.str("source", &loaded.path.to_string_lossy());
                    fields.u64("session", loaded.id);
                },
            );
            return Ok(
                match working.refresh_reduced(
                    engine, loaded.id, &original, settings, purpose, cancel, profile,
                )? {
                    Some(()) => Readiness::Ready,
                    None => Readiness::Superseded,
                },
            );
        }
        let session = match engine.prepare_reduced_photo(original, settings, cancel, profile)? {
            ReducedPreparation::Prepared(session) => session,
            ReducedPreparation::Superseded => return Ok(Readiness::Superseded),
        };
        working.insert_reduced(loaded.id, session, engine, purpose)?;
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Cache,
            "cache.processing.inserted",
            |fields| {
                fields.str("source", &loaded.path.to_string_lossy());
                fields.u64("session", loaded.id);
                fields.u64("scale", settings.working_scale.divisor() as u64);
            },
        );
        self.prune(working);
        Ok(Readiness::Ready)
    }
}

fn open_source(
    acquire: &mut AcquireSource<'_>,
    path: &Path,
    revision: u64,
) -> Result<Option<Arc<Source>>> {
    let Some(source) = acquire(path, revision)? else {
        return Ok(None);
    };
    super::record_source_load(revision, path, &source);
    Ok(Some(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use foveonic::source::FileIdentity;

    use foveonic::{
        inspection::{Artifact, Content},
        render::{PhotoRender, RenderProgress, Scale},
    };

    enum RenderOutcome {
        Rendered(Vec<Artifact>),
        Superseded,
        NeedsSource,
    }
    impl RenderOutcome {
        fn artifacts(self) -> Option<Vec<Artifact>> {
            match self {
                Self::Rendered(artifacts) => Some(artifacts),
                Self::Superseded => None,
                Self::NeedsSource => panic!("test supplies ready source acquisition"),
            }
        }
    }

    /// Photo identity with its own working set, rendering each photo alone.
    #[derive(Default)]
    struct TestPhotos {
        photos: Photos,
        working: WorkingSet,
    }
    impl std::ops::Deref for TestPhotos {
        type Target = Photos;
        fn deref(&self) -> &Photos {
            &self.photos
        }
    }
    impl TestPhotos {
        fn load_reduced(
            &mut self,
            path: &Path,
            revision: u64,
            cache: &Cache,
            engine: &Engine,
            acquire: &mut AcquireSource<'_>,
        ) -> Result<Option<Loaded>> {
            self.photos
                .load_reduced(&mut self.working, path, revision, cache, engine, acquire)
        }
        #[allow(clippy::too_many_arguments)]
        fn load_native(
            &mut self,
            path: &Path,
            revision: u64,
            cache: &Cache,
            engine: &Engine,
            purpose: Purpose,
            acquire: &mut AcquireSource<'_>,
        ) -> Result<bool> {
            self.photos.load_native(
                &mut self.working,
                path,
                revision,
                cache,
                engine,
                purpose,
                acquire,
            )
        }
        fn native(&self, path: &Path) -> Option<&NativePhoto> {
            self.photos.native(&self.working, path)
        }
        fn with_native<T>(
            &mut self,
            path: &Path,
            operation: impl FnOnce(&mut NativePhoto) -> Result<T>,
        ) -> Result<T> {
            self.photos.with_native(&mut self.working, path, operation)
        }
        fn memory_resources(
            &self,
            engine: &Engine,
            reservation: &foveonic::render::MemoryReservation,
        ) -> foveonic::render::MemoryResources {
            engine.memory_resources(&self.working, reservation)
        }
        fn shared_source(&self, path: &Path, origin: &SourceOrigin) -> Option<Arc<Source>> {
            self.photos.shared_source(&self.working, path, origin)
        }
        fn contains_reduced(&self, path: &Path, origin: &SourceOrigin) -> bool {
            self.photos.contains_reduced(&self.working, path, origin)
        }
        fn clear(&mut self, engine: &Engine) -> Result<()> {
            self.photos.clear(&mut self.working, engine)
        }
        fn trim_memory(
            &mut self,
            engine: &Engine,
            target: u64,
            reservation: &foveonic::render::MemoryReservation,
        ) -> foveonic::render::MemoryTrim {
            self.photos
                .trim_memory(&mut self.working, engine, target, reservation)
        }
        #[allow(clippy::too_many_arguments)]
        fn prefetch(
            &mut self,
            engine: &Engine,
            source: Arc<Source>,
            path: &Path,
            settings: &Settings,
            reservation: &foveonic::render::MemoryReservation,
            target: u64,
            cancel: &dyn Fn() -> bool,
            profile: &mut Profile,
        ) -> Result<bool> {
            self.photos.prefetch(
                &mut self.working,
                engine,
                source,
                path,
                settings,
                reservation,
                target,
                cancel,
                profile,
            )
        }
        /// Prepare and render one reduced photo, preparing sensor data again once from the file
        /// when its inputs are stale.
        #[allow(clippy::too_many_arguments)]
        fn render_reduced(
            &mut self,
            engine: &Engine,
            loaded: Loaded,
            settings: &Settings,
            purpose: Purpose,
            cancel: &dyn Fn() -> bool,
            profile: &mut Profile,
            acquire: &mut AcquireSource<'_>,
            progress: &dyn Fn(RenderProgress),
        ) -> Result<RenderOutcome> {
            let (id, path, identity, metadata, revision) = (
                loaded.id,
                loaded.path.clone(),
                loaded.identity.clone(),
                loaded.metadata.clone(),
                loaded.revision,
            );
            let mut next = Some(loaded);
            for refresh in [false, true] {
                let loaded = next.take().unwrap_or_else(|| Loaded {
                    metadata: metadata.clone(),
                    path: path.clone(),
                    identity: identity.clone(),
                    id,
                    source: None,
                    revision,
                });
                match self.photos.ready_reduced(
                    &mut self.working,
                    engine,
                    loaded,
                    settings,
                    purpose,
                    refresh,
                    cancel,
                    profile,
                    acquire,
                )? {
                    Readiness::Ready => {}
                    Readiness::NeedsSource => return Ok(RenderOutcome::NeedsSource),
                    Readiness::Superseded => return Ok(RenderOutcome::Superseded),
                }
                match self.working.render_reduced_with_progress(
                    engine, id, settings, purpose, cancel, profile, progress,
                )? {
                    Some(PhotoRender::Rendered(artifacts)) => {
                        return Ok(RenderOutcome::Rendered(artifacts))
                    }
                    Some(PhotoRender::Superseded) => return Ok(RenderOutcome::Superseded),
                    Some(PhotoRender::NeedsOriginal(_)) | None => {}
                }
            }
            Err("refreshed camera input is missing".into())
        }
    }

    #[test]
    fn verified_identity_snapshot_keeps_offline_reduced_edits_independent_of_source_stats() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("offline.x3f");
        std::fs::write(&path, b"identity").unwrap();
        let identity = SourceOrigin::File(FileIdentity::read(&path).unwrap());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            source_identity(Some(identity.clone()), None).unwrap(),
            identity.clone()
        );
        assert_eq!(
            source_identity(None, Some(identity.clone())).unwrap(),
            identity
        );
        assert!(source_identity(None, None).is_none());
    }
    #[test]
    fn newer_verified_replacement_wins_over_loaded_same_path_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("same.x3f");
        std::fs::write(&path, b"old").unwrap();
        let old = SourceOrigin::File(FileIdentity::read(&path).unwrap());
        std::fs::write(&path, b"replacement with another length").unwrap();
        let current = SourceOrigin::File(FileIdentity::read(&path).unwrap());
        assert_ne!(old, current);
        assert_eq!(
            source_identity(Some(current.clone()), Some(old)).unwrap(),
            current
        );
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn newly_prepared_cancelled_sensor_data_remains_accounted_and_retries_without_reloading() {
        let root = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let path = root.join("blue-parking-sign.x3f");
        let engine = Engine::new().unwrap();
        let cache = Cache::new();
        let mut photos = TestPhotos::default();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.working_scale = Scale::new(8).unwrap();
        let original = Arc::new(Source::open(&path).unwrap());
        let source_lifetime = Arc::downgrade(&original);
        let mut acquired = Some(original);
        let loaded = photos
            .load_reduced(&path, 1, &cache, &engine, &mut |_, _| Ok(acquired.take()))
            .unwrap()
            .unwrap();
        let prepared = std::cell::Cell::new(false);
        let cancelled = photos
            .render_reduced(
                &engine,
                loaded,
                &settings,
                Purpose::Edit,
                &|| {
                    let ready = source_lifetime.upgrade().is_none();
                    prepared.set(ready);
                    ready
                },
                &mut Profile::default(),
                &mut |_, _| panic!("source already acquired"),
                &|_| {},
            )
            .unwrap();
        assert!(
            prepared.get(),
            "cancel only after native source ownership retires"
        );
        assert!(matches!(cancelled, RenderOutcome::Superseded));
        let id = photos.id(&path).unwrap();
        assert!(photos.working.reduced(id).is_some());
        assert!(
            photos
                .memory_resources(&engine, &Default::default())
                .gpu_bytes
                > 0
        );
        let loaded = photos
            .load_reduced(&path, 2, &cache, &engine, &mut |_, _| {
                panic!("prepared sensor data must be reused")
            })
            .unwrap()
            .unwrap();
        let artifacts = photos
            .render_reduced(
                &engine,
                loaded,
                &settings,
                Purpose::Edit,
                &|| false,
                &mut Profile::default(),
                &mut |_, _| panic!("prepared sensor data must be reused"),
                &|_| {},
            )
            .unwrap()
            .artifacts()
            .unwrap();
        let Content::Image(image) = artifacts.into_iter().next().unwrap().content else {
            panic!("image");
        };
        let fresh = engine
            .render(
                &Source::open(&path).unwrap(),
                &settings,
                &mut Profile::default(),
            )
            .unwrap();
        assert_eq!(image.description(), fresh.description());
        assert_eq!(image.view(), fresh.view());
        assert_eq!(
            engine.readback_image(&image).unwrap(),
            engine.readback_image(&fresh).unwrap()
        );
        photos.clear(&engine).unwrap();
        assert!(photos.working.is_empty());
    }
    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn speculative_thumb_reuses_pixels_preserves_active_and_releases_source() {
        let root = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let paths = [
            root.join("blue-parking-sign.x3f"),
            root.join("merrill-hazy-coast.x3f"),
        ];
        let engine = Engine::new().unwrap();
        let cache = Cache::new();
        let mut photos = TestPhotos::default();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.working_scale = Scale::new(8).unwrap();
        let loaded = photos
            .load_reduced(&paths[0], 0, &cache, &engine, &mut |path, _| {
                cache.open(path, |_| {}).map(Some)
            })
            .unwrap()
            .expect("ready source");
        photos
            .render_reduced(
                &engine,
                loaded,
                &settings,
                Purpose::Edit,
                &|| false,
                &mut Profile::default(),
                &mut |path, _| cache.open(path, |_| {}).map(Some),
                &|_| {},
            )
            .unwrap()
            .artifacts()
            .unwrap();
        let recent_bytes = photos.working.most_recent_bytes();
        let source = Arc::new(Source::open(&paths[1]).unwrap());
        let origin = source.origin().clone();
        let weak = Arc::downgrade(&source);
        assert!(cache
            .priority()
            .speculate(|cancel| photos.prefetch(
                &engine,
                source,
                &paths[1],
                &settings,
                &Default::default(),
                1024 * 1024 * 1024,
                cancel,
                &mut Profile::default()
            ))
            .unwrap()
            .unwrap());
        assert!(weak.upgrade().is_none());
        assert_eq!(photos.working.most_recent_bytes(), recent_bytes);
        assert!(photos.contains_reduced(&paths[1], &origin));
        let before = engine.gpu_activity();
        let loaded = photos
            .load_reduced(&paths[1], 1, &cache, &engine, &mut |_, _| {
                panic!("prefetched input must not reopen source")
            })
            .unwrap()
            .expect("ready source");
        let mut artifacts = photos
            .render_reduced(
                &engine,
                loaded,
                &settings,
                Purpose::Edit,
                &|| false,
                &mut Profile::default(),
                &mut |_, _| panic!("no source load"),
                &|_| {},
            )
            .unwrap()
            .artifacts()
            .unwrap();
        assert_eq!(
            engine.gpu_activity().since(before).dispatches,
            1,
            "the existing editing frontier retains everything before the final tone pass"
        );
        let Content::Image(actual) = artifacts.pop().unwrap().content else {
            panic!("image")
        };
        let verifier = engine.executor();
        let fresh = verifier
            .render_artifacts(
                &Source::open(&paths[1]).unwrap(),
                &settings,
                &["final"],
                &mut Profile::default(),
            )
            .unwrap()
            .pop()
            .unwrap();
        let Content::Image(expected) = fresh.content else {
            panic!("image")
        };
        assert_eq!(
            engine.readback_image(&actual).unwrap(),
            verifier.readback_image(&expected).unwrap()
        );
        let first_id = photos
            .photos
            .photos
            .iter()
            .find(|photo| photo.path == paths[0])
            .unwrap()
            .id;
        photos
            .working
            .remove(key(first_id, Resolution::Reduced))
            .unwrap();
        let used = photos.working.len();
        let before = engine.gpu_activity();
        assert!(!photos
            .prefetch(
                &engine,
                Arc::new(Source::open(&paths[0]).unwrap()),
                &paths[0],
                &settings,
                &Default::default(),
                0,
                &|| false,
                &mut Profile::default()
            )
            .unwrap());
        assert_eq!(photos.working.len(), used);
        assert_eq!(
            engine.gpu_activity().since(before).dispatches,
            0,
            "no available memory means no speculative GPU work"
        );
    }

    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn reduced_photo_switches_reprepare_and_native_promotion_match_fresh() {
        let root = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let paths = [
            root.join("blue-parking-sign.x3f"),
            root.join("merrill-hazy-coast.x3f"),
        ];
        let engine = Engine::new().unwrap();
        let cache = Cache::new();
        cache.configure(foveonic::decode_cache::Settings {
            enabled: false,
            ..Default::default()
        });
        let mut photos = TestPhotos::default();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.working_scale = Scale::new(8).unwrap();
        settings.reconstruction = Some(Default::default());
        settings.dehaze_estimate = true;
        settings.dehaze.shadow = None;
        for (round, index) in [0, 1, 0, 1, 0].into_iter().enumerate() {
            let path = &paths[index];
            settings.exposure = round as f32 * 0.1;
            if round == 4 {
                settings.reconstruction.as_mut().unwrap().chroma = 4;
            }
            let loaded = photos
                .load_reduced(path, 0, &cache, &engine, &mut |path, _| {
                    cache.open(path, |_| {}).map(Some)
                })
                .unwrap()
                .expect("ready source");
            let weak = loaded.source.as_ref().map(Arc::downgrade);
            let Some(mut artifacts) = photos
                .render_reduced(
                    &engine,
                    loaded,
                    &settings,
                    Purpose::Edit,
                    &|| false,
                    &mut Profile::default(),
                    &mut |path, _| cache.open(path, |_| {}).map(Some),
                    &|_| {},
                )
                .unwrap()
                .artifacts()
            else {
                panic!("expected reduced photo {round}");
            };
            if let Some(weak) = weak {
                assert!(weak.upgrade().is_none());
            }
            let trim = photos.trim_memory(&engine, 4 * 1024 * 1024 * 1024, &Default::default());
            assert_eq!(trim.over_target_bytes, 0);
            let artifact = artifacts.pop().unwrap();
            let Content::Image(image) = artifact.content else {
                panic!("expected image");
            };
            let verifier = engine.executor();
            let fresh = verifier
                .render_artifacts(
                    &Source::open(path).unwrap(),
                    &settings,
                    &["final"],
                    &mut Profile::default(),
                )
                .unwrap()
                .pop()
                .unwrap();
            assert_eq!(artifact.dehaze_estimate, fresh.dehaze_estimate);
            let Content::Image(expected) = fresh.content else {
                panic!("expected image");
            };
            assert_eq!(
                engine.readback_image(&image).unwrap(),
                verifier.readback_image(&expected).unwrap(),
                "photo switch {round}"
            );
        }
        assert_eq!(photos.working.len(), 2);
        assert!(photos
            .load_native(
                &paths[0],
                0,
                &cache,
                &engine,
                Purpose::Edit,
                &mut |path, _| cache.open(path, |_| {}).map(Some)
            )
            .unwrap());
        let mut full = settings.clone();
        full.working_scale = Scale::NATIVE;
        photos
            .with_native(&paths[0], |session| {
                engine.render_native_photo(
                    session,
                    &full,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
            })
            .unwrap()
            .unwrap();
        let mut active_haze = settings.clone();
        active_haze.dehaze.amount = 10.;
        let loaded = photos
            .load_reduced(&paths[0], 0, &cache, &engine, &mut |path, _| {
                cache.open(path, |_| {}).map(Some)
            })
            .unwrap()
            .expect("ready source");
        assert!(photos
            .render_reduced(
                &engine,
                loaded,
                &active_haze,
                Purpose::Edit,
                &|| false,
                &mut Profile::default(),
                &mut |path, _| cache.open(path, |_| {}).map(Some),
                &|_| {},
            )
            .unwrap()
            .artifacts()
            .is_some());
        assert!(photos.native(&paths[0]).is_some());
        let loaded = photos
            .load_reduced(&paths[0], 0, &cache, &engine, &mut |path, _| {
                cache.open(path, |_| {}).map(Some)
            })
            .unwrap()
            .expect("ready source");
        assert!(photos
            .render_reduced(
                &engine,
                loaded,
                &settings,
                Purpose::Edit,
                &|| false,
                &mut Profile::default(),
                &mut |path, _| cache.open(path, |_| {}).map(Some),
                &|_| {},
            )
            .unwrap()
            .artifacts()
            .is_some());
        assert!(photos.native(&paths[0]).is_some());
        let trim = photos.trim_memory(&engine, 4 * 1024 * 1024 * 1024, &Default::default());
        assert_eq!(trim.over_target_bytes, 0);
        photos.clear(&engine).unwrap();
        assert!(photos.working.is_empty());
        assert_eq!(engine.cached_preparation_bytes(), 0);
    }
    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Merrill originals"]
    fn native_photo_return_reuses_source_and_replacement_invalidates_both_grids() {
        let root = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let directory = tempfile::tempdir().unwrap();
        let a = directory.path().join("a.x3f");
        let b = directory.path().join("b.x3f");
        std::fs::copy(root.join("blue-parking-sign.x3f"), &a).unwrap();
        std::fs::copy(root.join("merrill-hazy-coast.x3f"), &b).unwrap();
        let engine = Engine::new().unwrap();
        let cache = Cache::new();
        let mut photos = TestPhotos::default();
        let settings = Settings::new("Sunlight", "Standard");
        let mut acquired = 0;
        let mut saved = Vec::new();
        for path in [&a, &b] {
            assert!(photos
                .load_native(path, 0, &cache, &engine, Purpose::Edit, &mut |path, _| {
                    acquired += 1;
                    cache.open(path, |_| {}).map(Some)
                })
                .unwrap());
            let artifacts = photos
                .with_native(path, |session| {
                    engine.render_native_photo(
                        session,
                        &settings,
                        &["final"],
                        || false,
                        &mut Profile::default(),
                    )
                })
                .unwrap()
                .unwrap();
            let Content::Image(image) = artifacts.into_iter().next().unwrap().content else {
                panic!("image")
            };
            saved.push(image);
        }
        assert_eq!(acquired, 2);
        let a_id = photos.id(&a).unwrap();
        let b_id = photos.id(&b).unwrap();
        assert!(photos.working.contains(key(a_id, Resolution::Native)));
        assert!(photos.working.contains(key(b_id, Resolution::Native)));
        let reservation = Default::default();
        let trim = photos.trim_memory(&engine, 4 * 1024 * 1024 * 1024, &reservation);
        assert_eq!(trim.over_target_bytes, 0);
        for (index, path) in [&a, &b, &a].into_iter().enumerate() {
            assert!(photos
                .load_native(path, 1, &cache, &engine, Purpose::Edit, &mut |_, _| panic!(
                    "retained native source must not be acquired again"
                ))
                .unwrap());
            let artifacts = photos
                .with_native(path, |session| {
                    engine.render_native_photo(
                        session,
                        &settings,
                        &["final"],
                        || false,
                        &mut Profile::default(),
                    )
                })
                .unwrap()
                .unwrap();
            let Content::Image(image) = artifacts.into_iter().next().unwrap().content else {
                panic!("image")
            };
            assert_eq!(
                engine.readback_image(&image).unwrap(),
                engine.readback_image(&saved[index % 2]).unwrap()
            );
        }
        let verifier = engine.executor();
        let fresh = verifier
            .render_artifacts(
                &Source::open(&a).unwrap(),
                &settings,
                &["final"],
                &mut Profile::default(),
            )
            .unwrap()
            .pop()
            .unwrap();
        let Content::Image(fresh) = fresh.content else {
            panic!("image")
        };
        assert_eq!(
            engine.readback_image(&saved[0]).unwrap(),
            verifier.readback_image(&fresh).unwrap()
        );
        let a_origin = photos.native(&a).unwrap().source().origin().clone();
        let native_source = photos.shared_source(&a, &a_origin).unwrap();
        let loaded = photos
            .load_reduced(&a, 2, &cache, &engine, &mut |_, _| {
                panic!("reduced preparation should share the native source")
            })
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(loaded.source.as_ref().unwrap(), &native_source));
        drop(native_source);
        let mut reduced = settings.clone();
        reduced.working_scale = Scale::new(8).unwrap();
        assert!(photos
            .render_reduced(
                &engine,
                loaded,
                &reduced,
                Purpose::Edit,
                &|| false,
                &mut Profile::default(),
                &mut |_, _| panic!("shared source"),
                &|_| {},
            )
            .unwrap()
            .artifacts()
            .is_some());
        assert!(photos.working.contains(key(a_id, Resolution::Reduced)));
        let preserved = engine.readback_image(&saved[0]).unwrap();
        // Publish a fresh verified identity through the same cache boundary used
        // by the worker, then request the replaced photograph through its owner.
        std::fs::copy(root.join("merrill-hazy-coast.x3f"), &a).unwrap();
        let replacement = Source::open(&a).unwrap();
        let replacement_origin = replacement.origin().clone();
        drop(replacement);
        assert_ne!(replacement_origin, a_origin);
        let verified = Cache::new();
        verified.configure(foveonic::decode_cache::Settings::default());
        verified.prefetch_window(vec![a.clone()], Some(a.clone()));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while verified.source_origin(&a).as_ref() != Some(&replacement_origin) {
            assert!(
                std::time::Instant::now() < deadline,
                "replacement identity was not verified"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let mut replacement_acquisitions = 0;
        assert!(photos
            .load_native(&a, 3, &verified, &engine, Purpose::Edit, &mut |path, _| {
                replacement_acquisitions += 1;
                verified.open(path, |_| {}).map(Some)
            })
            .unwrap());
        assert_eq!(replacement_acquisitions, 1);
        let new_id = photos.id(&a).unwrap();
        assert_ne!(new_id, a_id);
        assert_eq!(
            photos.native(&a).unwrap().source().origin(),
            &replacement_origin
        );
        assert!(!photos.working.contains(key(a_id, Resolution::Native)));
        assert!(!photos.working.contains(key(a_id, Resolution::Reduced)));
        assert!(photos.working.contains(key(b_id, Resolution::Native)));
        assert_eq!(engine.readback_image(&saved[0]).unwrap(), preserved);
        photos.clear(&engine).unwrap();
        assert!(photos.working.is_empty());
        assert!(photos.native(&b).is_none());
        assert_eq!(engine.readback_image(&saved[0]).unwrap(), preserved);
    }
}
