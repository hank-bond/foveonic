//! Keeps each photo's latest edits saved without making rendering wait for disk access.

use eframe::egui;
use foveonic::{render::Settings, sidecar};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{mpsc, Arc, Condvar, Mutex},
    thread::JoinHandle,
};

#[cfg(test)]
#[path = "sidecars_test.rs"]
mod tests;

#[derive(Default)]
struct Pending {
    load: Option<(u64, PathBuf)>,
    saves: BTreeMap<PathBuf, Settings>,
    stop: bool,
}
enum Reply {
    Loaded(u64, PathBuf, Result<Option<Settings>, String>),
    Saved(PathBuf, Settings, Result<(), String>),
}
struct Store {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    replies: mpsc::Receiver<Reply>,
    thread: Option<JoinHandle<()>>,
}
impl Store {
    fn new(ctx: egui::Context, enabled: bool) -> Self {
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let slot = pending.clone();
        let (sender, replies) = mpsc::channel();
        let thread = std::thread::spawn(move || loop {
            enum Job {
                Load(u64, PathBuf, Option<Settings>),
                Save(PathBuf, Settings),
            }
            let job = {
                let (lock, wake) = &*slot;
                let mut state = lock.lock().unwrap();
                while state.load.is_none() && state.saves.is_empty() && !state.stop {
                    state = wake.wait(state).unwrap();
                }
                if let Some((generation, path)) = state.load.take() {
                    let recent = state.saves.get(&path).cloned();
                    Job::Load(generation, path, recent)
                } else if let Some((path, settings)) = state.saves.pop_first() {
                    Job::Save(path, settings)
                } else {
                    break;
                }
            };
            let reply = match job {
                Job::Load(generation, path, recent) => {
                    let result = if enabled {
                        sidecar::load(&path)
                            .map(|saved| recent.or(saved))
                            .map_err(|e| e.to_string())
                    } else {
                        Ok(None)
                    };
                    Reply::Loaded(generation, path, result)
                }
                Job::Save(path, settings) => {
                    let result = sidecar::save(&path, &settings).map_err(|e| e.to_string());
                    if let Err(error) = &result {
                        eprintln!("Cannot save edits for {}: {error}", path.display());
                    }
                    Reply::Saved(path, settings, result)
                }
            };
            let _ = sender.send(reply);
            ctx.request_repaint();
        });
        Self {
            pending,
            replies,
            thread: Some(thread),
        }
    }
    fn load(&self, generation: u64, path: PathBuf) {
        let (lock, wake) = &*self.pending;
        lock.lock().unwrap().load = Some((generation, path));
        wake.notify_one();
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
        state.load = None;
        state.stop = true;
        wake.notify_one();
        drop(state);
        // Switching photos does not cancel unsaved edits. Finish requested writes
        // on shutdown so the latest settings are not lost when the window closes.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub struct Session {
    store: Store,
    enabled: bool,
    generation: u64,
    selected: Option<PathBuf>,
    ready: bool,
    writable: bool,
    dirty: bool,
    last_queued: Option<Settings>,
    unsaved: BTreeMap<PathBuf, Settings>,
    errors: BTreeMap<PathBuf, String>,
}
impl Session {
    pub fn new(ctx: egui::Context, enabled: bool) -> Self {
        Self {
            store: Store::new(ctx, enabled),
            enabled,
            generation: 0,
            selected: None,
            ready: false,
            writable: false,
            dirty: false,
            last_queued: None,
            unsaved: BTreeMap::new(),
            errors: BTreeMap::new(),
        }
    }
    pub fn select(&mut self, path: PathBuf) {
        self.generation += 1;
        self.selected = Some(path.clone());
        self.ready = false;
        self.writable = false;
        self.dirty = false;
        self.last_queued = None;
        self.store.load(self.generation, path);
    }
    pub fn clear(&mut self) {
        self.generation += 1;
        self.selected = None;
        self.ready = false;
        self.writable = false;
        self.dirty = false;
        self.last_queued = None;
        self.store.pending.0.lock().unwrap().load = None;
    }
    pub fn ready(&self) -> bool {
        self.ready
    }

    /// An older load must not replace the settings for the photo now selected.
    pub fn receive(&mut self) -> Option<Settings> {
        let mut loaded = None;
        while let Ok(reply) = self.store.replies.try_recv() {
            match reply {
                Reply::Loaded(generation, path, result)
                    if generation == self.generation && self.selected.as_ref() == Some(&path) =>
                {
                    self.ready = true;
                    match result {
                        Ok(saved) => {
                            self.writable = true;
                            if !self.unsaved.contains_key(&path) {
                                self.errors.remove(&path);
                            }
                            loaded = Some(
                                self.unsaved
                                    .get(&path)
                                    .cloned()
                                    .or(saved)
                                    .unwrap_or_else(|| Settings::new("", "")),
                            );
                        }
                        Err(error) => {
                            self.writable = false;
                            self.errors.insert(
                                path,
                                format!(
                                    "Cannot restore edits; automatic saving is disabled: {error}"
                                ),
                            );
                            loaded = Some(Settings::new("", ""));
                        }
                    }
                }
                Reply::Saved(path, settings, result)
                    if self.unsaved.get(&path) == Some(&settings) =>
                {
                    match result {
                        Ok(()) => {
                            self.unsaved.remove(&path);
                            self.errors.remove(&path);
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
        loaded
    }
    /// A completed frame can lag behind the controls. Save the requested edits
    /// so a quick switch or shutdown does not lose the latest slider value.
    pub fn edited(&mut self, settings: &Settings) {
        if self.ready {
            self.dirty = true;
        }
        self.refresh(settings);
    }
    /// Resolve recorded camera choices after metadata arrives without losing pre-load edits.
    pub fn refresh(&mut self, settings: &Settings) {
        if !self.enabled
            || !self.ready
            || !self.writable
            || !self.dirty
            || self.last_queued.as_ref() == Some(settings)
        {
            return;
        }
        if let Some(path) = &self.selected {
            self.last_queued = Some(settings.clone());
            self.unsaved.insert(path.clone(), settings.clone());
            self.store.save(path.clone(), settings.clone());
        }
    }
    pub fn warning(&self) -> Option<String> {
        self.errors
            .first_key_value()
            .map(|(path, error)| format!("{}: {error}", path.display()))
    }
    pub fn can_retry(&self) -> bool {
        self.errors.keys().any(|path| {
            self.unsaved.contains_key(path)
                && (self.selected.as_deref() != Some(path.as_path()) || self.writable)
        })
    }
    pub fn retry(&mut self) {
        for path in self.errors.keys() {
            if self.selected.as_deref() == Some(path.as_path()) && !self.writable {
                continue;
            }
            if let Some(settings) = self.unsaved.get(path) {
                self.store.save(path.clone(), settings.clone());
            }
        }
    }
}

/// Restored edits must not be replaced by per-photo defaults. Only explicit
/// launch choices override their WB or mode.
pub fn selection_settings(saved: Settings, overrides: &[String; 2]) -> Settings {
    let mut settings = saved;
    for (value, explicit) in [
        (&mut settings.white_balance, &overrides[0]),
        (&mut settings.color_mode, &overrides[1]),
    ] {
        if !explicit.is_empty() {
            value.clone_from(explicit);
        }
    }
    settings
}
