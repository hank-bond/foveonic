//! Prepares ready decoded neighbors during idle time using the render worker's existing owners.

use super::reduced::Photos;
use crate::{
    export::resolve_camera_settings, preferences::PreviewMode,
    rendering::request::camera_preview_scale, sidecars::CachedRecipes,
};
use foveonic::{
    decode_cache::Cache,
    diagnostics::{self as trace, Category},
    noise_calibration::Calibration,
    profile::Profile,
    render::{Engine, MemoryReservation, RenderSession},
    source::SourceOrigin,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
pub struct Config {
    pub recipes: CachedRecipes,
    pub overrides: [String; 2],
    pub calibration: Option<Arc<Calibration>>,
}
impl PartialEq for Config {
    fn eq(&self, other: &Self) -> bool {
        self.recipes == other.recipes
            && self.overrides == other.overrides
            && self.calibration.as_ref().map(|c| c.signature())
                == other.calibration.as_ref().map(|c| c.signature())
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
        reduced: &mut Photos,
        native: Option<&RenderSession>,
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
                    && !reduced.contains_source(path, origin)
            })
        else {
            return false;
        };
        let ran = cache.speculate(|interrupted| {
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
                if !reduced.has_prefetch_capacity(engine, native, reservation, limit) {
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
                let mut settings = crate::sidecars::selection_settings(
                    config.recipes.resolve(&path, selection)?,
                    &config.overrides,
                );
                let Some(source) = native
                    .filter(|session| session.source().origin() == &origin)
                    .map(RenderSession::shared_source)
                    .or_else(|| cache.open_cached(&path, |_| {}))
                else {
                    return Ok(false);
                };
                if source.origin() != &origin || cancel() {
                    return Ok(false);
                }
                if !resolve_camera_settings(&mut settings, &source.metadata).is_empty() {
                    return Ok(false);
                }
                settings.working_scale =
                    camera_preview_scale(PreviewMode::Thumb, &source.metadata)?;
                settings.preview_region = None;
                settings.color_range_preview = None;
                settings.dehaze_estimate = settings.dehaze_auto();
                engine.set_noise_calibration(config.calibration.clone());
                reduced.prefetch(
                    engine,
                    source,
                    &path,
                    &settings,
                    native,
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
