//! Prioritizes visible thumbnails over a greedy folder pass on a separate worker from larger previews.

use super::PreparedPreview;
use eframe::egui;
use foveonic::decode_cache::{Activity, Cache};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Condvar, Mutex},
};

// Bound decoded replies independently of the inexpensive queued file paths.
pub(super) const REPLY_CAPACITY: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Thumbnail,
    Preview,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Request {
    pub path: PathBuf,
    pub version: u64,
}
pub(super) struct Reply {
    pub path: PathBuf,
    pub version: u64,
    pub kind: Kind,
    pub image: Result<PreparedPreview, String>,
}
#[derive(Default)]
struct Queue {
    requests: VecDeque<Request>,
    wanted: HashMap<PathBuf, u64>,
    visible: Vec<PathBuf>,
    active: Option<Request>,
    stop: bool,
    demand: Option<Activity>,
}
impl Queue {
    fn current(&self, request: &Request) -> bool {
        !self.stop && self.wanted.get(&request.path) == Some(&request.version)
    }
    fn promote(&mut self) {
        let ranks: HashMap<_, _> = self
            .visible
            .iter()
            .enumerate()
            .map(|(i, p)| (p, i))
            .collect();
        let mut visible = Vec::new();
        self.requests.retain(|r| {
            if let Some(rank) = ranks.get(&r.path) {
                visible.push((*rank, r.clone()));
                false
            } else {
                true
            }
        });
        visible.sort_by_key(|(rank, _)| *rank);
        for (_, request) in visible.into_iter().rev() {
            self.requests.push_front(request);
        }
    }
    fn reserve(&mut self, kind: Kind, cache: Option<&Cache>) {
        if kind == Kind::Thumbnail && self.demand.is_none() && !self.requests.is_empty() {
            self.demand = cache.map(Cache::thumbnail_demand);
        }
        if self.requests.is_empty() && self.active.is_none() {
            self.demand = None;
        }
    }
}

pub(super) struct Worker {
    state: Arc<(Mutex<Queue>, Condvar)>,
    cache: Option<Arc<Cache>>,
    kind: Kind,
}
impl Worker {
    pub fn start(
        kind: Kind,
        cache: Option<Arc<Cache>>,
        context: egui::Context,
        replies: mpsc::SyncSender<Reply>,
    ) -> Self {
        let source = cache.clone();
        Self::start_with(kind, cache, context, replies, move |path| {
            load(kind, source.as_deref(), path)
        })
    }
    fn start_with(
        kind: Kind,
        cache: Option<Arc<Cache>>,
        context: egui::Context,
        replies: mpsc::SyncSender<Reply>,
        read: impl Fn(&Path) -> foveonic::source::Result<PreparedPreview> + Send + 'static,
    ) -> Self {
        let state = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let pending = state.clone();
        let source = cache.clone();
        std::thread::spawn(move || loop {
            let request = {
                let mut state = pending.0.lock().unwrap();
                while state.requests.is_empty() && !state.stop {
                    state = pending.1.wait(state).unwrap();
                }
                if state.stop {
                    break;
                }
                let request = state.requests.pop_front().unwrap();
                state.active = Some(request.clone());
                request
            };
            let load = || {
                if !pending.0.lock().unwrap().current(&request) {
                    return Err("Preview request superseded".into());
                }
                read(&request.path).map_err(|e| e.to_string())
            };
            let image = match source.as_ref() {
                Some(cache) if kind == Kind::Thumbnail => cache.thumbnail(load),
                Some(cache) => cache.background(load),
                None => load(),
            };
            let current = {
                let mut state = pending.0.lock().unwrap();
                let current = state.current(&request);
                if current {
                    state.wanted.remove(&request.path);
                }
                state.active = None;
                state.reserve(kind, source.as_deref());
                current
            };
            if current {
                // Wake before a bounded send so a full reply queue can always drain.
                context.request_repaint();
                if replies
                    .send(Reply {
                        path: request.path,
                        version: request.version,
                        kind,
                        image,
                    })
                    .is_err()
                {
                    break;
                }
                context.request_repaint();
            }
        });
        Self { state, cache, kind }
    }
    pub fn replace(&self, requests: Vec<Request>, visible: &[PathBuf]) {
        let mut state = self.state.0.lock().unwrap();
        state.wanted = requests
            .iter()
            .map(|r| (r.path.clone(), r.version))
            .collect();
        state.requests = requests
            .into_iter()
            .filter(|r| state.active.as_ref() != Some(r))
            .collect();
        state.visible = visible.to_vec();
        state.promote();
        state.reserve(self.kind, self.cache.as_deref());
        self.state.1.notify_one();
    }
    pub fn request(&self, request: Request) {
        let mut state = self.state.0.lock().unwrap();
        if state.current(&request) {
            return;
        }
        state.requests.retain(|r| r.path != request.path);
        state.wanted.insert(request.path.clone(), request.version);
        if state.visible.contains(&request.path) {
            state.requests.push_front(request);
        } else {
            state.requests.push_back(request);
        }
        state.reserve(self.kind, self.cache.as_deref());
        self.state.1.notify_one();
    }
    pub fn visible(&self, paths: &[PathBuf]) {
        let mut state = self.state.0.lock().unwrap();
        if state.visible != paths {
            state.visible = paths.to_vec();
            state.promote();
        }
    }
    pub fn cancel(&self, path: &Path) {
        let mut state = self.state.0.lock().unwrap();
        state.requests.retain(|r| r.path != path);
        state.wanted.remove(path);
        state.reserve(self.kind, self.cache.as_deref());
    }
    pub fn retain(&self, paths: &[PathBuf]) {
        let mut state = self.state.0.lock().unwrap();
        state.requests.retain(|r| paths.contains(&r.path));
        state.wanted.retain(|path, _| paths.contains(path));
        state.reserve(self.kind, self.cache.as_deref());
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let mut state = self.state.0.lock().unwrap();
        state.stop = true;
        state.requests.clear();
        state.wanted.clear();
        state.reserve(self.kind, self.cache.as_deref());
        self.state.1.notify_one();
    }
}

fn load(
    kind: Kind,
    cache: Option<&Cache>,
    path: &Path,
) -> foveonic::source::Result<PreparedPreview> {
    let preview = match kind {
        Kind::Thumbnail => {
            match cache
                .and_then(|c| c.selection(path))
                .and_then(|s| s.thumbnail)
            {
                Some(preview) => preview,
                None => foveonic::preview::read(path)?,
            }
        }
        Kind::Preview => cache
            .and_then(|c| c.preview(path))
            .ok_or("No cached preview")?,
    };
    PreparedPreview::new(preview, kind == Kind::Preview)
}

#[cfg(test)]
#[path = "browser_loader_test.rs"]
mod tests;
