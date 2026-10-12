//! Checks capture publication without a renderer, latest-only requests, and selection provenance.

use super::*;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

struct Harness {
    loader: Loader,
    started: mpsc::Receiver<(PathBuf, usize)>,
    release: mpsc::Sender<()>,
}
impl Harness {
    fn new() -> Self {
        let (started, receiver) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let count = Mutex::new(0);
        let loader = Loader::with_reader(egui::Context::default(), move |path| {
            let mut count = count.lock().unwrap();
            *count += 1;
            started.send((path.to_owned(), *count)).unwrap();
            gate.recv_timeout(Duration::from_secs(2)).unwrap();
            Ok([("ISO".into(), count.to_string())].into())
        });
        Self {
            loader,
            started: receiver,
            release,
        }
    }
    fn started(&self) -> (PathBuf, usize) {
        self.started.recv_timeout(Duration::from_secs(2)).unwrap()
    }
    fn publish(&mut self) {
        self.release.send(()).unwrap();
        wait_until(|| {
            self.loader.receive();
            self.loader.properties.is_some()
        });
    }
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !condition() {
        assert!(Instant::now() < deadline, "capture reader did not finish");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn publishes_capture_without_any_render_worker_and_keeps_only_latest_pending_image() {
    let mut h = Harness::new();
    h.loader.select(Some("a.x3f".into()));
    assert_eq!(h.started(), ("a.x3f".into(), 1));
    h.loader.select(Some("b.x3f".into()));
    h.loader.select(Some("c.x3f".into()));
    h.release.send(()).unwrap();
    assert_eq!(h.started(), ("c.x3f".into(), 2));
    h.loader.receive();
    assert!(h.loader.properties.is_none()); // A cannot publish while C is selected.
    h.publish();
    assert_eq!(h.loader.properties.as_ref().unwrap()["ISO"], "2");
    assert!(h.started.try_recv().is_err()); // B was replaced, not queued.
}

#[test]
fn clear_and_same_path_reselection_reject_the_old_read_and_reset_visible_properties() {
    let mut h = Harness::new();
    h.loader.select(Some("a.x3f".into()));
    h.started();
    h.loader.select(None);
    h.loader.select(Some("a.x3f".into()));
    h.release.send(()).unwrap();
    assert_eq!(h.started(), ("a.x3f".into(), 2));
    h.loader.receive();
    assert!(h.loader.properties.is_none());
    h.publish();
    assert_eq!(h.loader.properties.as_ref().unwrap()["ISO"], "2");
    h.loader.select(None);
    assert!(h.loader.properties.is_none());
    let state = h.loader.state.0.lock().unwrap();
    assert!(state.pending.is_none() && state.ready.is_none() && state.selected.is_none());
}

#[test]
fn selecting_another_image_clears_an_unreceived_result_and_failures_leave_no_properties() {
    let mut h = Harness::new();
    h.loader.select(Some("a.x3f".into()));
    h.started();
    h.release.send(()).unwrap();
    wait_until(|| h.loader.state.0.lock().unwrap().ready.is_some());
    h.loader.select(Some("b.x3f".into()));
    h.started();
    h.loader.receive();
    assert!(h.loader.properties.is_none());
    h.publish();

    let mut loader = Loader::with_reader(egui::Context::default(), |_| Err("invalid X3F".into()));
    loader.select(Some("bad.x3f".into()));
    wait_until(|| loader.state.0.lock().unwrap().ready.is_some());
    loader.receive();
    assert!(loader.properties.is_none());
}
