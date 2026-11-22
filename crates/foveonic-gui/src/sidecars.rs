//! Keeps each photo's latest edits saved without making rendering wait for disk access.

use eframe::egui;
use foveonic::diagnostics as trace;
use foveonic::source::MetadataSummary;
use foveonic::{render::Settings, sidecar};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{mpsc, Arc, Condvar, Mutex},
    thread::JoinHandle,
};

#[cfg(test)]
#[path = "sidecars_test.rs"]
mod tests;

#[derive(Default)]
struct Pending {
    saves: BTreeMap<PathBuf, Settings>,
    resolve: VecDeque<(u64, PathBuf, foveonic::render::RecipeDefaults)>,
    stop: bool,
}
enum Reply {
    Saved(PathBuf, Settings, Result<(), String>),
    Resolved(u64, PathBuf, Settings, Option<String>),
}
pub struct Resolved {
    pub path: PathBuf,
    pub settings: Settings,
}
/// A photo's saved recipe, or its source defaults when it has none, with a
/// warning when the recipe could not be used. A malformed recipe opens with
/// defaults and the next edit replaces it. A recipe file that cannot be read
/// also opens with defaults, but saving will not replace it.
fn restore(
    path: &std::path::Path,
    defaults: &foveonic::render::RecipeDefaults,
    enabled: bool,
    cache: &foveonic::decode_cache::Cache,
) -> (Settings, Option<String>) {
    let _restore = trace::scope_with(trace::Category::Cpu, "sidecars.restore", |fields| {
        fields
            .str("source", &path.to_string_lossy())
            .bool("sidecar_enabled", enabled);
    });
    let cached = cache.selection(path);
    trace::event(
        trace::Category::Cache,
        "sidecars.selection_snapshot",
        |fields| {
            fields.bool("hit", cached.is_some());
        },
    );
    let report = |error: &(dyn std::error::Error + 'static)| {
        foveonic::support::report_error(
            "sidecar.load",
            error,
            foveonic::support::OperationContext {
                source: Some(path.to_string_lossy().into_owned()),
                phase: Some("restore edits".into()),
                ..Default::default()
            },
        );
    };
    let (saved, warning) = if !enabled {
        (None, None)
    } else if let Some(cached) = &cached {
        (cached.settings.clone(), None)
    } else {
        match sidecar::load(path) {
            Ok(sidecar::Saved::Malformed(error)) => {
                report(error.as_ref());
                (
                    None,
                    Some(format!("Recipe malformed or from a newer version; opened with defaults, and the next edit replaces it: {error}")),
                )
            }
            Ok(saved) => (saved.settings(), None),
            Err(error) => {
                report(error.as_ref());
                (
                    None,
                    Some(format!(
                        "Recipe unreadable; opened with defaults, and edits are not saved over it: {error}"
                    )),
                )
            }
        }
    };
    // Only a missing recipe gets source defaults. Saved recipes,
    // including explicit neutral values, are complete edits.
    let settings = saved.unwrap_or_else(|| {
        cached
            .map(|s| Ok(MetadataSummary::from_properties(s.properties)))
            .unwrap_or_else(|| cache.metadata_summary(path))
            .ok()
            .and_then(|summary| source_defaults(summary, defaults).ok())
            .unwrap_or_default()
    });
    (settings, warning)
}

/// An immutable recipe snapshot for optional work on already-local sources.
/// Resolution stays here with foreground recipe restoration and never probes a card.
#[derive(Clone, PartialEq)]
pub(crate) struct CachedRecipes {
    defaults: foveonic::render::RecipeDefaults,
    enabled: bool,
    recent: BTreeMap<PathBuf, Settings>,
}
impl CachedRecipes {
    pub fn resolve(
        &self,
        path: &std::path::Path,
        selection: foveonic::decode_cache::Selection,
    ) -> foveonic::source::Result<Settings> {
        if let Some(settings) = self.recent.get(path) {
            return Ok(settings.clone());
        }
        if let Some(settings) = selection.settings.filter(|_| self.enabled) {
            return Ok(settings);
        }
        source_defaults(
            MetadataSummary::from_properties(selection.properties),
            &self.defaults,
        )
    }
}

fn source_defaults(
    summary: MetadataSummary,
    defaults: &foveonic::render::RecipeDefaults,
) -> foveonic::source::Result<Settings> {
    if defaults.is_empty() {
        capture_defaults(summary)
    } else {
        defaults.from_properties(&summary.properties)
    }
}

struct Store {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    replies: mpsc::Receiver<Reply>,
    thread: Option<JoinHandle<()>>,
}
impl Store {
    fn new(ctx: egui::Context, enabled: bool, cache: Arc<foveonic::decode_cache::Cache>) -> Self {
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let slot = pending.clone();
        let (sender, replies) = mpsc::channel();
        let builder = std::thread::Builder::new().name("sidecars".into());
        let thread = builder.spawn(move || loop {
            enum Job {
                // Boxed so the queue's job stays small beside resolutions.
                Save(PathBuf, Box<Settings>),
                Resolve(u64, PathBuf, foveonic::render::RecipeDefaults),
            }
            let job = {
                let (lock, wake) = &*slot;
                let mut state = lock.lock().unwrap();
                while state.saves.is_empty() && state.resolve.is_empty() && !state.stop {
                    state = wake.wait(state).unwrap();
                }
                if let Some((path, settings)) = state.saves.pop_first() {
                    Job::Save(path, Box::new(settings))
                } else if let Some((generation, path, defaults)) = state.resolve.pop_front() {
                    Job::Resolve(generation, path, defaults)
                } else {
                    break;
                }
            };
            let reply = match job {
                Job::Resolve(generation, path, defaults) => {
                    let _context = trace::enabled().then(|| {
                        trace::enter(crate::timing::context_for(
                            0,
                            generation,
                            &path,
                            "sidecar_resolve",
                        ))
                    });
                    let _job =
                        trace::scope_with(trace::Category::Cpu, "sidecars.resolve_job", |fields| {
                            fields
                                .u64("generation", generation)
                                .str("source", &path.to_string_lossy());
                        });
                    let (settings, warning) = restore(&path, &defaults, enabled, &cache);
                    Reply::Resolved(generation, path, settings, warning)
                }
                Job::Save(path, settings) => {
                    let _context = trace::enabled().then(|| {
                        trace::enter(crate::timing::context_for(0, 0, &path, "sidecar_save"))
                    });
                    let _job =
                        trace::scope_with(trace::Category::Io, "sidecars.save_job", |fields| {
                            fields.str("source", &path.to_string_lossy());
                        });
                    let result = sidecar::save(&path, &settings).map_err(|error| {
                        foveonic::support::report_error(
                            "sidecar.save",
                            error.as_ref(),
                            foveonic::support::OperationContext {
                                source: Some(path.to_string_lossy().into_owned()),
                                phase: Some("save edits".into()),
                                settings: Some(serde_json::to_value(&settings).unwrap_or_default()),
                                ..Default::default()
                            },
                        );
                        error.to_string()
                    });
                    trace::event(trace::Category::Io, "sidecars.save_result", |fields| {
                        fields.bool("success", result.is_ok());
                    });
                    if result.is_ok() {
                        cache.saved_settings(&path, &settings);
                    }
                    if let Err(error) = &result {
                        eprintln!("Cannot save edits for {}: {error}", path.display());
                    }
                    Reply::Saved(path, *settings, result)
                }
            };
            let _ = sender.send(reply);
            ctx.request_repaint();
        });
        let thread = thread.expect("start sidecars thread");
        Self {
            pending,
            replies,
            thread: Some(thread),
        }
    }
    fn save(&self, path: PathBuf, settings: Settings) {
        let (lock, wake) = &*self.pending;
        lock.lock().unwrap().saves.insert(path, settings);
        wake.notify_one();
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let (lock, wake) = &*self.pending;
        let mut state = lock.lock().unwrap();
        state.resolve.clear();
        state.stop = true;
        wake.notify_one();
        drop(state);
        // Changing photos does not cancel unsaved edits. Finish requested writes
        // on shutdown so the latest settings are not lost when the window closes.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub struct Session {
    defaults: foveonic::render::RecipeDefaults,
    store: Store,
    enabled: bool,
    unsaved: BTreeMap<PathBuf, Settings>,
    errors: BTreeMap<PathBuf, String>,
    memory_only: bool,
    requested: BTreeMap<PathBuf, Settings>,
    resolve_generation: u64,
    resolved: Vec<Resolved>,
}
impl Session {
    pub fn new(
        ctx: egui::Context,
        enabled: bool,
        cache: Arc<foveonic::decode_cache::Cache>,
    ) -> Self {
        Self {
            defaults: Default::default(),
            store: Store::new(ctx, enabled, cache),
            enabled,
            unsaved: BTreeMap::new(),
            errors: BTreeMap::new(),
            memory_only: false,
            requested: BTreeMap::new(),
            resolve_generation: 0,
            resolved: Vec::new(),
        }
    }
    pub fn set_defaults(&mut self, defaults: &foveonic::render::RecipeDefaults) {
        self.defaults = defaults.clone();
    }

    /// Import review retains requested edits until another folder is opened.
    /// Already queued ordinary-folder saves still finish on the existing writer.
    pub fn set_memory_only(&mut self, enabled: bool) {
        self.memory_only = enabled;
        self.requested.clear();
        self.resolve_settings(&[]);
    }

    /// Batch submission uses the latest controls even after an asynchronous save finishes.
    pub fn requested_settings(&self, path: &std::path::Path) -> Option<&Settings> {
        self.requested.get(path).or_else(|| self.unsaved.get(path))
    }

    pub fn cached_recipes(&self, paths: &[PathBuf]) -> CachedRecipes {
        CachedRecipes {
            defaults: self.defaults.clone(),
            enabled: self.enabled,
            recent: paths
                .iter()
                .filter_map(|path| {
                    self.requested_settings(path)
                        .map(|settings| (path.clone(), settings.clone()))
                })
                .collect(),
        }
    }

    /// Resolve one replaceable editing subset on the existing background recipe owner.
    pub fn resolve_settings(&mut self, paths: &[PathBuf]) {
        self.resolve_generation += 1;
        self.resolved.clear();
        let mut pending = VecDeque::new();
        for path in paths {
            if let Some(settings) = self.requested_settings(path).cloned() {
                self.resolved.push(Resolved {
                    path: path.clone(),
                    settings,
                });
            } else {
                pending.push_back((self.resolve_generation, path.clone(), self.defaults.clone()));
            }
        }
        let (lock, wake) = &*self.store.pending;
        lock.lock().unwrap().resolve = pending;
        wake.notify_one();
    }
    pub fn take_resolved(&mut self) -> Vec<Resolved> {
        std::mem::take(&mut self.resolved)
    }

    /// Collect finished resolutions and saves. An older resolution is ignored.
    pub fn receive(&mut self) {
        while let Ok(reply) = self.store.replies.try_recv() {
            match reply {
                Reply::Resolved(generation, path, settings, warning)
                    if generation == self.resolve_generation =>
                {
                    if let Some(warning) = warning {
                        self.errors.insert(path.clone(), warning);
                    } else if !self.unsaved.contains_key(&path) {
                        self.errors.remove(&path);
                    }
                    trace::event(
                        trace::Category::Coordinator,
                        "sidecars.resolve_received",
                        |fields| {
                            fields
                                .u64("generation", generation)
                                .str("source", &path.to_string_lossy());
                        },
                    );
                    // An edit can arrive while a background read is completing.
                    let settings = self.requested_settings(&path).cloned().unwrap_or(settings);
                    self.resolved.push(Resolved { path, settings });
                }
                Reply::Saved(path, settings, result)
                    if self.unsaved.get(&path) == Some(&settings) =>
                {
                    match result {
                        Ok(()) => {
                            self.unsaved.remove(&path);
                            if self.errors.remove(&path).is_some() {
                                foveonic::support::event(
                                    foveonic::support::Level::Info,
                                    "sidecar.recovered",
                                    "Edits were saved after an earlier failure",
                                    serde_json::json!({"source":crate::support::path(&path)}),
                                );
                            }
                        }
                        Err(error) => {
                            self.errors
                                .insert(path, format!("Edits are not saved: {error}"));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    /// Record a photo's edited settings and save them through the coalescing writer.
    pub fn apply(&mut self, path: PathBuf, settings: Settings) {
        if self.requested.get(&path) == Some(&settings) {
            return;
        }
        self.requested.insert(path.clone(), settings.clone());
        if self.enabled && !self.memory_only {
            self.unsaved.insert(path.clone(), settings.clone());
            self.store.save(path, settings);
        }
    }
    pub fn warning(&self) -> Option<String> {
        self.warnings().next()
    }
    pub fn warnings(&self) -> impl Iterator<Item = String> + '_ {
        self.errors
            .iter()
            .map(|(path, error)| format!("{}: {error}", path.display()))
    }
    pub fn can_retry(&self) -> bool {
        self.errors
            .keys()
            .any(|path| self.unsaved.contains_key(path))
    }
    pub fn retry(&mut self) {
        for path in self.errors.keys() {
            if let Some(settings) = self.unsaved.get(path) {
                self.store.save(path.clone(), settings.clone());
            }
        }
    }
}

/// Resolve scalar source defaults off the UI thread, before launch choices and
/// edits are applied. CAMF preset catalogues arrive separately with full metadata.
pub(crate) fn capture_defaults(summary: MetadataSummary) -> foveonic::source::Result<Settings> {
    if summary.family.is_none() {
        return Ok(Settings::default());
    }
    foveonic::render::camera_settings_from_properties(summary.properties)
}
