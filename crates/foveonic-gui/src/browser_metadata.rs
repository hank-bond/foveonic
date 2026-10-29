//! Loads browser capture metadata and folder ratings off-thread and coalesces independent rating saves.

use super::query::Entry;
use eframe::egui;
use foveonic::{
    decode_cache::Cache,
    review::{self, Rating},
};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Condvar, Mutex},
};

struct Request {
    generation: u64,
    path: PathBuf,
}
#[derive(Default)]
struct State {
    generation: u64,
    queue: VecDeque<PathBuf>,
    saves: BTreeMap<PathBuf, Rating>,
    stop: bool,
}
pub(super) enum Reply {
    Loaded {
        generation: u64,
        path: PathBuf,
        entry: Entry,
        error: Option<String>,
    },
    Saved {
        ratings: BTreeMap<PathBuf, Rating>,
        error: Option<String>,
    },
}
pub(super) struct Loader {
    state: Arc<(Mutex<State>, Condvar)>,
    replies: mpsc::Receiver<Reply>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Loader {
    pub fn new(ctx: egui::Context, cache: Option<Arc<Cache>>) -> Self {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let slot = state.clone();
        let (sender, replies) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let mut folders = HashMap::new();
            let mut generation = 0;
            loop {
                enum Job {
                    Load(Request),
                    Save(PathBuf, BTreeMap<PathBuf, Rating>),
                }
                let job = {
                    let mut state = slot.0.lock().unwrap();
                    while state.queue.is_empty() && state.saves.is_empty() && !state.stop {
                        state = slot.1.wait(state).unwrap();
                    }
                    if let Some((path, rating)) = state.saves.pop_first() {
                        let folder = path.parent().unwrap_or(Path::new(".")).to_owned();
                        let neighbors: Vec<_> = state
                            .saves
                            .keys()
                            .filter(|path| path.parent() == Some(folder.as_path()))
                            .cloned()
                            .collect();
                        let mut ratings = BTreeMap::from([(path, rating)]);
                        for path in neighbors {
                            ratings.insert(path.clone(), state.saves.remove(&path).unwrap());
                        }
                        Job::Save(folder, ratings)
                    } else if state.stop {
                        break;
                    } else {
                        Job::Load(Request {
                            generation: state.generation,
                            path: state.queue.pop_front().unwrap(),
                        })
                    }
                };
                let reply = match job {
                    Job::Save(folder, ratings) => {
                        let changes = ratings
                            .iter()
                            .map(|(path, rating)| {
                                (PathBuf::from(path.file_name().unwrap_or_default()), *rating)
                            })
                            .collect();
                        let error = review::save_many(&folder, &changes)
                            .err()
                            .map(|e| e.to_string());
                        if let Some(error) = &error {
                            eprintln!("Cannot save ratings for {}: {error}", folder.display());
                        }
                        folders.remove(&folder);
                        Reply::Saved { ratings, error }
                    }
                    Job::Load(request) => {
                        if generation != request.generation {
                            generation = request.generation;
                            folders.clear();
                        }
                        let mut read = || {
                            if slot.0.lock().unwrap().generation != request.generation {
                                return None;
                            }
                            let folder = request.path.parent().unwrap_or(Path::new(""));
                            let ratings = folders
                                .entry(folder.to_owned())
                                .or_insert_with(|| review::load(folder).map_err(|e| e.to_string()));
                            let error = ratings.as_ref().err().cloned();
                            let rating = ratings
                                .as_ref()
                                .ok()
                                .and_then(|ratings| {
                                    ratings.get(Path::new(
                                        request.path.file_name().unwrap_or_default(),
                                    ))
                                })
                                .copied()
                                .unwrap_or_default();
                            let properties = cache
                                .as_ref()
                                .map_or_else(
                                    || foveonic::source::read_capture_properties(&request.path),
                                    |cache| cache.capture_properties(&request.path),
                                )
                                .unwrap_or_default();
                            let bytes = cache
                                .as_ref()
                                .and_then(|cache| cache.source_bytes(&request.path))
                                .or_else(|| {
                                    std::fs::metadata(&request.path).ok().map(|info| info.len())
                                });
                            Some(Reply::Loaded {
                                generation: request.generation,
                                path: request.path.clone(),
                                entry: Entry::new(properties, bytes, rating),
                                error,
                            })
                        };
                        let reply = if let Some(cache) = &cache {
                            cache.background(read)
                        } else {
                            read()
                        };
                        let Some(reply) = reply else {
                            continue;
                        };
                        reply
                    }
                };
                if sender.send(reply).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            state,
            replies,
            thread: Some(thread),
        }
    }
    pub fn replace(&self, files: &[PathBuf]) -> u64 {
        let mut state = self.state.0.lock().unwrap();
        state.generation += 1;
        state.queue = files.iter().cloned().collect();
        self.state.1.notify_one();
        state.generation
    }
    pub fn visible(&self, visible: &[PathBuf]) {
        let mut state = self.state.0.lock().unwrap();
        let mut promoted = Vec::new();
        state.queue.retain(|path| {
            if visible.contains(path) {
                promoted.push(path.clone());
                false
            } else {
                true
            }
        });
        for path in promoted.into_iter().rev() {
            state.queue.push_front(path);
        }
    }
    pub fn save(&self, paths: &[PathBuf], rating: Rating) {
        let mut state = self.state.0.lock().unwrap();
        state
            .saves
            .extend(paths.iter().cloned().map(|path| (path, rating)));
        drop(state);
        self.state.1.notify_one();
    }
    pub fn receive(&self) -> impl Iterator<Item = Reply> + '_ {
        self.replies.try_iter()
    }
}
impl Drop for Loader {
    fn drop(&mut self) {
        let mut state = self.state.0.lock().unwrap();
        state.stop = true;
        state.queue.clear();
        state.generation += 1;
        self.state.1.notify_one();
        drop(state);
        // Flush requested rating writes just as the editing writer flushes edits.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl super::Browser {
    pub fn metadata_ready(&self) -> bool {
        self.metadata_loaded.len() == self.files.len()
    }
    pub(super) fn receive_metadata(&mut self, ctx: &egui::Context) {
        const REPLIES_PER_FRAME: usize = 16;
        let replies: Vec<_> = self
            .metadata_loader
            .receive()
            .take(REPLIES_PER_FRAME)
            .collect();
        let received = !replies.is_empty();
        for reply in replies {
            match reply {
                Reply::Loaded {
                    generation,
                    path,
                    mut entry,
                    error,
                } if generation == self.metadata_generation && self.files.contains(&path) => {
                    if let Some(old) = self.records.get(&path) {
                        entry.extent = old.extent;
                    }
                    if let Some(rating) = self.rating_changes.get(&path) {
                        entry.rating = *rating;
                    }
                    if let Some(error) = error {
                        self.rating_errors.insert(path.clone(), error);
                    }
                    self.metadata_loaded.insert(path.clone());
                    self.records.insert(path, entry);
                    self.query_dirty = true;
                }
                Reply::Saved { ratings, error } => {
                    for (path, rating) in ratings {
                        if self
                            .rating_changes
                            .get(&path)
                            .is_some_and(|current| *current != rating)
                        {
                            continue;
                        }
                        if let Some(error) = &error {
                            self.rating_errors.insert(path, error.clone());
                        } else {
                            self.rating_errors.remove(&path);
                        }
                    }
                }
                _ => (),
            }
        }
        if received {
            ctx.request_repaint();
        }
    }
    pub(super) fn apply_query(&mut self) {
        if self.query_dirty || self.sort != self.applied_sort {
            self.query_dirty = false;
            // Metadata arrivals cannot change a filename sort. Avoid rebuilding
            // the whole decode neighborhood as each thumbnail acquires metadata.
            if self.sort != self.applied_sort || self.sort.field != super::query::Field::Name {
                super::query::sort(&mut self.files, &self.records, self.sort);
                self.invalidate_prefetch();
            }
            self.applied_sort = self.sort;
            self.refresh_filter();
        }
    }
    pub fn rating(&self, path: &Path) -> Rating {
        self.rating_changes
            .get(path)
            .copied()
            .or_else(|| self.records.get(path).map(|e| e.rating))
            .unwrap_or_default()
    }
    pub fn changed_rating(&self, path: &Path) -> Option<Rating> {
        self.rating_changes.get(path).copied()
    }
    pub(super) fn rate(&mut self, targets: &[PathBuf], rating: Rating) {
        for path in targets {
            self.records.entry(path.clone()).or_default().rating = rating;
            self.rating_changes.insert(path.clone(), rating);
        }
        if !self.import_mode && std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").is_none() {
            self.metadata_loader.save(targets, rating);
        }
        self.query_dirty = true;
    }
}

#[cfg(test)]
#[path = "browser_metadata_test.rs"]
mod tests;
