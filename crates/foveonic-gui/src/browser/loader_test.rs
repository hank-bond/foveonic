//! Checks visible-first folder ordering, supersession and independent bounded preview delivery.

use super::*;
use std::time::Duration;

fn request(index: usize) -> Request {
    Request {
        path: format!("missing-{index}.x3f").into(),
        version: index as u64,
    }
}

#[test]
fn visible_requests_move_ahead_without_losing_the_remaining_folder_order() {
    let mut queue = Queue {
        requests: (0..100).map(request).collect(),
        visible: vec![request(70).path, request(71).path],
        ..Default::default()
    };
    queue.promote();
    let expected: Vec<_> = [70, 71]
        .into_iter()
        .chain((0..100).filter(|i| ![70, 71].contains(i)))
        .map(request)
        .collect();
    assert!(queue.requests.iter().eq(&expected));
    queue.visible = vec![request(20).path, request(21).path];
    queue.promote();
    assert!(queue
        .requests
        .iter()
        .take(2)
        .eq([request(20), request(21)].iter()));
    assert_eq!(queue.requests.len(), 100);
}

#[test]
fn stopping_with_full_image_replies_releases_priority_before_background_join() {
    let cache = Arc::new(Cache::new());
    let (tx, rx) = mpsc::sync_channel(REPLY_CAPACITY);
    let (loaded, loads) = mpsc::channel();
    let worker = Worker::start_with(
        Kind::Thumbnail,
        Some(cache.clone()),
        egui::Context::default(),
        tx,
        move |_| {
            loaded.send(()).unwrap();
            Err("fixture".into())
        },
    );
    worker.replace((0..REPLY_CAPACITY + 4).map(request).collect(), &[]);
    for _ in 0..=REPLY_CAPACITY {
        loads.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    let (finished, done) = mpsc::channel();
    let background = std::thread::spawn(move || {
        cache.background(|| finished.send(()).unwrap());
    });
    worker.stop();
    done.recv_timeout(Duration::from_secs(2))
        .expect("background shutdown must not wait for a full image mailbox to drain");
    background.join().unwrap();
    drop(rx);
}

#[test]
fn queued_folder_replacement_discards_old_work_and_late_results() {
    let (started, entered) = mpsc::channel();
    let (advance, wait) = mpsc::channel();
    let (tx, rx) = mpsc::sync_channel(REPLY_CAPACITY);
    let worker = Worker::start_with(
        Kind::Thumbnail,
        None,
        egui::Context::default(),
        tx,
        move |path| {
            if path == request(1).path {
                started.send(()).unwrap();
                wait.recv().unwrap();
            }
            Err("fixture".into())
        },
    );
    worker.replace(vec![request(1), request(2)], &[]);
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.replace(vec![request(8), request(9)], &[request(9).path]);
    advance.send(()).unwrap();
    let first = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let second = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(first.path, request(9).path);
    assert_eq!(second.path, request(8).path);
    assert!(first.image.is_err() && second.image.is_err());
    assert!(rx.try_recv().is_err());
}

#[test]
fn a_slow_large_preview_cannot_hold_up_the_thumbnail_worker() {
    let (started, entered) = mpsc::channel();
    let (advance, wait) = mpsc::channel();
    let (tx, rx) = mpsc::sync_channel(REPLY_CAPACITY);
    let small = Worker::start_with(
        Kind::Thumbnail,
        None,
        egui::Context::default(),
        tx.clone(),
        |_| Err("small fixture".into()),
    );
    let large = Worker::start_with(
        Kind::Preview,
        None,
        egui::Context::default(),
        tx,
        move |_| {
            started.send(()).unwrap();
            wait.recv().unwrap();
            Err("large fixture".into())
        },
    );
    large.request(request(100));
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    small.replace((0..8).map(request).collect(), &[request(6).path]);
    for index in [6, 0, 1, 2, 3, 4, 5, 7] {
        let reply = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(reply.path, request(index).path);
        assert!(reply.kind == Kind::Thumbnail);
    }
    advance.send(()).unwrap();
    assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap().kind == Kind::Preview);
}

#[test]
fn preview_misses_are_quiet_and_thumbnail_read_failures_keep_support_details() {
    const CHILD: &str = "FOVEONIC_BROWSER_PREVIEW_REPORT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "browser::loader::tests::preview_misses_are_quiet_and_thumbnail_read_failures_keep_support_details",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let mut guard = foveonic::support::initialize(foveonic::support::Config {
        directory: directory.path().join("logs"),
        application: "Browser preview test".into(),
        version: "1".into(),
        initial_context: serde_json::Value::Null,
    });
    let missing = directory.path().join("missing.x3f");
    let enabled = Arc::new(Cache::new());
    enabled.configure(foveonic::decode_cache::Settings::default());
    let disabled = Arc::new(Cache::new());
    for cache in [None, Some(enabled), Some(disabled)] {
        let (tx, rx) = mpsc::sync_channel(REPLY_CAPACITY);
        let worker = Worker::start(Kind::Preview, cache, egui::Context::default(), tx);
        worker.request(Request {
            path: missing.clone(),
            version: 1,
        });
        let reply = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(matches!(reply.image, Ok(None)));
        assert!(reply.memory.is_none());
        worker.stop();
    }
    // Both inputs are generated here. Neither needs a photographic fixture.
    let corrupt = directory.path().join("corrupt.x3f");
    std::fs::write(&corrupt, b"not an X3F file").unwrap();
    for path in [&missing, &corrupt] {
        let (tx, rx) = mpsc::sync_channel(REPLY_CAPACITY);
        let worker = Worker::start(Kind::Thumbnail, None, egui::Context::default(), tx);
        worker.request(Request {
            path: path.clone(),
            version: 1,
        });
        assert!(rx
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .image
            .is_err());
        worker.stop();
    }
    assert_eq!(
        guard.shutdown(Duration::from_secs(3)),
        foveonic::support::ShutdownStatus::Drained
    );
    let snapshot = guard.snapshot();
    let failures: Vec<_> = snapshot
        .runs
        .iter()
        .flat_map(|run| &run.events)
        .filter(|event| event.event == "operation.failed")
        .collect();
    assert_eq!(failures.len(), 2);
    for (failure, path) in failures.iter().zip([&missing, &corrupt]) {
        assert_eq!(failure.fields["boundary"], "browser.preview");
        assert_eq!(
            failure.fields["operation"]["source"],
            path.to_string_lossy().as_ref()
        );
        assert_eq!(failure.fields["backtrace_status"], "Captured");
        assert!(!failure.fields["backtrace"].as_str().unwrap().is_empty());
        assert!(!failure.fields["causes"].as_array().unwrap().is_empty());
    }
    assert!(failures[0].fields["causes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|cause| cause["io_kind"] == "NotFound" && cause["os_code"].is_number()));
}
