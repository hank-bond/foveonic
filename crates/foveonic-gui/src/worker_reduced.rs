//! Reuses bounded reduced-photo inputs while native requests keep the existing worker session.

use super::Request;
use crate::preferences::PreviewMode;
use foveonic::{
    decode_cache::Cache,
    importing::Identity,
    inspection::Artifact,
    profile::Profile,
    render::{
        Engine, ReducedPreparation, ReducedRender, ReducedWorkingSet, RenderProgress,
        RenderSession, Settings,
    },
    source::{LoadStage, Metadata, Result, Source},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

const BUDGET: u64 = 128 * 1024 * 1024;

fn source_identity(path: &Path, verified: Option<Identity>) -> Result<Identity> {
    verified.map_or_else(|| Identity::read(path), Ok)
}

struct Photo {
    path: PathBuf,
    identity: Identity,
    id: u64,
}
pub(super) struct Loaded {
    pub metadata: Arc<Metadata>,
    path: PathBuf,
    identity: Identity,
    id: u64,
    source: Option<Arc<Source>>,
    revision: u64,
}
pub(super) enum Attempt {
    Rendered(Option<Vec<Artifact>>),
    Native(Option<Arc<Source>>),
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
    pub fn requested(request: &Request) -> bool {
        matches!(
            request.preview_quality,
            PreviewMode::Draft | PreviewMode::Thumb
        ) && request.output == "final"
            && !request.comparison
            && request.export.is_none()
            && request.settings.preview_region.is_none()
    }
    pub fn clear(&mut self, engine: &Engine) {
        *self = Self::default();
        engine.clear_preparation();
    }
    fn prune(&mut self) {
        self.photos.retain(|photo| self.working.contains(photo.id));
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
        self.prune();
        let identity = source_identity(path, cache.import_identity(path))?;
        if let Some(photo) = self
            .photos
            .iter()
            .find(|photo| photo.path == path && photo.identity == identity)
        {
            if let Some(metadata) = self.working.metadata(photo.id) {
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
        let changed = self
            .photos
            .iter()
            .any(|photo| photo.path == path && photo.identity != identity);
        if let Some(old) = self.photos.iter().find(|photo| photo.path == path) {
            self.working.remove(old.id);
        }
        self.prune();
        let (metadata, source) = if let Some((_, session)) = native
            .as_ref()
            .filter(|(native_path, _)| native_path == path && !changed)
        {
            (session.metadata().clone(), None)
        } else {
            if let Some((_, previous)) = native.take() {
                engine.release_native_preparation(previous.metadata());
            }
            let source = open_source(cache, path, revision, progress)?;
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
    ) -> Result<Attempt> {
        if Engine::reduced_session_requirement(&loaded.metadata, settings).is_some() {
            return Ok(Attempt::Native(loaded.source));
        }
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
                    *native = None;
                    self.prune();
                    return Ok(Attempt::Rendered(Some(artifacts)));
                }
                ReducedRender::Superseded => return Ok(Attempt::Rendered(None)),
                ReducedRender::NeedsOriginal(_) => {}
            }
        }
        let original = if let Some(source) = loaded.source.take() {
            source
        } else if native
            .as_ref()
            .is_some_and(|(path, _)| *path == loaded.path)
        {
            native.take().unwrap().1.into_source()
        } else {
            *native = None;
            open_source(cache, &loaded.path, loaded.revision, loaded_progress)?
        };
        let preparation = engine.prepare_reduced_session(original, settings, cancel, profile)?;
        let mut session = match preparation {
            ReducedPreparation::Prepared(session) => session,
            ReducedPreparation::NeedsOriginal { source, .. } => {
                return Ok(Attempt::Native(Some(source)))
            }
            ReducedPreparation::Superseded => return Ok(Attempt::Rendered(None)),
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
        self.photos.retain(|photo| photo.path != loaded.path);
        self.photos.push(Photo {
            path: loaded.path,
            identity: loaded.identity,
            id: loaded.id,
        });
        self.prune();
        *native = None;
        match outcome {
            ReducedRender::Rendered(artifacts) => Ok(Attempt::Rendered(Some(artifacts))),
            ReducedRender::Superseded => Ok(Attempt::Rendered(None)),
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
    use foveonic::{inspection::Content, render::PreviewQuality};

    fn request() -> Request {
        Request {
            framing_preview: false,
            calibration: None,
            rendition: Default::default(),
            revision: 1,
            path: "photo.x3f".into(),
            settings: Settings::new("Sunlight", "Standard"),
            preview_quality: PreviewMode::Thumb,
            output: "final".into(),
            comparison: false,
            export: None,
        }
    }
    #[test]
    fn verified_identity_snapshot_keeps_offline_reduced_edits_independent_of_source_stats() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("offline.x3f");
        std::fs::write(&path, b"identity").unwrap();
        let identity = Identity::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            source_identity(&path, Some(identity.clone())).unwrap(),
            identity
        );
        assert!(source_identity(&path, None).is_err());
    }
    #[test]
    fn only_explicit_small_final_requests_use_the_reduced_set() {
        let mut request = request();
        assert!(Photos::requested(&request));
        request.preview_quality = PreviewMode::Draft;
        assert!(Photos::requested(&request));
        for quality in [PreviewMode::Full, PreviewMode::Fast, PreviewMode::Smart] {
            request.preview_quality = quality;
            assert!(!Photos::requested(&request));
        }
        request.preview_quality = PreviewMode::Thumb;
        request.comparison = true;
        assert!(!Photos::requested(&request));
        request.comparison = false;
        request.output = "linear.base".into();
        assert!(!Photos::requested(&request));
    }

    #[test]
    fn supported_classic_small_policy_does_not_require_fast_or_reduced_session_ownership() {
        use foveonic::source::{Matrix, MatrixEncoding};
        let metadata = Metadata {
            width: 256,
            height: 192,
            rotation: 0,
            processing_version: None,
            properties: [
                ("CAMMODEL".into(), "SIGMA SD14".into()),
                ("ISO".into(), "100".into()),
            ]
            .into(),
            matrices: [(
                "ActiveImageArea".into(),
                Matrix {
                    encoding: MatrixEncoding::Float32,
                    representation: "test",
                    dimensions: vec![4],
                    values: vec![7., 5., 248., 186.],
                },
            )]
            .into(),
            lists: Default::default(),
        };
        let mut request = request();
        assert!(!request.settings.fast_preview_available(&metadata));
        assert!(request
            .settings
            .small_preview_available(&metadata, PreviewQuality::Thumb));
        assert_eq!(
            super::super::effective_preview_quality(&request, &request.settings, &metadata),
            PreviewQuality::Thumb
        );
        let mut resolved = request.settings.clone();
        resolved.preview_quality = PreviewQuality::Thumb;
        assert_eq!(
            Engine::reduced_session_requirement(&metadata, &resolved),
            Some(foveonic::render::NeedsOriginal::UnsupportedFamily)
        );
        request.settings.preview_region = Some([0, 0, 64, 64]);
        assert_eq!(
            super::super::effective_preview_quality(&request, &request.settings, &metadata),
            PreviewQuality::Full
        );
        request.settings.preview_region = None;
        request.settings.output_size = Some([64, 64]);
        assert_eq!(
            super::super::effective_preview_quality(&request, &request.settings, &metadata),
            PreviewQuality::Full
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
        settings.preview_quality = PreviewQuality::Thumb;
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
            let Attempt::Rendered(Some(mut artifacts)) = photos
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
            assert!(photos.working.retained_bytes(&engine) <= BUDGET);
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
        super::super::load_native_source(&cache, &engine, &mut native, &paths[0], 0, |_| {})
            .unwrap();
        let mut full = settings.clone();
        full.preview_quality = PreviewQuality::Full;
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
        let original = native.as_ref().unwrap().1.source().samples().as_ptr();
        let cached = native.as_ref().unwrap().1.cached_bytes();
        let mut active_haze = settings.clone();
        active_haze.dehaze.amount = 10.;
        let loaded = photos
            .load(&paths[0], 0, &cache, &engine, &mut native, &|_| {})
            .unwrap();
        assert!(matches!(
            photos
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
                .unwrap(),
            Attempt::Native(None)
        ));
        assert_eq!(
            native.as_ref().unwrap().1.source().samples().as_ptr(),
            original
        );
        assert_eq!(
            native.as_ref().unwrap().1.cached_bytes(),
            cached,
            "native-only fallback stays warm"
        );
        let loaded = photos
            .load(&paths[0], 0, &cache, &engine, &mut native, &|_| {})
            .unwrap();
        assert!(matches!(
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
                    &|_| {}
                )
                .unwrap(),
            Attempt::Rendered(Some(_))
        ));
        assert!(native.is_none());
        assert!(engine.cached_preparation_bytes() < 8 * 1024 * 1024);
        photos.clear(&engine);
        assert!(photos.working.is_empty());
        assert_eq!(engine.cached_preparation_bytes(), 0);
    }
}
