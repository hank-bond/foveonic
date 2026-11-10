//! Reuses bounded reduced-photo inputs while native requests keep the existing worker session.

use foveonic::{
    decode_cache::Cache,
    inspection::Artifact,
    profile::Profile,
    render::{
        Engine, ReducedPreparation, ReducedRender, ReducedWorkingSet, RenderProgress,
        RenderSession, Settings,
    },
    source::{FileIdentity, LoadStage, Metadata, Result, Source, SourceOrigin},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

const BUDGET: u64 = 128 * 1024 * 1024;

fn source_identity(
    path: &Path,
    verified: Option<SourceOrigin>,
    loaded: Option<SourceOrigin>,
) -> Result<SourceOrigin> {
    verified
        .or(loaded)
        .map_or_else(|| FileIdentity::read(path).map(SourceOrigin::File), Ok)
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
    working: ReducedWorkingSet,
    photos: Vec<Photo>,
    next_id: u64,
}
impl Default for Photos {
    fn default() -> Self {
        Self {
            working: ReducedWorkingSet::new(BUDGET),
            photos: Vec::new(),
            next_id: 0,
        }
    }
}
impl Photos {
    pub fn has_prefetch_capacity(
        &self,
        engine: &Engine,
        native: Option<&RenderSession>,
        reservation: &foveonic::render::MemoryReservation,
        limit: u64,
    ) -> bool {
        engine
            .memory_resources(native, &self.working, reservation)
            .total_bytes
            < limit
            && self.working.historical_bytes() < self.working.limit_bytes()
    }
    pub fn source_snapshot(&self) -> Vec<(PathBuf, SourceOrigin)> {
        self.photos
            .iter()
            .filter(|photo| self.working.contains(photo.id))
            .map(|photo| (photo.path.clone(), photo.identity.clone()))
            .collect()
    }
    pub fn contains_source(&self, path: &Path, origin: &SourceOrigin) -> bool {
        self.photos.iter().any(|photo| {
            photo.path == path && &photo.identity == origin && self.working.contains(photo.id)
        })
    }

    /// Prepare cold history without taking over the native or reduced active photo.
    /// Admission uses spare budget only, so optional work cannot evict used photos.
    #[allow(clippy::too_many_arguments)]
    pub fn prefetch(
        &mut self,
        engine: &Engine,
        source: Arc<Source>,
        path: &Path,
        settings: &Settings,
        native: Option<&RenderSession>,
        reservation: &foveonic::render::MemoryReservation,
        limit: u64,
        cancel: &dyn Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<bool> {
        let origin = source.origin().clone();
        if self.contains_source(path, &origin)
            || cancel()
            || !self.has_prefetch_capacity(engine, native, reservation, limit)
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
        let bytes = session
            .cached_bytes()
            .saturating_add(session.retained_cpu_bytes());
        let used = engine
            .memory_resources(native, &self.working, reservation)
            .total_bytes;
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Cache,
            "cache.processing.prefetch_admission",
            |fields| {
                fields
                    .str("source", &path.to_string_lossy())
                    .u64("session_bytes", bytes)
                    .u64("used_bytes", used)
                    .u64("budget_bytes", limit)
                    .u64("historical_bytes", self.working.historical_bytes())
                    .u64("historical_budget_bytes", self.working.limit_bytes());
            },
        );
        if bytes > limit.saturating_sub(used)
            || bytes
                > self
                    .working
                    .limit_bytes()
                    .saturating_sub(self.working.historical_bytes())
        {
            return Ok(false);
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.working.prefetch(id, session, engine)?;
        self.photos.retain(|photo| photo.path != path);
        self.photos.push(Photo {
            path: path.to_owned(),
            identity: origin,
            id,
        });
        self.prune();
        Ok(self.working.contains(id))
    }

    pub fn configure_memory(&mut self, limit: u64) {
        self.working.set_limit(limit);
        self.prune();
    }
    pub fn trim_memory(
        &mut self,
        engine: &Engine,
        native: Option<&mut RenderSession>,
        limit: u64,
        reservation: &foveonic::render::MemoryReservation,
    ) -> foveonic::render::MemoryTrim {
        self.configure_memory(limit);
        let result = engine.trim_memory(native, &mut self.working, limit, reservation);
        self.prune();
        result
    }
    pub fn clear(&mut self, engine: &Engine) {
        *self = Self::default();
        engine.clear_preparation();
    }
    pub fn park_active(&mut self, engine: &Engine, revision: u64) {
        self.working.park_active();
        self.report_retention(engine, revision, "park");
        self.prune();
    }
    fn prune(&mut self) {
        self.photos.retain(|photo| self.working.contains(photo.id));
    }
    fn report_retention(&self, engine: &Engine, revision: u64, stage: &str) {
        if crate::timing::enabled() {
            let evicted: Vec<_> = self
                .photos
                .iter()
                .filter(|photo| !self.working.contains(photo.id))
                .map(|photo| photo.id)
                .collect();
            crate::timing::emit("working_set", || {
                serde_json::json!({"revision": revision, "stage": stage,
                "historical_budget_bytes": self.working.limit_bytes(), "historical_bytes": self.working.historical_bytes(),
                "active_bytes": self.working.active_bytes(), "active_cache_limit_bytes": self.working.active_cache_limit_bytes(),
                "retained_bytes": self.working.retained_bytes(engine),
                "resident_photos": self.working.len(), "evicted": evicted})
            });
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        &mut self,
        path: &Path,
        revision: u64,
        cache: &Cache,
        engine: &Engine,
        native: &mut Option<(PathBuf, RenderSession)>,
        progress: &dyn Fn(LoadStage),
    ) -> Result<Loaded> {
        let _scope = foveonic::diagnostics::scope_with(
            foveonic::diagnostics::Category::Cache,
            "cache.processing.lookup",
            |fields| {
                fields.str("source", &path.to_string_lossy());
            },
        );
        self.prune();
        let native_origin = native
            .as_ref()
            .filter(|(loaded, _)| loaded == path)
            .map(|(_, session)| session.source().origin().clone());
        let loaded_origin = self
            .photos
            .iter()
            .find(|photo| photo.path == path)
            .and_then(|photo| self.working.origin(photo.id))
            .cloned();
        let mut identity = source_identity(
            path,
            cache.source_origin(path).or(native_origin),
            loaded_origin,
        )?;
        if let Some(photo) = self
            .photos
            .iter()
            .find(|photo| photo.path == path && photo.identity == identity)
        {
            if let Some(metadata) = self.working.metadata(photo.id) {
                foveonic::diagnostics::event(
                    foveonic::diagnostics::Category::Cache,
                    "cache.processing.hit",
                    |fields| {
                        fields.str("source", &path.to_string_lossy());
                        fields.u64("session", photo.id);
                    },
                );
                return Ok(Loaded {
                    metadata: metadata.clone(),
                    path: path.to_owned(),
                    identity,
                    id: photo.id,
                    source: None,
                    revision,
                });
            }
        }
        if let Some(old) = self.photos.iter().find(|photo| photo.path == path) {
            foveonic::diagnostics::event(
                foveonic::diagnostics::Category::Cache,
                "cache.processing.invalidated",
                |fields| {
                    fields.str("source", &path.to_string_lossy());
                    fields.u64("session", old.id);
                    fields.str("reason", "source_identity_changed");
                },
            );
            self.working.remove(old.id);
        }
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Cache,
            "cache.processing.miss",
            |fields| {
                fields.str("source", &path.to_string_lossy());
                fields.str("reason", "not_retained");
            },
        );
        self.prune();
        let (metadata, source) = if let Some((_, session)) =
            native.as_ref().filter(|(native_path, session)| {
                native_path == path && *session.source().origin() == identity
            }) {
            (session.metadata().clone(), None)
        } else {
            if let Some((_, previous)) = native.take() {
                engine.release_native_preparation(previous.metadata());
            }
            let source = open_source(cache, path, revision, progress)?;
            identity = source.origin().clone();
            (source.metadata.clone(), Some(source))
        };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        Ok(Loaded {
            metadata,
            path: path.to_owned(),
            identity,
            id,
            source,
            revision,
        })
    }
    pub fn changed_parameters(
        &self,
        loaded: &Loaded,
        settings: &Settings,
    ) -> Option<Vec<&'static str>> {
        self.working
            .last_settings(loaded.id)
            .map(|previous| previous.changed_parameters(settings))
    }
    pub fn cached_bytes(&self, loaded: &Loaded) -> u64 {
        self.working
            .session_bytes(loaded.id)
            .map_or(0, |bytes| bytes.0)
    }
    pub fn cached_bytes_for_path(&self, path: &Path) -> u64 {
        self.photos
            .iter()
            .find(|photo| photo.path == path)
            .and_then(|photo| self.working.session_bytes(photo.id))
            .map_or(0, |bytes| bytes.0)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        engine: &Engine,
        cache: &Cache,
        native: &mut Option<(PathBuf, RenderSession)>,
        mut loaded: Loaded,
        settings: &Settings,
        cancel: &dyn Fn() -> bool,
        profile: &mut Profile,
        loaded_progress: &dyn Fn(LoadStage),
        render_progress: &dyn Fn(RenderProgress),
    ) -> Result<Option<Vec<Artifact>>> {
        // Native-only clipping evidence must leave with the native session before
        // the shared reduced budget is enforced. Keep at most this request's Arc
        // for resealing when the corrected sensor or analysis has changed.
        if let Some((path, session)) = native.take() {
            engine.release_native_preparation(session.metadata());
            if path == loaded.path && loaded.source.is_none() {
                loaded.source = Some(session.into_source());
            }
        }
        if let Some(outcome) = self.working.render_with_progress(
            engine,
            loaded.id,
            settings,
            cancel,
            profile,
            render_progress,
        )? {
            match outcome {
                ReducedRender::Rendered(artifacts) => {
                    self.report_retention(engine, loaded.revision, "render");
                    self.prune();
                    return Ok(Some(artifacts));
                }
                ReducedRender::Superseded => return Ok(None),
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
            open_source(cache, &loaded.path, loaded.revision, loaded_progress)?
        };
        if self.working.contains(loaded.id) {
            if self
                .working
                .refresh(engine, loaded.id, &original, settings, cancel, profile)?
                .is_none()
            {
                return Ok(None);
            }
            if let Some(outcome) = self.working.render_with_progress(
                engine,
                loaded.id,
                settings,
                cancel,
                profile,
                render_progress,
            )? {
                self.report_retention(engine, loaded.revision, "refresh");
                self.prune();
                return match outcome {
                    ReducedRender::Rendered(artifacts) => Ok(Some(artifacts)),
                    ReducedRender::Superseded => Ok(None),
                    ReducedRender::NeedsOriginal(reason) => {
                        Err(format!("refreshed camera input is missing: {reason:?}").into())
                    }
                };
            }
        }
        let preparation = engine.prepare_reduced_session(original, settings, cancel, profile)?;
        let mut session = match preparation {
            ReducedPreparation::Prepared(session) => session,
            ReducedPreparation::Superseded => return Ok(None),
        };
        let outcome = engine.render_reduced_session_with_progress(
            &mut session,
            settings,
            &["final"],
            cancel,
            profile,
            render_progress,
        )?;
        self.working.insert(loaded.id, session, engine)?;
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Cache,
            "cache.processing.inserted",
            |fields| {
                fields.str("source", &loaded.path.to_string_lossy());
                fields.u64("session", loaded.id);
                fields.u64("scale", settings.working_scale.divisor() as u64);
            },
        );
        self.photos.retain(|photo| photo.path != loaded.path);
        self.photos.push(Photo {
            path: loaded.path,
            identity: loaded.identity,
            id: loaded.id,
        });
        self.report_retention(engine, loaded.revision, "insert");
        self.prune();
        match outcome {
            ReducedRender::Rendered(artifacts) => Ok(Some(artifacts)),
            ReducedRender::Superseded => Ok(None),
            ReducedRender::NeedsOriginal(reason) => {
                Err(format!("sealed reduced request requires original: {reason:?}").into())
            }
        }
    }
}

fn open_source(
    cache: &Cache,
    path: &Path,
    revision: u64,
    progress: &dyn Fn(LoadStage),
) -> Result<Arc<Source>> {
    let _span = crate::timing::Span::new(revision, "source_open");
    let source = cache.open(path, progress)?;
    super::record_source_load(revision, path, &source);
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use foveonic::{inspection::Content, render::Scale};

    #[test]
    fn verified_identity_snapshot_keeps_offline_reduced_edits_independent_of_source_stats() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("offline.x3f");
        std::fs::write(&path, b"identity").unwrap();
        let identity = SourceOrigin::File(FileIdentity::read(&path).unwrap());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            source_identity(&path, Some(identity.clone()), None).unwrap(),
            identity.clone()
        );
        assert_eq!(
            source_identity(&path, None, Some(identity.clone())).unwrap(),
            identity
        );
        assert!(source_identity(&path, None, None).is_err());
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
            source_identity(&path, Some(current.clone()), Some(old)).unwrap(),
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
        photos.configure_memory(512 * 1024 * 1024);
        let mut native = None;
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.working_scale = Scale::new(8).unwrap();
        let loaded = photos
            .load(&paths[0], 0, &cache, &engine, &mut native, &|_| {})
            .unwrap();
        photos
            .render(
                &engine,
                &cache,
                &mut native,
                loaded,
                &settings,
                &|| false,
                &mut Profile::default(),
                &|_| {},
                &|_| {},
            )
            .unwrap()
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
                None,
                &Default::default(),
                1024 * 1024 * 1024,
                cancel,
                &mut Profile::default()
            ))
            .unwrap()
            .unwrap());
        assert!(weak.upgrade().is_none());
        assert_eq!(photos.working.active_bytes(), active_bytes);
        assert!(photos.contains_source(&paths[1], &origin));
        let before = engine.gpu_activity();
        let loaded = photos
            .load(&paths[1], 1, &cache, &engine, &mut native, &|_| {
                panic!("prefetched input must not reopen source")
            })
            .unwrap();
        let mut artifacts = photos
            .render(
                &engine,
                &cache,
                &mut native,
                loaded,
                &settings,
                &|| false,
                &mut Profile::default(),
                &|_| panic!("no source load"),
                &|_| {},
            )
            .unwrap()
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
        photos.working.remove(first_id);
        let used = photos.working.len();
        let before = engine.gpu_activity();
        assert!(!photos
            .prefetch(
                &engine,
                Arc::new(Source::open(&paths[0]).unwrap()),
                &paths[0],
                &settings,
                None,
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
        let mut native = None;
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
                .load(path, 0, &cache, &engine, &mut native, &|_| {})
                .unwrap();
            let weak = loaded.source.as_ref().map(Arc::downgrade);
            let Some(mut artifacts) = photos
                .render(
                    &engine,
                    &cache,
                    &mut native,
                    loaded,
                    &settings,
                    &|| false,
                    &mut Profile::default(),
                    &|_| {},
                    &|_| {},
                )
                .unwrap()
            else {
                panic!("expected reduced photo {round}");
            };
            assert!(native.is_none());
            if let Some(weak) = weak {
                assert!(weak.upgrade().is_none());
            }
            assert!(photos.working.historical_bytes() <= BUDGET);
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
        photos.park_active(&engine, 0);
        super::super::load_native_source(&cache, &engine, &mut native, &paths[0], 0, |_| {})
            .unwrap();
        let mut full = settings.clone();
        full.working_scale = Scale::NATIVE;
        engine
            .render_session(
                &mut native.as_mut().unwrap().1,
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
            .load(&paths[0], 0, &cache, &engine, &mut native, &|_| {})
            .unwrap();
        assert!(photos
            .render(
                &engine,
                &cache,
                &mut native,
                loaded,
                &active_haze,
                &|| false,
                &mut Profile::default(),
                &|_| {},
                &|_| {}
            )
            .unwrap()
            .is_some());
        assert!(native.is_none());
        let loaded = photos
            .load(&paths[0], 0, &cache, &engine, &mut native, &|_| {})
            .unwrap();
        assert!(photos
            .render(
                &engine,
                &cache,
                &mut native,
                loaded,
                &settings,
                &|| false,
                &mut Profile::default(),
                &|_| {},
                &|_| {}
            )
            .unwrap()
            .is_some());
        assert!(native.is_none());
        assert!(photos.working.historical_bytes() <= BUDGET);
        photos.clear(&engine);
        assert!(photos.working.is_empty());
        assert_eq!(engine.cached_preparation_bytes(), 0);
    }
}
