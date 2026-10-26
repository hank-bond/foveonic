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
