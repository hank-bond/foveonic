//! Prepares ready decoded neighbors during idle time using the render worker's existing owners.

use super::processing::Photos;
use crate::{rendering::request::camera_preview_scale, sidecars::CachedRecipes};
use foveonic::{
    decode_cache::Cache,
    diagnostics::{self as trace, Category},
    noise_calibration::Calibration,
    profile::Profile,
    render::{Engine, MemoryReservation, ProcessingQuality, WorkingSet},
    source::SourceOrigin,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
pub struct Config {
    pub processing_quality: ProcessingQuality,
    pub recipes: CachedRecipes,
    pub calibration: Option<Arc<Calibration>>,
}
impl PartialEq for Config {
    fn eq(&self, other: &Self) -> bool {
        self.processing_quality == other.processing_quality
            && self.recipes == other.recipes
            && self.calibration.as_ref().map(|c| c.signature())
                == other.calibration.as_ref().map(|c| c.signature())
    }
}

/// What idle preparation last found nothing to do for: a configuration and the
/// cache's preparation window. The window grows within one generation as
/// prefetched copies are verified, so its contents count, not only its generation.
pub struct Exhausted {
    config: Arc<Config>,
    window: (u64, Vec<PathBuf>),
}
impl Exhausted {
    pub fn new(config: Arc<Config>, window: (u64, Vec<PathBuf>)) -> Self {
        Self { config, window }
    }
    pub fn covers(&self, config: &Arc<Config>, window: &(u64, Vec<PathBuf>)) -> bool {
        Arc::ptr_eq(&self.config, config) && &self.window == window
    }
}

#[derive(Default)]
pub struct Preparation {
    config: Option<Arc<Config>>,
    generation: u64,
    limit: u64,
    attempted: Vec<(PathBuf, SourceOrigin)>,
}
impl Preparation {
    /// One attempt per source and neighborhood. Eviction must not cause an idle
    /// prepare/evict loop. Interrupted attempts remain eligible for retry.
    #[allow(clippy::too_many_arguments)]
    pub fn run_one(
        &mut self,
        config: &Arc<Config>,
        cache: &Cache,
        engine: &mut Engine,
        working: &mut WorkingSet,
        photos: &mut Photos,
        reservation: &MemoryReservation,
        limit: u64,
        pending: &dyn Fn() -> bool,
    ) -> bool {
        let (generation, paths) = cache.preparation_window();
        if self.generation != generation
            || self.limit != limit
            || self
                .config
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, config))
        {
            self.generation = generation;
            self.limit = limit;
            self.config = Some(config.clone());
            self.attempted.clear();
        }
        let Some((path, origin)) = paths
            .into_iter()
            .filter_map(|path| cache.source_origin(&path).map(|origin| (path, origin)))
            .find(|(path, origin)| {
                !self.attempted.contains(&(path.clone(), origin.clone()))
                    && !photos.contains_reduced(working, path, origin)
            })
        else {
            return false;
        };
        let ran = cache.priority().speculate(|interrupted| {
            let _context = trace::enabled().then(|| {
                trace::enter(crate::timing::context_for(
                    0,
                    generation,
                    &path,
                    "thumb_prefetch",
                ))
            });
            let _scope =
                trace::scope_with(Category::Cache, "cache.processing.prefetch", |fields| {
                    fields
                        .str("source", &path.to_string_lossy())
                        .u64("generation", generation);
                });
            let cancel = || interrupted() || pending() || !cache.preparation_is_current(generation);
            let result = (|| -> foveonic::source::Result<bool> {
                if cancel() {
                    return Ok(false);
                }
                if engine.memory_resources(working, reservation).total_bytes >= limit {
                    trace::event(
                        Category::Cache,
                        "cache.processing.prefetch_skipped",
                        |fields| {
                            fields
                                .str("reason", "memory-budget")
                                .u64("budget_bytes", limit);
                        },
                    );
                    return Ok(false);
                }
                let Some(selection) = cache.selection(&path) else {
                    return Ok(false);
                };
                let mut settings = config.recipes.resolve(&path, selection)?;
                settings.processing_quality = config.processing_quality;
                let Some(source) = photos
                    .shared_source(working, &path, &origin)
                    .or_else(|| cache.open_cached(&path, |_| {}))
                else {
                    return Ok(false);
                };
                if source.origin() != &origin || cancel() {
                    return Ok(false);
                }
                if settings.validate_for_camera(&source.metadata).is_err() {
                    return Ok(false);
                }
                settings.working_scale = camera_preview_scale(
                    foveonic::render::Scale::new(8).unwrap(),
                    &source.metadata,
                )?;
                settings.preview_region = None;
                settings.color_range_preview = None;
                settings.dehaze_estimate = settings.dehaze_auto();
                engine.set_noise_calibration(config.calibration.clone());
                photos.prefetch(
                    working,
                    engine,
                    source,
                    &path,
                    &settings,
                    reservation,
                    limit,
                    &cancel,
                    &mut Profile::new(crate::timing::mode()),
                )
            })();
            let interrupted = cancel();
            trace::event(
                Category::Cache,
                "cache.processing.prefetch_result",
                |fields| {
                    fields.str("source", &path.to_string_lossy()).str(
                        "outcome",
                        if interrupted {
                            "interrupted"
                        } else {
                            match result {
                                Ok(true) => "retained",
                                Ok(false) => "not-retained",
                                Err(_) => "failed",
                            }
                        },
                    );
                    if let Err(error) = &result {
                        fields.str("error", &error.to_string());
                    }
                },
            );
            !interrupted
        });
        if ran == Some(true) {
            self.attempted.push((path, origin));
        }
        ran.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_that_grows_within_its_generation_is_prepared_again() {
        let sidecars = crate::sidecars::Session::new(
            eframe::egui::Context::default(),
            false,
            Arc::new(Cache::new()),
        );
        let config = Arc::new(Config {
            processing_quality: ProcessingQuality::Quality,
            recipes: sidecars.cached_recipes(&[]),
            calibration: None,
        });
        let exhausted = Exhausted::new(config.clone(), (2, Vec::new()));
        assert!(exhausted.covers(&config, &(2, Vec::new())));
        assert!(
            !exhausted.covers(&config, &(2, vec![PathBuf::from("verified.x3f")])),
            "a newly verified neighbor needs preparing"
        );
        assert!(!exhausted.covers(&Arc::new((*config).clone()), &(2, Vec::new())));
    }
}
