//! Prioritizes visible thumbnails over a greedy folder pass on a separate worker from larger previews.

use super::PreparedPreview;
use eframe::egui;
use foveonic::diagnostics as trace;
use foveonic::priority::Priority;
use foveonic::{decode_cache::Cache, priority::Demand};
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
    pub image: Result<Option<PreparedPreview>, String>,
    pub(super) memory: Option<crate::display_image::StagedBytes>,
}
#[derive(Default)]
struct Queue {
    requests: VecDeque<Request>,
    wanted: HashMap<PathBuf, u64>,
    visible: Vec<PathBuf>,
    active: Option<Request>,
    stop: bool,
    demand: Option<Demand>,
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
            self.demand = cache.map(|cache| cache.priority().demand(Priority::Thumbnail));
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
    staging: crate::display_image::Staging,
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
        read: impl Fn(&Path) -> foveonic::source::Result<Option<PreparedPreview>> + Send + 'static,
    ) -> Self {
        let state = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let pending = state.clone();
        let source = cache.clone();
        let staging = crate::display_image::Staging::default();
        let worker_staging = staging.clone();
        let builder = std::thread::Builder::new().name(
            (if kind == Kind::Thumbnail {
                "browser-thumbnail-loader"
            } else {
                "browser-preview-loader"
            })
            .into(),
        );
        let spawned = builder.spawn(move || {
            loop {
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
                let purpose = if kind == Kind::Thumbnail {
                    "browser_thumbnail"
                } else {
                    "browser_preview"
                };
                let _context = trace::enabled().then(|| {
                    trace::enter(crate::timing::context_for(
                        0,
                        request.version,
                        &request.path,
                        purpose,
                    ))
                });
                let _job = trace::scope_with(trace::Category::Cpu, "browser.image_job", |fields| {
                    fields
                        .str("source", &request.path.to_string_lossy())
                        .u64("browser_version", request.version)
                        .str("kind", purpose);
                });
                let load = || {
                    if !pending.0.lock().unwrap().current(&request) {
                        return Err("Preview request superseded".into());
                    }
                    let _read = trace::scope(trace::Category::Cpu, "browser.image_load");
                    let _support_context=tracing::info_span!("browser_preview",source=%request.path.display(),purpose=purpose).entered();
                    read(&request.path).map_err(|error| {
                        foveonic::support::report_error(
                            "browser.preview",
                            error.as_ref(),
                            foveonic::support::OperationContext {
                                component: Some("browser images".into()),
                                source: Some(request.path.to_string_lossy().into_owned()),
                                revision: Some(request.version.to_string()),
                                purpose: Some(purpose.into()),
                                ..Default::default()
                            },
                        );
                        error.to_string()
                    })
                };
                let image = match source.as_ref() {
                    Some(cache) if kind == Kind::Thumbnail => cache.priority().run(Priority::Thumbnail, load),
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
                trace::event(
                    trace::Category::Presentation,
                    "browser.image_result",
                    |fields| {
                        fields.str(
                            "outcome",
                            if !current {
                                "superseded"
                            } else {
                                match &image {
                                    Ok(Some(_)) => "ready",
                                    Ok(None) => "miss",
                                    Err(_) => "failed",
                                }
                            },
                        );
                    },
                );
                if current {
                    // Wake before a bounded send so a full reply queue can always drain.
                    context.request_repaint();
                    let memory = worker_staging.track(match &image {
                        Ok(Some(image)) => image.retained_bytes(),
                        _ => 0,
                    });
                    let _publish = trace::scope_with(
                        trace::Category::Coordinator,
                        "browser.image_reply_send",
                        |fields| {
                            fields
                                .u64("capacity", REPLY_CAPACITY as u64)
                                .str("measurement", "host_wait");
                        },
                    );
                    if replies
                        .send(Reply {
                            path: request.path,
                            version: request.version,
                            kind,
                            image,
                            memory,
                        })
                        .is_err()
                    {
                        break;
                    }
                    context.request_repaint();
                }
            }
        });
        spawned.expect("start browser loader thread");
        Self {
            state,
            cache,
            kind,
            staging,
        }
    }
    pub(super) fn memory_resources(&self, inventory: &mut crate::memory::Inventory) {
        inventory.cpu(self.staging.bytes());
    }
    pub(super) fn resource_revision(&self) -> u64 {
        self.staging.revision()
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
    pub fn stop(&self) {
        let mut state = self.state.0.lock().unwrap();
        state.stop = true;
        state.requests.clear();
        state.wanted.clear();
        state.reserve(self.kind, self.cache.as_deref());
        self.state.1.notify_one();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

fn load(
    kind: Kind,
    cache: Option<&Cache>,
    path: &Path,
) -> foveonic::source::Result<Option<PreparedPreview>> {
    let _scope = trace::scope_with(trace::Category::Cpu, "browser.prepare_preview", |fields| {
        fields.str("source", &path.to_string_lossy()).str(
            "kind",
            if kind == Kind::Thumbnail {
                "thumbnail"
            } else {
                "preview"
            },
        );
    });
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
        Kind::Preview => {
            let Some(preview) = cache.and_then(|c| c.preview(path)) else {
                return Ok(None);
            };
            preview
        }
    };
    PreparedPreview::new(preview, kind == Kind::Preview).map(Some)
}

#[cfg(test)]
#[path = "loader_test.rs"]
mod tests;
