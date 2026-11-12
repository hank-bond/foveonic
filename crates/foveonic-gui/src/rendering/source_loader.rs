//! Acquires decoded sources off the renderer and hands bounded results through coordination.

use super::worker::Pending;
use crate::progress::{Reporter, Tracker};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use foveonic::{decode_cache::Cache, diagnostics as trace, source::Source};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
};

type Slot = Arc<(Mutex<Pending>, Condvar)>;

pub(super) struct Request {
    path: PathBuf,
    revision: u64,
    context: Option<trace::Context>,
}

pub(super) struct Loaded {
    pub path: PathBuf,
    pub result: Result<Arc<Source>, String>,
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
    pub results: Receiver<Loaded>,
    jobs: Sender<Request>,
    queue: VecDeque<Request>,
    active: Option<PathBuf>,
    wanted: Vec<PathBuf>,
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
                let _activity = cache.foreground();
                let _scope = trace::scope(trace::Category::Cpu, "source.load");
                let _span = crate::timing::Span::new(request.revision, "source_open");
                let reporter = Reporter::new(progress.clone(), request.revision);
                if hold.is_none() {
                    hold = holds.try_recv().ok();
                }
                if hold.as_ref().is_some_and(|hold| hold.path == request.path) {
                    let hold = hold.take().unwrap();
                    let _pause = trace::scope(trace::Category::Scenario, "scenario.source_pause");
                    let _ = hold.reached.try_send(());
                    let _ = hold.release.recv();
                }
                // Cache I/O stays on this thread. Decoding and conversion retain
                // their existing shared host-sized processing pool.
                let result = cache
                    .open(&request.path, |stage| reporter.loaded(stage))
                    .map_err(|error| error.to_string());
                if completed
                    .send(Loaded {
                        path: request.path,
                        result,
                    })
                    .is_err()
                {
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
            slot,
        },
        Control(hold),
    )
}

impl Handle {
    pub fn ready(&self, path: &Path, source: Option<&Loaded>) -> bool {
        source.is_some_and(|source| source.path == path)
    }

    pub fn open(
        &mut self,
        path: &Path,
        revision: u64,
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
            return loaded.result.map(Some).map_err(Into::into);
        }
        if request {
            let _ = self.signals.send(Signal::Needed(Request {
                path: path.to_owned(),
                revision,
                context: trace::current_context(),
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

    pub fn completed(&mut self, loaded: Loaded) {
        self.active = None;
        if self.wanted.contains(&loaded.path) {
            self.slot.0.lock().unwrap().source = Some(loaded);
            self.slot.1.notify_one();
        } else {
            self.slot
                .0
                .lock()
                .unwrap()
                .source_requests
                .remove(&loaded.path);
        }
        self.admit();
    }

    pub fn admit(&mut self) {
        if self.active.is_some() || self.slot.0.lock().unwrap().source.is_some() {
            return;
        }
        if let Some(request) = self.queue.pop_front() {
            self.active = Some(request.path.clone());
            let _ = self.jobs.try_send(request);
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
        start(
            cache,
            Tracker::default(),
            Arc::new((Mutex::new(Pending::default()), Condvar::new())),
        )
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
        assert!(!handle.ready(&a, None));
        assert!(
            !handle.ready(&b, None),
            "B has no decoded handoff; its renderer session decides reuse"
        );
        drop(hold);
        let loaded = service
            .results
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(loaded.result.is_err());
        service.completed(loaded);
        assert!(handle.ready(&a, service.slot.0.lock().unwrap().source.as_ref()));
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
