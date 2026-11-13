//! Owns verified per-photo native and reduced sessions under the shared processing budget.

use foveonic::{
    decode_cache::Cache,
    inspection::Artifact,
    profile::Profile,
    render::{
        Engine, ProcessingWorkingSet, ReducedPreparation, ReducedRender, RenderProgress,
        RenderSession, SessionKey, SessionKind, SessionUse, Settings,
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

pub(super) enum RenderOutcome {
    Rendered(Vec<Artifact>),
    Superseded,
    NeedsSource,
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
}
pub(super) struct Photos {
    working: ProcessingWorkingSet,
    photos: Vec<Photo>,
    next_id: u64,
}
impl Default for Photos {
    fn default() -> Self {
        Self {
            working: ProcessingWorkingSet::new(),
            photos: Vec::new(),
            next_id: 0,
        }
    }
}
fn key(photo: u64, kind: SessionKind) -> SessionKey {
    SessionKey { photo, kind }
}
impl Photos {
    fn id(&self, path: &Path) -> Option<u64> {
        self.photos
            .iter()
            .find(|photo| photo.path == path)
            .map(|photo| photo.id)
    }
    fn prune(&mut self) {
        self.photos.retain(|photo| {
            self.working.contains(key(photo.id, SessionKind::Native))
                || self.working.contains(key(photo.id, SessionKind::Reduced))
        });
    }
    fn invalidate(&mut self, path: &Path, engine: &Engine) {
        let Some(index) = self.photos.iter().position(|photo| photo.path == path) else {
            return;
        };
        let photo = self.photos.remove(index);
        if let Some(native) = self.working.native(photo.id) {
            engine.release_native_preparation(native.metadata());
        }
        self.working.remove_photo(photo.id);
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
    }
    fn verified_id(&mut self, path: &Path, cache: &Cache, engine: &Engine) -> Option<u64> {
        self.prune();
        if self.photos.iter().any(|photo| {
            photo.path == path
                && source_identity(cache.source_origin(path), Some(photo.identity.clone())).as_ref()
                    != Some(&photo.identity)
        }) {
            self.invalidate(path, engine);
        }
        self.id(path)
    }
    fn register(&mut self, path: &Path, identity: SourceOrigin, engine: &Engine) -> u64 {
        if let Some(photo) = self.photos.iter().find(|photo| photo.path == path) {
            if photo.identity == identity {
                return photo.id;
            }
            self.invalidate(path, engine);
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.photos.push(Photo {
            path: path.to_owned(),
            identity,
            id,
        });
        id
    }
    pub fn native(&self, path: &Path) -> Option<&RenderSession> {
        self.working.native(self.id(path)?)
    }
    pub fn native_mut(&mut self, path: &Path) -> Option<&mut RenderSession> {
        let id = self.id(path)?;
        self.working.native_mut(id)
    }
    pub fn native_sources(&self) -> impl Iterator<Item = (&Path, &RenderSession)> {
        self.photos.iter().filter_map(|photo| {
            self.working
                .native(photo.id)
                .map(|session| (photo.path.as_path(), session))
        })
    }
    pub fn shared_source(&self, path: &Path, origin: &SourceOrigin) -> Option<Arc<Source>> {
        self.native(path)
            .filter(|session| session.source().origin() == origin)
            .map(RenderSession::shared_source)
    }
    pub fn load_native(
        &mut self,
        path: &Path,
        revision: u64,
        cache: &Cache,
        engine: &Engine,
        usage: SessionUse,
        acquire: &mut AcquireSource<'_>,
    ) -> Result<bool> {
        let id = self.verified_id(path, cache, engine);
        if let Some(id) = id.filter(|id| self.working.native(*id).is_some()) {
            self.native_event(path, "hit", "same_source");
            self.working.activate(key(id, SessionKind::Native), usage);
            return Ok(true);
        }
        let Some(source) = open_source(acquire, path, revision)? else {
            self.native_event(path, "pending", "not_retained");
            return Ok(false);
        };
        let id = self.register(path, source.origin().clone(), engine);
        self.working
            .insert_native(id, RenderSession::new(source), engine, usage);
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
    pub fn has_prefetch_capacity(
        &self,
        engine: &Engine,
        reservation: &foveonic::render::MemoryReservation,
        limit: u64,
    ) -> bool {
        self.memory_resources(engine, reservation).total_bytes < limit
    }
    pub fn reduced_sources(&self) -> Vec<(PathBuf, SourceOrigin)> {
        self.photos
            .iter()
            .filter(|photo| self.working.reduced(photo.id).is_some())
            .map(|photo| (photo.path.clone(), photo.identity.clone()))
            .collect()
    }
    pub fn contains_reduced(&self, path: &Path, origin: &SourceOrigin) -> bool {
        self.photos.iter().any(|photo| {
            photo.path == path
                && &photo.identity == origin
                && self.working.reduced(photo.id).is_some()
        })
    }
    /// Admit only the speculative candidate; pressure must not evict useful existing sessions.
    #[allow(clippy::too_many_arguments)]
    pub fn prefetch(
        &mut self,
        engine: &Engine,
        source: Arc<Source>,
        path: &Path,
        settings: &Settings,
        reservation: &foveonic::render::MemoryReservation,
        limit: u64,
        cancel: &dyn Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<bool> {
        let origin = source.origin().clone();
        if self.contains_reduced(path, &origin)
            || cancel()
            || !self.has_prefetch_capacity(engine, reservation, limit)
        {
            return Ok(false);
        }
        let ReducedPreparation::Prepared(mut session) =
            engine.prepare_reduced_session(source, settings, cancel, profile)?
        else {
            return Ok(false);
        };
        if !matches!(
            engine.render_reduced_session(&mut session, settings, &["final"], cancel, profile)?,
            ReducedRender::Rendered(_)
        ) || cancel()
        {
            return Ok(false);
        }
        let id = self.register(path, origin, engine);
        self.working
            .insert_reduced(id, session, engine, SessionUse::Preparation)?;
        let used = self.memory_resources(engine, reservation).total_bytes;
        let admitted = used <= limit;
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Cache,
            "cache.processing.prefetch_admission",
            |fields| {
                fields
                    .str("source", &path.to_string_lossy())
                    .u64("used_bytes", used)
                    .u64("budget_bytes", limit)
                    .bool("admitted", admitted);
            },
        );
        if !admitted {
            self.working.remove(key(id, SessionKind::Reduced));
        }
        self.prune();
        Ok(admitted)
    }
    pub fn memory_resources(
        &self,
        engine: &Engine,
        reservation: &foveonic::render::MemoryReservation,
    ) -> foveonic::render::MemoryResources {
        engine.memory_resources(&self.working, reservation)
    }
    pub fn trim_memory(
        &mut self,
        engine: &Engine,
        limit: u64,
        reservation: &foveonic::render::MemoryReservation,
    ) -> foveonic::render::MemoryTrim {
        let result = engine.trim_memory(&mut self.working, limit, reservation);
        self.prune();
        result
    }
    pub fn clear(&mut self, engine: &Engine) {
        *self = Self::default();
        engine.clear_preparation();
    }
    pub fn park_active(&mut self) {
        self.working.park_active();
    }
    pub fn load_reduced(
        &mut self,
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
        if let Some(id) = self.verified_id(path, cache, engine) {
            if let Some(session) = self.working.reduced(id) {
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
        let source = if let Some(session) = self.native(path) {
            session.shared_source()
        } else {
            let Some(source) = open_source(acquire, path, revision)? else {
                return Ok(None);
            };
            source
        };
        let identity = source.origin().clone();
        let id = self.register(path, identity.clone(), engine);
        Ok(Some(Loaded {
            metadata: source.metadata.clone(),
            path: path.to_owned(),
            identity,
            id,
            source: Some(source),
            revision,
        }))
    }
    pub fn reduced_changes(
        &self,
        loaded: &Loaded,
        settings: &Settings,
    ) -> Option<Vec<&'static str>> {
        self.working
            .reduced_settings(loaded.id)
            .map(|previous| previous.changed_parameters(settings))
    }
    pub fn reduced_bytes(&self, loaded: &Loaded) -> u64 {
        self.working
            .reduced_bytes(loaded.id)
            .map_or(0, |bytes| bytes.0)
    }
    pub fn reduced_bytes_for_path(&self, path: &Path) -> u64 {
        self.id(path)
            .and_then(|id| self.working.reduced_bytes(id))
            .map_or(0, |bytes| bytes.0)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_reduced(
        &mut self,
        engine: &Engine,
        mut loaded: Loaded,
        settings: &Settings,
        usage: SessionUse,
        cancel: &dyn Fn() -> bool,
        profile: &mut Profile,
        acquire: &mut AcquireSource<'_>,
        render_progress: &dyn Fn(RenderProgress),
    ) -> Result<RenderOutcome> {
        if loaded.source.is_none() {
            loaded.source = self.shared_source(&loaded.path, &loaded.identity);
        }
        if let Some(outcome) = self.working.render_reduced_with_progress(
            engine,
            loaded.id,
            settings,
            usage,
            cancel,
            profile,
            render_progress,
        )? {
            match outcome {
                ReducedRender::Rendered(artifacts) => {
                    self.prune();
                    return Ok(RenderOutcome::Rendered(artifacts));
                }
                ReducedRender::Superseded => return Ok(RenderOutcome::Superseded),
                ReducedRender::NeedsOriginal(reason) => {
                    foveonic::diagnostics::event(
                        foveonic::diagnostics::Category::Cache,
                        "cache.processing.refresh",
                        |fields| {
                            fields.str("source", &loaded.path.to_string_lossy());
                            fields.u64("session", loaded.id);
                            fields.str("reason", &format!("{reason:?}"));
                        },
                    );
                }
            }
        }
        let original = if let Some(source) = loaded.source.take() {
            source
        } else {
            let Some(source) = open_source(acquire, &loaded.path, loaded.revision)? else {
                return Ok(RenderOutcome::NeedsSource);
            };
            source
        };
        if self.working.contains(key(loaded.id, SessionKind::Reduced)) {
            if self
                .working
                .refresh_reduced(
                    engine, loaded.id, &original, settings, usage, cancel, profile,
                )?
                .is_none()
            {
                return Ok(RenderOutcome::Superseded);
            }
            if let Some(outcome) = self.working.render_reduced_with_progress(
                engine,
                loaded.id,
                settings,
                usage,
                cancel,
                profile,
                render_progress,
            )? {
                self.prune();
                return match outcome {
                    ReducedRender::Rendered(artifacts) => Ok(RenderOutcome::Rendered(artifacts)),
                    ReducedRender::Superseded => Ok(RenderOutcome::Superseded),
                    ReducedRender::NeedsOriginal(reason) => {
                        Err(format!("refreshed camera input is missing: {reason:?}").into())
                    }
                };
            }
        }
        let preparation = engine.prepare_reduced_session(original, settings, cancel, profile)?;
        let mut session = match preparation {
            ReducedPreparation::Prepared(session) => session,
            ReducedPreparation::Superseded => return Ok(RenderOutcome::Superseded),
        };
        let outcome = engine.render_reduced_session_with_progress(
            &mut session,
            settings,
            &["final"],
            cancel,
            profile,
            render_progress,
        )?;
        self.working
            .insert_reduced(loaded.id, session, engine, usage)?;
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Cache,
            "cache.processing.inserted",
            |fields| {
                fields.str("source", &loaded.path.to_string_lossy());
                fields.u64("session", loaded.id);
                fields.u64("scale", settings.working_scale.divisor() as u64);
            },
        );
        self.prune();
        match outcome {
            ReducedRender::Rendered(artifacts) => Ok(RenderOutcome::Rendered(artifacts)),
            ReducedRender::Superseded => Ok(RenderOutcome::Superseded),
            ReducedRender::NeedsOriginal(reason) => {
                Err(format!("sealed reduced request requires original: {reason:?}").into())
            }
        }
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

    impl RenderOutcome {
        fn artifacts(self) -> Option<Vec<Artifact>> {
            match self {
                Self::Rendered(artifacts) => Some(artifacts),
                Self::Superseded => None,
                Self::NeedsSource => panic!("test supplies ready source acquisition"),
            }
        }
    }
    use foveonic::{inspection::Content, render::Scale};

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
    fn speculative_thumb_reuses_pixels_preserves_active_and_releases_source() {
        let root = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let paths = [
            root.join("blue-parking-sign.x3f"),
            root.join("merrill-hazy-coast.x3f"),
        ];
        let engine = Engine::new().unwrap();
        let cache = Cache::new();
        let mut photos = Photos::default();
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
                SessionUse::Foreground,
                &|| false,
                &mut Profile::default(),
                &mut |path, _| cache.open(path, |_| {}).map(Some),
                &|_| {},
            )
            .unwrap()
            .artifacts()
            .unwrap();
        let active_bytes = photos.working.active_bytes();
        let source = Arc::new(Source::open(&paths[1]).unwrap());
        let origin = source.origin().clone();
        let weak = Arc::downgrade(&source);
        assert!(cache
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
        assert_eq!(photos.working.active_bytes(), active_bytes);
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
                SessionUse::Foreground,
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
            .iter()
            .find(|photo| photo.path == paths[0])
            .unwrap()
            .id;
        photos.working.remove(key(first_id, SessionKind::Reduced));
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
    fn reduced_photo_switches_reseal_and_native_promotion_match_fresh() {
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
        let mut photos = Photos::default();
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
                    SessionUse::Foreground,
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
            assert_eq!(trim.over_budget_bytes, 0);
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
        photos.park_active();
        assert!(photos
            .load_native(
                &paths[0],
                0,
                &cache,
                &engine,
                SessionUse::Foreground,
                &mut |path, _| cache.open(path, |_| {}).map(Some)
            )
            .unwrap());
        let mut full = settings.clone();
        full.working_scale = Scale::NATIVE;
        engine
            .render_session(
                photos.native_mut(&paths[0]).unwrap(),
                &full,
                &["final"],
                || false,
                &mut Profile::default(),
            )
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
                SessionUse::Foreground,
                &|| false,
                &mut Profile::default(),
                &mut |path, _| cache.open(path, |_| {}).map(Some),
                &|_| {}
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
                SessionUse::Foreground,
                &|| false,
                &mut Profile::default(),
                &mut |path, _| cache.open(path, |_| {}).map(Some),
                &|_| {}
            )
            .unwrap()
            .artifacts()
            .is_some());
        assert!(photos.native(&paths[0]).is_some());
        let trim = photos.trim_memory(&engine, 4 * 1024 * 1024 * 1024, &Default::default());
        assert_eq!(trim.over_budget_bytes, 0);
        photos.clear(&engine);
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
        let mut photos = Photos::default();
        let settings = Settings::new("Sunlight", "Standard");
        let mut acquired = 0;
        let mut saved = Vec::new();
        for path in [&a, &b] {
            assert!(photos
                .load_native(
                    path,
                    0,
                    &cache,
                    &engine,
                    SessionUse::Foreground,
                    &mut |path, _| {
                        acquired += 1;
                        cache.open(path, |_| {}).map(Some)
                    }
                )
                .unwrap());
            let artifacts = engine
                .render_session(
                    photos.native_mut(path).unwrap(),
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
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
        assert!(photos.working.contains(key(a_id, SessionKind::Native)));
        assert!(photos.working.contains(key(b_id, SessionKind::Native)));
        let reservation = Default::default();
        let trim = photos.trim_memory(&engine, 4 * 1024 * 1024 * 1024, &reservation);
        assert_eq!(trim.over_budget_bytes, 0);
        for (index, path) in [&a, &b, &a].into_iter().enumerate() {
            assert!(photos
                .load_native(
                    path,
                    1,
                    &cache,
                    &engine,
                    SessionUse::Foreground,
                    &mut |_, _| panic!("retained native source must not be acquired again")
                )
                .unwrap());
            let artifacts = engine
                .render_session(
                    photos.native_mut(path).unwrap(),
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
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
                SessionUse::Foreground,
                &|| false,
                &mut Profile::default(),
                &mut |_, _| panic!("shared source"),
                &|_| {}
            )
            .unwrap()
            .artifacts()
            .is_some());
        assert!(photos.working.contains(key(a_id, SessionKind::Reduced)));
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
            .load_native(
                &a,
                3,
                &verified,
                &engine,
                SessionUse::Foreground,
                &mut |path, _| {
                    replacement_acquisitions += 1;
                    verified.open(path, |_| {}).map(Some)
                }
            )
            .unwrap());
        assert_eq!(replacement_acquisitions, 1);
        let new_id = photos.id(&a).unwrap();
        assert_ne!(new_id, a_id);
        assert_eq!(
            photos.native(&a).unwrap().source().origin(),
            &replacement_origin
        );
        assert!(!photos.working.contains(key(a_id, SessionKind::Native)));
        assert!(!photos.working.contains(key(a_id, SessionKind::Reduced)));
        assert!(photos.working.contains(key(b_id, SessionKind::Native)));
        assert_eq!(engine.readback_image(&saved[0]).unwrap(), preserved);
        photos.clear(&engine);
        assert!(photos.working.is_empty());
        assert!(photos.native(&b).is_none());
        assert_eq!(engine.readback_image(&saved[0]).unwrap(), preserved);
    }
}
