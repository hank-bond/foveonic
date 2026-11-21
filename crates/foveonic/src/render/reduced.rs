//! Prepares sensor data downsampled to the working scale and renders reduced working photos.

use super::{recipes, Engine, PhotoRender, Scale, Settings};
use crate::{
    interactive::{Cache, Execution, Superseded},
    profile::Profile,
    source::{Metadata, Result, Source, SourceOrigin},
};
use std::sync::Arc;

fn failure(
    error: Box<dyn std::error::Error + Send + Sync>,
    phase: &str,
    metadata: &Metadata,
    settings: &Settings,
) -> Box<dyn std::error::Error + Send + Sync> {
    super::execute::report_failure(
        error,
        phase,
        metadata,
        serde_json::json!({"settings":crate::support::settings_snapshot(settings)}),
    )
}

/// The caller recovers authoritative native samples outside the processing core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeedsOriginal {
    UpstreamChanged,
    EngineChanged,
}

pub enum ReducedPreparation {
    Prepared(Box<ReducedPhoto>),
    Superseded,
}

/// Retains immutable recipe-declared camera inputs, never decoded source samples.
/// The complete camera and photographic recipes still author every render.
pub struct ReducedPhoto {
    pub(super) metadata: Arc<Metadata>,
    pub(super) origin: SourceOrigin,
    pub(super) owner: Arc<()>,
    pub(super) scale: Scale,
    pub(super) cache: Cache,
    pub(super) preparation: super::source_preparation::SourcePreparation,
    metadata_bytes: u64,
    pub(super) last_settings: Option<Box<Settings>>,
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
pub struct PhotoResources {
    pub metadata_bytes: u64,
    pub owner_bytes: u64,
    pub cache_gpu_bytes: u64,
    pub cache_structure_bytes: u64,
    pub parameter_payload_bytes: u64,
    pub preparation_bytes: u64,
    pub source_bytes: u64,
    pub output_parameters: Vec<ParameterResources>,
}
impl ReducedPhoto {
    /// Inspect resource ownership outside timed editing interactions.
    pub fn resources(&self) -> PhotoResources {
        let mut retained = crate::recipe::RetainedBytes::default();
        let cache_containers = self.cache.account_keys(&mut retained);
        let payloads = retained.payload_bytes();
        let cache_bytes = retained.bytes();
        let scalar_containers = self.preparation.account(&mut retained);
        PhotoResources {
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
    pub(super) fn account_keys(&self, retained: &mut crate::recipe::RetainedBytes) -> usize {
        self.cache.account_keys(retained) + self.preparation.account(retained)
    }
    pub(super) fn owned_cpu_bytes(&self) -> u64 {
        self.metadata_bytes
            + self.origin.allocated_bytes() as u64
            + std::mem::size_of::<Self>() as u64
            + self.last_settings.as_ref().map_or(0, |settings| {
                std::mem::size_of::<Settings>()
                    + settings.white_balance.capacity()
                    + settings.color_mode.capacity()
            }) as u64
    }
    pub(super) fn buffers(&self) -> impl Iterator<Item = &crate::gpu::ReadGuard> {
        self.cache.buffers()
    }
}

pub(super) fn unique_buffer_bytes<'a>(
    buffers: impl Iterator<Item = &'a crate::gpu::ReadGuard>,
) -> u64 {
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
    pub fn prepare_reduced_photo(
        &self,
        source: impl Into<Arc<Source>>,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<ReducedPreparation> {
        let source = source.into();
        settings.validate_values().map_err(|error| {
            failure(error, "validate-reduced-recipe", &source.metadata, settings)
        })?;
        profile
            .mode
            .check_gpu_features(self.device().features())
            .map_err(|error| {
                failure(
                    error.into(),
                    "validate-profile-features",
                    &source.metadata,
                    settings,
                )
            })?;
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
            let graph = recipes::sensor_data_graph(
                self,
                recipes::RecipeInput::Preparing {
                    source: &source,
                    preparation: &preparation,
                },
                settings,
                profile,
                &mut execution,
            )?;
            let roots = &graph.sensor_data_outputs;
            if roots.is_empty() {
                return Err("camera recipe declared no immutable sensor dependencies".into());
            }
            execution.plan_sensor_data(&graph, roots)?;
            self.run_graph(
                super::Input::Source(&source),
                &graph,
                roots,
                profile,
                &mut execution,
            )?;
            Ok(())
        })();
        let retained = cache.finish_sensor_data();
        self.release_native_preparation(&source.metadata);
        match prepared {
            Ok(()) if !retained => Err(failure(
                "required camera dependencies exceed the retained input budget".into(),
                "retain-camera-dependencies",
                &source.metadata,
                settings,
            )),
            Ok(()) => Ok(ReducedPreparation::Prepared(Box::new(ReducedPhoto {
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
            Err(error) => Err(failure(
                error,
                "prepare-camera-dependencies",
                &source.metadata,
                settings,
            )),
        }
    }

    /// Recover missing sensor data while preserving everything still valid.
    /// The caller supplies a freshly verified load of the same original; metadata
    /// remains the numerical owner and is never replaced to forge source identity.
    pub fn refresh_reduced_photo(
        &self,
        session: &mut ReducedPhoto,
        source: &Source,
        settings: &Settings,
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<Option<()>> {
        settings.validate_values().map_err(|error| {
            failure(error, "validate-reduced-recipe", &source.metadata, settings)
        })?;
        if !Arc::ptr_eq(&session.owner, &self.owner) {
            return Err(failure(
                "camera dependencies belong to another engine".into(),
                "verify-engine",
                &source.metadata,
                settings,
            ));
        }
        if session.origin != *source.origin() {
            return Err(failure(
                "refreshed source is not the same verified original".into(),
                "verify-source",
                &source.metadata,
                settings,
            ));
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
            let graph = recipes::sensor_data_graph(
                self,
                recipes::RecipeInput::Preparing {
                    source,
                    preparation: &session.preparation,
                },
                settings,
                profile,
                &mut execution,
            )?;
            execution.plan_sensor_data(&graph, &graph.sensor_data_outputs)?;
            self.run_graph(
                super::Input::Source(source),
                &graph,
                &graph.sensor_data_outputs,
                profile,
                &mut execution,
            )?;
            Ok(())
        })();
        let retained = session.cache.finish_sensor_data();
        self.release_native_preparation(&source.metadata);
        match prepared {
            Ok(()) if !retained => Err(failure(
                "required camera dependencies exceed the retained input budget".into(),
                "retain-camera-dependencies",
                &source.metadata,
                settings,
            )),
            Ok(()) => {
                session.metadata_bytes = source.metadata.retained_bytes();
                session.metadata = source.metadata.clone();
                session.scale = settings.working_scale;
                Ok(Some(()))
            }
            Err(error) if error.downcast_ref::<Superseded>().is_some() => Ok(None),
            Err(error) => Err(failure(
                error,
                "refresh-camera-dependencies",
                &source.metadata,
                settings,
            )),
        }
    }

    pub fn render_reduced_photo(
        &self,
        session: &mut ReducedPhoto,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<PhotoRender> {
        self.render_reduced_photo_observed(session, settings, names, cancel, profile, None)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_reduced_photo_with_progress(
        &self,
        session: &mut ReducedPhoto,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: impl Fn(super::RenderProgress),
    ) -> Result<PhotoRender> {
        self.render_reduced_photo_observed(
            session,
            settings,
            names,
            cancel,
            profile,
            Some(&progress),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_reduced_photo_observed(
        &self,
        session: &mut ReducedPhoto,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: Option<&dyn Fn(super::RenderProgress)>,
    ) -> Result<PhotoRender> {
        settings.validate_values().map_err(|error| {
            failure(
                error,
                "validate-reduced-recipe",
                &session.metadata,
                settings,
            )
        })?;
        if !Arc::ptr_eq(&session.owner, &self.owner) {
            return Ok(PhotoRender::NeedsOriginal(NeedsOriginal::EngineChanged));
        }
        session.cache.bind(&self.owner);
        session.cache.bind_source(&session.origin);
        let mut execution = Execution {
            cache: Some(&mut session.cache),
            cancel: Some(&cancel),
            progress,
            ..Execution::default()
        };
        let result = self.run_recipe(
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
            session.cache.commit(None);
        }
        session.cache.finish();
        match result {
            Ok(artifacts) => {
                session.scale = settings.working_scale;
                session.last_settings = Some(Box::new(settings.clone()));
                Ok(PhotoRender::Rendered(artifacts))
            }
            Err(error)
                if error
                    .downcast_ref::<super::execute::SourceRequired>()
                    .is_some() =>
            {
                Ok(PhotoRender::NeedsOriginal(NeedsOriginal::UpstreamChanged))
            }
            Err(error) if error.downcast_ref::<Superseded>().is_some() => {
                Ok(PhotoRender::Superseded)
            }
            Err(error) => Err(failure(
                error,
                "render-reduced",
                &session.metadata,
                settings,
            )),
        }
    }
}

#[cfg(test)]
#[path = "reduced_test.rs"]
mod tests;
