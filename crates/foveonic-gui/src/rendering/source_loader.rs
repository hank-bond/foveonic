//! Acquires decoded sources off the renderer and hands bounded results through coordination.

use super::{coordinator::TaskPriority, worker::Pending};
use crate::progress::{Reporter, Tracker};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
#[cfg(test)]
use foveonic::render::ProcessingQuality;
use foveonic::{decode_cache::Cache, diagnostics as trace, source::Source};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    time::Duration,
};

type Slot = Arc<(Mutex<Pending>, Condvar)>;

#[derive(Clone)]
pub(super) struct Request {
    path: PathBuf,
    revision: u64,
    context: Option<trace::Context>,
    priority: TaskPriority,
    cancelled: Arc<AtomicBool>,
}

pub(super) struct Loaded {
    pub path: PathBuf,
    pub result: foveonic::source::Result<Arc<Source>>,
}

/// A yielded acquisition remains wanted and never becomes a photographic error.
pub(super) enum Completion {
    Loaded(Loaded),
    Retry(Request),
}

struct Hold {
    path: PathBuf,
    reached: Sender<()>,
    release: Receiver<()>,
}

/// Dropping the diagnostic handle releases the source thread.
pub(crate) struct LoadHold {
    pub reached: Receiver<()>,
    _release: Sender<()>,
}

pub(super) struct Control(Sender<Hold>);
impl Control {
    pub fn hold(&self, path: PathBuf) -> LoadHold {
        let (reached, arrival) = bounded(1);
        let (release, resume) = bounded(1);
        let _ = self.0.try_send(Hold {
            path,
            reached,
            release: resume,
        });
        LoadHold {
            reached: arrival,
            _release: release,
        }
    }
}

pub(super) enum Signal {
    Needed(Request),
    Taken,
}

/// Renderer-side access never opens a file. None means its request is loading.
pub(super) struct Handle {
    signals: Sender<Signal>,
    slot: Slot,
}

/// Coordination owns the wanted loads and admits one extra source at a time.
pub(super) struct Service {
    pub signals: Receiver<Signal>,
    pub results: Receiver<Completion>,
    jobs: Sender<Request>,
    queue: VecDeque<Request>,
    active: Option<PathBuf>,
    wanted: Vec<PathBuf>,
    priorities: Vec<(PathBuf, TaskPriority)>,
    active_request: Option<Request>,
    slot: Slot,
}

pub(super) fn start(
    cache: Arc<Cache>,
    progress: Tracker,
    slot: Slot,
) -> (Handle, Service, Control) {
    let (signals, requests) = unbounded();
    let (jobs, work) = bounded::<Request>(1);
    let (completed, results) = bounded(1);
    let (hold, holds) = bounded::<Hold>(1);
    std::thread::Builder::new()
        .name("source-loader".into())
        .spawn(move || {
            let mut hold = None;
            while let Ok(request) = work.recv() {
                let _context = request.context.map(trace::enter);
                let _activity = match request.priority {
                    TaskPriority::Viewport => Some(cache.foreground()),
                    TaskPriority::Auxiliary => Some(cache.auxiliary_demand()),
                    TaskPriority::Thumbnail => Some(cache.thumbnail_demand()),
                    TaskPriority::Idle => None,
                };
                let _scope = trace::scope(trace::Category::Cpu, "source.load");
                let _span = crate::timing::Span::new(request.revision, "source_open");
                let reporter = Reporter::new(progress.clone(), request.revision);
                if hold.is_none() {
                    hold = holds.try_recv().ok();
                }
                // A denied gate parks only this source thread. Plan changes cancel
                // the request; timed rechecks also notice gate-only demand release.
                let completion = loop {
                    if request.cancelled.load(Ordering::Acquire) {
                        break Completion::Retry(request);
                    }
                    let mut acquire = |cancel: &dyn Fn() -> bool| {
                        if hold.as_ref().is_some_and(|hold| hold.path == request.path) {
                            let hold = hold.take().unwrap();
                            let _pause =
                                trace::scope(trace::Category::Scenario, "scenario.source_pause");
                            let _ = hold.reached.try_send(());
                            let _ = hold.release.recv();
                        }

                        if cancel() || request.cancelled.load(Ordering::Acquire) {
                            return None;
                        }
                        let operation_id = crate::support::operation_id("source");
                        let _support_context = tracing::info_span!("source", source = %request.path.display(), operation_id = %operation_id).entered();
                        foveonic::support::event(
                            foveonic::support::Level::Info,
                            "source.open",
                            "Opening photograph",
                            serde_json::json!({"operation_id":operation_id,"source":crate::support::path(&request.path),"revision":request.revision}),
                        );
                        let result = cache
                            .open(&request.path, |stage| {
                                if request.priority == TaskPriority::Viewport {
                                    reporter.loaded(stage);
                                }
                            })
                            .map_err(|error| {
                                foveonic::support::reported_error(
                                    "source.open",
                                    error,
                                    foveonic::support::OperationContext {
                                        operation_id: Some(operation_id.clone()),
                                        component: Some("source loader".into()),
                                        phase: Some("open and decode".into()),
                                        source: Some(request.path.to_string_lossy().into_owned()),
                                        revision: Some(request.revision.to_string()),
                                        ..Default::default()
                                    },
                                )
                            });
                        if result.is_ok() {
                            foveonic::support::event(
                                foveonic::support::Level::Info,
                                "source.ready",
                                "Photograph opened",
                                serde_json::json!({"operation_id":operation_id,"source":crate::support::path(&request.path),"revision":request.revision}),
                            );
                        }
                        (!cancel() && !request.cancelled.load(Ordering::Acquire)).then_some(result)
                    };
                    let attempt = match request.priority {
                        TaskPriority::Viewport => Some(acquire(&|| false)),
                        TaskPriority::Auxiliary => cache.auxiliary_execution(acquire),
                        TaskPriority::Thumbnail => cache.thumbnail_execution(acquire),
                        TaskPriority::Idle => cache.idle_execution(acquire),
                    };
                    match attempt {
                        Some(Some(result)) => {
                            break Completion::Loaded(Loaded {
                                path: request.path,
                                result,
                            })
                        }
                        Some(None) => break Completion::Retry(request),
                        None => std::thread::sleep(Duration::from_millis(10)),
                    }
                };
                if completed.send(completion).is_err() {
                    break;
                }
            }
        })
        .expect("start source-loader thread");
    (
        Handle {
            signals,
            slot: slot.clone(),
        },
        Service {
            signals: requests,
            results,
            jobs,
            queue: Default::default(),
            active: None,
            wanted: Vec::new(),
            priorities: Vec::new(),
            active_request: None,
            slot,
        },
        Control(hold),
    )
}

impl Handle {
    /// Retry reuse once acquisition is consumed, including by another consumer.
    pub fn can_retry(&self, path: &Path, pending: &Pending) -> bool {
        pending
            .source
            .as_ref()
            .is_some_and(|source| source.path == path)
            || !pending.source_requests.contains(path)
    }

    pub fn open(
        &mut self,
        path: &Path,
        revision: u64,
    ) -> foveonic::source::Result<Option<Arc<Source>>> {
        self.open_at(path, revision, TaskPriority::Viewport)
    }

    pub fn open_at(
        &mut self,
        path: &Path,
        revision: u64,
        priority: TaskPriority,
    ) -> foveonic::source::Result<Option<Arc<Source>>> {
        let (loaded, request) = {
            let mut pending = self.slot.0.lock().unwrap();
            if pending
                .source
                .as_ref()
                .is_some_and(|source| source.path == path)
            {
                pending.source_requests.remove(path);
                (pending.source.take(), false)
            } else {
                (None, pending.source_requests.insert(path.to_owned()))
            }
        };
        if let Some(loaded) = loaded {
            let _ = self.signals.send(Signal::Taken);
            return loaded.result.map(Some);
        }
        if request {
            let _ = self.signals.send(Signal::Needed(Request {
                path: path.to_owned(),
                revision,
                context: trace::current_context(),
                priority,
                cancelled: Arc::new(AtomicBool::new(false)),
            }));
        }
        Ok(None)
    }
}

impl Service {
    pub fn retain(&mut self, wanted: Vec<PathBuf>) {
        self.queue.retain(|request| wanted.contains(&request.path));
        let mut pending = self.slot.0.lock().unwrap();
        pending.source_requests.retain(|path| wanted.contains(path));
        if pending
            .source
            .as_ref()
            .is_some_and(|source| !wanted.contains(&source.path))
        {
            pending.source = None;
        }
        if let Some(active) = &self.active_request {
            if !wanted.contains(&active.path) {
                active.cancelled.store(true, Ordering::Release);
            }
        }
        self.wanted = wanted;
    }

    pub fn signal(&mut self, signal: Signal) {
        if let Signal::Needed(request) = signal {
            if self.wanted.contains(&request.path)
                && self.active.as_ref() != Some(&request.path)
                && !self.queue.iter().any(|queued| queued.path == request.path)
            {
                self.queue.push_back(request);
            } else if !self.wanted.contains(&request.path) {
                self.slot
                    .0
                    .lock()
                    .unwrap()
                    .source_requests
                    .remove(&request.path);
            }
        }
        self.admit();
    }

    pub fn completed(&mut self, completion: Completion) {
        self.active = None;
        self.active_request = None;
        match completion {
            Completion::Retry(mut request) if self.wanted.contains(&request.path) => {
                request.cancelled = Arc::new(AtomicBool::new(false));
                self.queue.push_back(request);
            }
            Completion::Loaded(loaded) if self.wanted.contains(&loaded.path) => {
                self.slot.0.lock().unwrap().source = Some(loaded);
                self.slot.1.notify_one();
            }
            Completion::Retry(request) => {
                self.slot
                    .0
                    .lock()
                    .unwrap()
                    .source_requests
                    .remove(&request.path);
            }
            Completion::Loaded(loaded) => {
                self.slot
                    .0
                    .lock()
                    .unwrap()
                    .source_requests
                    .remove(&loaded.path);
            }
        }
        self.admit();
    }

    fn priority(&self, path: &Path, fallback: TaskPriority) -> TaskPriority {
        self.priorities
            .iter()
            .filter(|(candidate, _)| candidate == path)
            .map(|(_, priority)| *priority)
            .min()
            .unwrap_or(fallback)
    }

    /// Reprioritize shared acquisitions, preserving the strongest same-path demand.
    pub fn prioritize(&mut self, priorities: &[(PathBuf, TaskPriority)]) {
        self.priorities = priorities.to_vec();
        if let Some(active) = &self.active_request {
            if self.priority(&active.path, active.priority) != active.priority
                || priorities
                    .iter()
                    .any(|(_, priority)| *priority < active.priority)
            {
                active.cancelled.store(true, Ordering::Release);
            }
        }
        let mut pending = self.slot.0.lock().unwrap();
        if pending.source.as_ref().is_some_and(|source| {
            let held = self.priority(&source.path, TaskPriority::Idle);
            priorities
                .iter()
                .any(|(path, priority)| *priority < held && pending.source_requests.contains(path))
        }) {
            let loaded = pending.source.take().unwrap();
            pending.source_requests.remove(&loaded.path);
            self.slot.1.notify_one();
        }
        drop(pending);
        self.admit();
    }

    pub fn admit(&mut self) {
        if self.active.is_some() || self.slot.0.lock().unwrap().source.is_some() {
            return;
        }
        let highest = self.priorities.iter().map(|(_, priority)| *priority).min();
        let index = self
            .queue
            .iter()
            .enumerate()
            .filter_map(|(index, request)| {
                let priority = self.priority(&request.path, request.priority);
                highest
                    .is_none_or(|highest| priority <= highest)
                    .then_some((index, priority))
            })
            .min_by_key(|(_, priority)| *priority)
            .map(|(index, _)| index);
        if let Some(mut request) = index.and_then(|index| self.queue.remove(index)) {
            request.priority = self.priority(&request.path, request.priority);
            self.active = Some(request.path.clone());
            self.active_request = Some(request.clone());
            let _ = self.jobs.try_send(request);
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if let Some(active) = &self.active_request {
            active.cancelled.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn rig() -> (Handle, Service, Control) {
        let cache = Arc::new(Cache::new());
        cache.configure(foveonic::decode_cache::Settings {
            enabled: false,
            ..Default::default()
        });
        rig_with_cache(cache)
    }

    fn rig_with_cache(cache: Arc<Cache>) -> (Handle, Service, Control) {
        start(
            cache,
            Tracker::default(),
            Arc::new((Mutex::new(Pending::default()), Condvar::new())),
        )
    }

    fn signal_next(service: &mut Service) {
        let signal = service
            .signals
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        service.signal(signal);
    }

    fn complete_next(service: &mut Service) {
        let completion = service
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        service.completed(completion);
    }

    #[test]
    fn denied_lower_tier_resumes_after_gate_release_without_a_ui_event() {
        let cache = Arc::new(Cache::new());
        let foreground = cache.foreground();
        let (mut handle, mut service, _control) = rig_with_cache(cache);
        let path = PathBuf::from("blocked-preview-missing.x3f");
        service.retain(vec![path.clone()]);
        service.prioritize(&[(path.clone(), TaskPriority::Thumbnail)]);
        handle.open_at(&path, 1, TaskPriority::Thumbnail).unwrap();
        signal_next(&mut service);
        assert!(service
            .results
            .recv_timeout(Duration::from_millis(30))
            .is_err());
        assert!(!handle.can_retry(&path, &service.slot.0.lock().unwrap()));
        drop(foreground);
        complete_next(&mut service);
        assert!(handle.open_at(&path, 1, TaskPriority::Thumbnail).is_err());
    }

    #[test]
    fn running_preview_yields_to_viewport_and_retries_without_publishing_cancellation() {
        let cache = Arc::new(Cache::new());
        let (mut handle, mut service, control) = rig_with_cache(cache.clone());
        let preview = PathBuf::from("interrupted-preview-missing.x3f");
        let viewport = PathBuf::from("priority-viewport-missing.x3f");
        service.retain(vec![preview.clone(), viewport.clone()]);
        service.prioritize(&[(preview.clone(), TaskPriority::Thumbnail)]);
        let hold = control.hold(preview.clone());
        handle
            .open_at(&preview, 1, TaskPriority::Thumbnail)
            .unwrap();
        signal_next(&mut service);
        hold.reached.recv_timeout(Duration::from_secs(5)).unwrap();
        let foreground = cache.foreground();
        service.prioritize(&[
            (viewport.clone(), TaskPriority::Viewport),
            (preview.clone(), TaskPriority::Thumbnail),
        ]);
        handle.open(&viewport, 2).unwrap();
        signal_next(&mut service);
        drop(hold);
        let yielded = service
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(matches!(&yielded, Completion::Retry(request) if request.path == preview));
        service.completed(yielded);
        assert!(service.slot.0.lock().unwrap().source.is_none());
        assert!(!handle.can_retry(&preview, &service.slot.0.lock().unwrap()));
        assert_eq!(service.active.as_ref(), Some(&viewport));
        complete_next(&mut service);
        assert!(handle.open(&viewport, 2).is_err());
        signal_next(&mut service);
        assert!(
            service.active.is_none(),
            "queued preview waits for viewport settlement"
        );
        drop(foreground);
        service.prioritize(&[(preview.clone(), TaskPriority::Thumbnail)]);
        complete_next(&mut service);
        assert!(handle
            .open_at(&preview, 1, TaskPriority::Thumbnail)
            .is_err());
    }

    #[test]
    fn same_path_promotion_restarts_one_shared_acquisition_at_viewport_priority() {
        let (mut handle, mut service, control) = rig();
        let path = PathBuf::from("promoted-preview-missing.x3f");
        service.retain(vec![path.clone()]);
        service.prioritize(&[(path.clone(), TaskPriority::Thumbnail)]);
        let hold = control.hold(path.clone());
        handle.open_at(&path, 1, TaskPriority::Thumbnail).unwrap();
        signal_next(&mut service);
        hold.reached.recv_timeout(Duration::from_secs(5)).unwrap();
        service.prioritize(&[
            (path.clone(), TaskPriority::Thumbnail),
            (path.clone(), TaskPriority::Viewport),
        ]);
        assert!(handle.open(&path, 2).unwrap().is_none());
        assert!(
            service.signals.is_empty(),
            "promotion must reuse the shared request"
        );
        drop(hold);
        let yielded = service
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(matches!(&yielded, Completion::Retry(_)));
        service.completed(yielded);
        assert_eq!(
            service.active_request.as_ref().unwrap().priority,
            TaskPriority::Viewport
        );
        assert!(service.queue.is_empty());
        complete_next(&mut service);
        assert!(handle.open(&path, 2).is_err());
    }

    #[test]
    fn queued_requests_follow_replacement_priorities() {
        let (mut handle, mut service, control) = rig();
        let active = PathBuf::from("queued-active-missing.x3f");
        let first = PathBuf::from("queued-first-missing.x3f");
        let second = PathBuf::from("queued-second-missing.x3f");
        service.retain(vec![active.clone(), first.clone(), second.clone()]);
        let hold = control.hold(active.clone());
        handle.open(&active, 1).unwrap();
        signal_next(&mut service);
        hold.reached.recv_timeout(Duration::from_secs(5)).unwrap();
        for (path, priority) in [
            (&first, TaskPriority::Thumbnail),
            (&second, TaskPriority::Auxiliary),
        ] {
            handle.open_at(path, 2, priority).unwrap();
            signal_next(&mut service);
        }
        service.prioritize(&[
            (active.clone(), TaskPriority::Viewport),
            (first.clone(), TaskPriority::Auxiliary),
            (second.clone(), TaskPriority::Thumbnail),
        ]);
        drop(hold);
        complete_next(&mut service);
        assert!(handle.open(&active, 1).is_err());
        service.prioritize(&[
            (first.clone(), TaskPriority::Auxiliary),
            (second, TaskPriority::Thumbnail),
        ]);
        assert_eq!(service.active.as_ref(), Some(&first));
        assert_eq!(
            service.active_request.as_ref().unwrap().priority,
            TaskPriority::Auxiliary
        );
        complete_next(&mut service);
    }

    #[test]
    fn a_shared_load_failure_settles_both_current_and_camera_requests() {
        use crate::{
            editor::Rendition,
            rendering::{
                coordinator::{Cursor, RenderCoordinator},
                request::Request as RenderRequest,
            },
        };
        use std::time::Instant;

        fn next(
            handle: &Handle,
            service: &Service,
            cursor: &mut Cursor,
            coordinator: &RenderCoordinator,
        ) -> Option<Arc<super::super::coordinator::Task>> {
            let pending = service.slot.0.lock().unwrap();
            cursor.next_ready(&coordinator.plan(), |task| {
                handle.can_retry(&task.request.path, &pending)
            })
        }

        let (mut handle, mut service, control) = rig();
        let path = PathBuf::from("shared-missing.x3f");
        let mut coordinator = RenderCoordinator::default();
        let mut cursor = Cursor::default();
        for (revision, rendition) in [(1, Rendition::Current), (2, Rendition::Camera)] {
            coordinator.desire(
                Arc::new(RenderRequest {
                    auxiliary_preview: false,

                    detail: false,
                    path: path.clone(),
                    revision,
                    rendition,
                    settings: foveonic::render::Settings::new("Sunlight", "Standard"),
                    framing_preview: false,
                    calibration: None,
                    preview_scale: foveonic::render::Scale::NATIVE,
                    processing_quality: ProcessingQuality::Quality,
                    output: "final".into(),
                    comparison: false,
                    export: None,
                }),
                None,
                Instant::now(),
            );
        }
        service.retain(vec![path.clone()]);
        let hold = control.hold(path.clone());
        for _ in 0..2 {
            let task = next(&handle, &service, &mut cursor, &coordinator).unwrap();
            assert!(handle.open(&path, task.stamp.revision).unwrap().is_none());
            cursor.defer(&task.stamp);
        }
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        hold.reached.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(next(&handle, &service, &mut cursor, &coordinator).is_none());
        coordinator.pick(
            super::super::worker::ColorPickRequest {
                tool: super::super::worker::ColorPickTool::Basic,
                token: 1,
                path: path.clone(),
                settings: Default::default(),
                uv: [0.5, 0.5],
            },
            2,
            None,
        );
        assert!(cursor.pick_ready(&coordinator.plan(), false).is_some());
        cursor.retry_pick();
        assert!(cursor
            .pick_ready(
                &coordinator.plan(),
                handle.can_retry(&path, &service.slot.0.lock().unwrap())
            )
            .is_none());
        drop(hold);
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        let first = next(&handle, &service, &mut cursor, &coordinator).unwrap();
        assert!(handle.open(&path, first.stamp.revision).is_err());
        coordinator.finish(&first.stamp);
        let pick = cursor
            .pick_ready(
                &coordinator.plan(),
                handle.can_retry(&path, &service.slot.0.lock().unwrap()),
            )
            .expect("a pick must also retry after a sibling consumes the handoff");
        coordinator.finish_pick(pick.id);
        let sibling = next(&handle, &service, &mut cursor, &coordinator)
            .expect("the other rendition must retry after its shared handoff was consumed");
        assert!(handle
            .open(&path, sibling.stamp.revision)
            .unwrap()
            .is_none());
        cursor.defer(&sibling.stamp);
        assert!(
            next(&handle, &service, &mut cursor, &coordinator).is_none(),
            "a fresh acquisition must restore waiting"
        );
        for signal in service.signals.try_iter().collect::<Vec<_>>() {
            service.signal(signal);
        }
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        let sibling = next(&handle, &service, &mut cursor, &coordinator).unwrap();
        assert!(handle.open(&path, sibling.stamp.revision).is_err());
        coordinator.finish(&sibling.stamp);
        assert!(!coordinator.busy());
    }

    #[test]
    fn held_loading_coalesces_edits_and_delivers_its_failure_once() {
        let (mut handle, mut service, control) = rig();
        let a = PathBuf::from("missing-a.x3f");
        let b = PathBuf::from("missing-b.x3f");
        let hold = control.hold(a.clone());
        service.retain(vec![a.clone(), b.clone()]);
        assert!(handle.open(&a, 1).unwrap().is_none());
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        hold.reached.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(handle.open(&a, 2).unwrap().is_none());
        assert!(
            service.signals.is_empty(),
            "an edit must reuse its pending source load"
        );
        assert!(!handle.can_retry(&a, &service.slot.0.lock().unwrap()));
        assert!(
            handle.can_retry(&b, &service.slot.0.lock().unwrap()),
            "B has no pending acquisition and may try session reuse"
        );
        drop(hold);
        let loaded = service
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(matches!(&loaded, Completion::Loaded(loaded) if loaded.result.is_err()));
        service.completed(loaded);
        assert!(handle.can_retry(&a, &service.slot.0.lock().unwrap()));
        assert!(handle.open(&a, 2).is_err());
        assert!(service.slot.0.lock().unwrap().source.is_none());
    }

    #[test]
    fn source_handoff_is_bounded_and_removed_loads_do_not_block_the_queue() {
        let (mut handle, mut service, control) = rig();
        let a = PathBuf::from("removed-a.x3f");
        let b = PathBuf::from("wanted-b.x3f");
        let hold = control.hold(a.clone());
        service.retain(vec![a.clone(), b.clone()]);
        assert!(handle.open(&a, 1).unwrap().is_none());
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        hold.reached.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(handle.open(&b, 1).unwrap().is_none());
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        assert_eq!(service.queue.len(), 1);
        service.retain(vec![b.clone()]);
        drop(hold);
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        assert!(
            service.slot.0.lock().unwrap().source.is_none(),
            "removed source must not occupy the handoff"
        );
        assert_eq!(service.active.as_ref(), Some(&b));
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        assert!(handle.open(&b, 1).is_err());
    }

    #[test]
    fn speculative_handoff_does_not_block_foreground_source() {
        let (mut handle, mut service, control) = rig();
        let a = PathBuf::from("speculative-a.x3f");
        let b = PathBuf::from("foreground-b.x3f");
        let hold = control.hold(a.clone());
        service.retain(vec![a.clone(), b.clone()]);
        assert!(handle.open(&a, 1).unwrap().is_none());
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        hold.reached.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(handle.open(&b, 2).unwrap().is_none());
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        drop(hold);
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        assert_eq!(
            service.slot.0.lock().unwrap().source.as_ref().unwrap().path,
            a
        );
        service.prioritize(&[
            (a.clone(), TaskPriority::Idle),
            (a.clone(), TaskPriority::Viewport),
            (b.clone(), TaskPriority::Viewport),
        ]);
        assert!(
            service.slot.0.lock().unwrap().source.is_some(),
            "same-path foreground can consume the handoff"
        );
        service.prioritize(&[
            (b.clone(), TaskPriority::Viewport),
            (a.clone(), TaskPriority::Idle),
        ]);
        assert!(service.slot.0.lock().unwrap().source.is_none());
        assert_eq!(service.active.as_ref(), Some(&b));
        assert!(handle.can_retry(&a, &service.slot.0.lock().unwrap()));
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        assert!(handle.open(&b, 2).is_err());
        assert!(
            handle.open(&a, 1).unwrap().is_none(),
            "continuation can reacquire its source"
        );
    }

    #[test]
    fn queued_speculative_source_does_not_overtake_foreground() {
        let (mut handle, mut service, control) = rig();
        let a = PathBuf::from("speculative-active.x3f");
        let b = PathBuf::from("speculative-queued.x3f");
        let c = PathBuf::from("foreground-queued.x3f");
        let hold = control.hold(a.clone());
        service.retain(vec![a.clone(), b.clone(), c.clone()]);
        assert!(handle.open(&a, 1).unwrap().is_none());
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        hold.reached.recv_timeout(Duration::from_secs(5)).unwrap();
        for (path, revision) in [(&b, 2), (&c, 3)] {
            assert!(handle.open(path, revision).unwrap().is_none());
            service.signal(
                service
                    .signals
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap(),
            );
        }
        service.prioritize(&[
            (c.clone(), TaskPriority::Viewport),
            (a.clone(), TaskPriority::Viewport),
            (b.clone(), TaskPriority::Idle),
        ]);
        drop(hold);
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        service.prioritize(&[
            (c.clone(), TaskPriority::Viewport),
            (a.clone(), TaskPriority::Idle),
            (b.clone(), TaskPriority::Idle),
        ]);
        assert_eq!(service.active.as_ref(), Some(&c));
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        assert!(handle.open(&c, 3).is_err());
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        assert!(
            service.active.is_none(),
            "queued speculative work waits for foreground settlement"
        );
        service.prioritize(&[(a, TaskPriority::Idle), (b.clone(), TaskPriority::Idle)]);
        assert_eq!(service.active.as_ref(), Some(&b));
    }

    #[test]
    fn removing_and_readding_a_photo_registers_a_new_load() {
        let (mut handle, mut service, control) = rig();
        let a = PathBuf::from("readded-a.x3f");
        let hold = control.hold(a.clone());
        service.retain(vec![a.clone()]);
        assert!(handle.open(&a, 1).unwrap().is_none());
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        hold.reached.recv_timeout(Duration::from_secs(5)).unwrap();
        service.retain(vec![]);
        drop(hold);
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        service.retain(vec![a.clone()]);
        assert!(handle.open(&a, 2).unwrap().is_none());
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        assert!(handle.open(&a, 2).is_err());
    }

    #[test]
    fn another_load_starts_only_after_the_ready_handoff_is_consumed() {
        let (mut handle, mut service, control) = rig();
        let a = PathBuf::from("first-a.x3f");
        let b = PathBuf::from("next-b.x3f");
        let hold = control.hold(a.clone());
        service.retain(vec![a.clone(), b.clone()]);
        handle.open(&a, 1).unwrap();
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        hold.reached.recv_timeout(Duration::from_secs(5)).unwrap();
        handle.open(&b, 2).unwrap();
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        drop(hold);
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        assert!(
            service.active.is_none(),
            "ready A occupies the one extra source slot"
        );
        assert_eq!(service.queue.len(), 1);
        assert!(handle.open(&a, 1).is_err());
        service.signal(
            service
                .signals
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        assert_eq!(service.active.as_ref(), Some(&b));
        service.completed(
            service
                .results
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
        );
        assert!(handle.open(&b, 2).is_err());
    }
}
