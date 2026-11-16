//! Checks support-log retention, process isolation, reports and failure fallback.

use super::*;
use serde_json::json;
use std::{
    fs, thread,
    time::{Duration, Instant},
};

fn config(dir: &std::path::Path) -> Config {
    Config {
        directory: dir.to_owned(),
        application: "test".into(),
        version: "1".into(),
        initial_context: json!({"gpu":"pending"}),
    }
}
fn wait_for(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "support writer did not settle");
        thread::yield_now();
    }
}

#[test]
fn clear_persists_cutoffs_and_copy_uses_visible_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let mut guard = store::start(config(dir.path()));
    let h = guard.handle();
    h.update_context("settings", json!({"exposure":0.3}));
    h.event(Level::Info, "before", "hidden after clear", json!({}));
    wait_for(|| {
        h.snapshot()
            .runs
            .iter()
            .any(|r| r.events.iter().any(|e| e.event == "before"))
    });
    let clear = h.request_clear();
    let mut result = None;
    wait_for(|| {
        result = clear.try_result();
        result.is_some()
    });
    result.unwrap().unwrap();
    h.event(Level::Info, "after", "still visible", json!({}));
    wait_for(|| {
        h.snapshot()
            .runs
            .iter()
            .any(|r| r.events.iter().any(|e| e.event == "after"))
    });
    let text = h.snapshot().copy_report(VisibleSelection::All);
    assert!(text.contains("still visible") && !text.contains("hidden after clear"));
    assert!(text.contains("0.3"));
    assert_eq!(
        guard.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
    let mut next = store::start(config(dir.path()));
    let text = next.snapshot().copy_report(VisibleSelection::All);
    assert!(text.contains("still visible") && !text.contains("hidden after clear"));
    assert_eq!(
        next.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
}

#[test]
fn rotation_keeps_one_launch_and_repeats_context() {
    let dir = tempfile::tempdir().unwrap();
    let mut guard = store::start(config(dir.path()));
    let h = guard.handle();
    h.update_context("settings", json!({"camera":"SDQH"}));
    for batch in 0..11 {
        for n in 0..100 {
            h.event(Level::Info, "work", &format!("{batch}-{n}"), Value::Null);
        }
        let count = (batch + 1) * 100;
        wait_for(|| {
            h.snapshot()
                .runs
                .iter()
                .flat_map(|r| &r.events)
                .filter(|e| e.event == "work")
                .count()
                >= count
        });
    }
    assert_eq!(
        guard.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
    let paths: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|s| s == "jsonl"))
        .collect();
    assert!(paths.len() >= 2);
    let mut next = store::start(config(dir.path()));
    let old = next
        .snapshot()
        .runs
        .into_iter()
        .find(|r| r.events.iter().any(|e| e.event == "work"))
        .unwrap();
    assert_eq!(old.context["settings"]["camera"], "SDQH");
    assert_eq!(
        old.events.iter().filter(|e| e.event == "work").count(),
        1100
    );
    assert_eq!(
        next.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
}

#[test]
fn malformed_tail_and_concurrent_active_segments_are_safe() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = store::start(config(dir.path()));
    let mut b = store::start(config(dir.path()));
    a.handle().event(Level::Info, "a", "first", Value::Null);
    b.handle().event(Level::Info, "b", "second", Value::Null);
    wait_for(|| {
        a.snapshot()
            .runs
            .iter()
            .any(|r| r.events.iter().any(|e| e.event == "a"))
    });
    wait_for(|| {
        b.snapshot()
            .runs
            .iter()
            .any(|r| r.events.iter().any(|e| e.event == "b"))
    });
    let paths: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    assert_eq!(paths.len(), 2);
    fs::write(dir.path().join("damaged.jsonl"), b"{\"unfinished\"").unwrap();
    a.handle().refresh_history();
    wait_for(|| !a.snapshot().incomplete.is_empty());
    assert_eq!(a.shutdown(Duration::from_secs(3)), ShutdownStatus::Drained);
    assert_eq!(b.shutdown(Duration::from_secs(3)), ShutdownStatus::Drained);
}

#[test]
fn oversize_records_are_bounded_and_disclosed() {
    let dir = tempfile::tempdir().unwrap();
    let mut guard = store::start(config(dir.path()));
    let h = guard.handle();
    h.event(
        Level::Error,
        "failure",
        "problem",
        json!({"id":"failure-123","backtrace":"\u{0}🧪".repeat(30_000),
            "message":"large error".repeat(15_000),
            "causes":[{"message":"\u{0}large cause".repeat(20_000),"io_kind":null,"os_code":null},
                {"message":"permission denied","io_kind":"PermissionDenied","os_code":5}],
            "operation":{"source":"photo.x3f","destination":"out.tiff","operation_id":"export-9","settings":{"exposure":0.7}}}),
    );
    for outer in [false, true] {
        let mut fields = json!({"operation":{"source":"photo.x3f","destination":"out.tiff","operation_id":"large-export","settings":{"label":"\u{0}".repeat(100_000)}}});
        if outer {
            fields["failure_id"] = json!("failure-large");
        } else {
            fields["id"] = json!("failure-large");
            fields["causes"] =
                json!([{"message":"denied","io_kind":"PermissionDenied","os_code":5}]);
        }
        h.event(
            Level::Error,
            if outer {
                "operation.context"
            } else {
                "operation.failed"
            },
            "Oversized frozen context",
            fields,
        );
    }
    wait_for(|| {
        h.snapshot()
            .runs
            .iter()
            .any(|r| r.events.iter().filter(|e| e.level == Level::Error).count() == 3)
    });
    let snap = h.snapshot();
    let failure = snap
        .runs
        .iter()
        .flat_map(|run| &run.events)
        .find(|record| record.event == "failure")
        .unwrap();
    assert_eq!(failure.fields["id"], "failure-123");
    assert_eq!(failure.fields["causes"][1]["os_code"], 5);
    assert_eq!(failure.fields["causes"][1]["io_kind"], "PermissionDenied");
    assert_eq!(failure.fields["operation"]["source"], "photo.x3f");
    assert_eq!(failure.fields["operation"]["operation_id"], "export-9");
    assert_eq!(failure.fields["operation"]["settings"]["exposure"], 0.7);
    assert!(serde_json::to_vec(failure).unwrap().len() + 1 < 64 * 1024);
    for record in snap
        .runs
        .iter()
        .flat_map(|run| &run.events)
        .filter(|record| record.event != "failure" && record.level == Level::Error)
    {
        assert_eq!(
            record
                .fields
                .get("id")
                .or_else(|| record.fields.get("failure_id"))
                .unwrap(),
            "failure-large"
        );
        assert_eq!(record.fields["operation"]["source"], "photo.x3f");
        assert_eq!(record.fields["operation"]["destination"], "out.tiff");
        assert_eq!(record.fields["operation"]["operation_id"], "large-export");
        assert_eq!(record.fields["operation"]["settings"]["truncated"], true);
        if record.event == "operation.failed" {
            assert_eq!(record.fields["causes"][0]["os_code"], 5);
        }
        assert!(serde_json::to_vec(record).unwrap().len() + 1 < 64 * 1024);
    }
    assert!(snap
        .copy_report(VisibleSelection::All)
        .contains("truncated"));
    assert_eq!(
        guard.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
    for entry in fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|e| e == "jsonl"))
    {
        let data = fs::read(entry.path()).unwrap();
        assert!(data
            .split(|b| *b == b'\n')
            .all(|line| line.len() < 64 * 1024));
    }
}

#[test]
fn child_panic_before_ui_is_visible_after_restart() {
    const CHILD: &str = "FOVEONIC_SUPPORT_PANIC_CHILD";
    if let Some(dir) = std::env::var_os(CHILD) {
        let _guard = initialize(config(std::path::Path::new(&dir)));
        panic!("early child failure");
    }
    let dir = tempfile::tempdir().unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "support::tests::child_panic_before_ui_is_visible_after_restart",
            "--nocapture",
        ])
        .env(CHILD, dir.path())
        .status()
        .unwrap();
    assert!(!status.success());
    let mut guard = store::start(config(dir.path()));
    let text = guard.snapshot().copy_report(VisibleSelection::All);
    assert!(text.contains("early child failure"), "{text}");
    assert!(text.contains("panic hook"), "{text}");
    assert!(
        text.contains("pending"),
        "initial launch context missing: {text}"
    );
    assert_eq!(
        guard.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
}

#[test]
fn unwritable_path_keeps_in_memory_history_and_surfaces_status() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    fs::write(&path, b"not a directory").unwrap();
    let mut guard = store::start(config(&path));
    let h = guard.handle();
    h.event(
        Level::Error,
        "test.failure",
        "in-memory report",
        Value::Null,
    );
    wait_for(|| {
        h.snapshot()
            .copy_report(VisibleSelection::All)
            .contains("in-memory report")
    });
    assert!(h.status().persistence_error.is_some());
    assert_eq!(
        guard.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
}

#[test]
fn separate_process_active_segment_survives_other_launch_rotation() {
    const CHILD: &str = "FOVEONIC_SUPPORT_LOCK_CHILD";
    if let Some(dir) = std::env::var_os(CHILD) {
        let dir = std::path::PathBuf::from(dir);
        let mut guard = store::start(config(&dir));
        guard
            .handle()
            .event(Level::Info, "child", "active child", Value::Null);
        wait_for(|| {
            guard
                .snapshot()
                .copy_report(VisibleSelection::All)
                .contains("active child")
        });
        fs::write(dir.join("child-ready"), b"ready").unwrap();
        wait_for(|| dir.join("child-release").exists());
        assert_eq!(
            guard.shutdown(Duration::from_secs(3)),
            ShutdownStatus::Drained
        );
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "support::tests::separate_process_active_segment_survives_other_launch_rotation",
        ])
        .env(CHILD, dir.path())
        .spawn()
        .unwrap();
    wait_for(|| dir.path().join("child-ready").exists());
    let child_segments: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    assert_eq!(child_segments.len(), 1);
    for index in 0..5 {
        fs::write(dir.path().join(format!("closed-{index}.jsonl")), b"").unwrap();
    }
    let mut guard = store::start(config(dir.path()));
    assert!(
        child_segments[0].exists(),
        "another process pruned an active segment"
    );
    fs::write(dir.path().join("child-release"), b"release").unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(
        guard.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
}

#[test]
fn clear_merges_other_active_launch_cutoff_without_hiding_later_events() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = store::start(config(dir.path()));
    let mut b = store::start(config(dir.path()));
    let other = b.handle();
    other.event(Level::Info, "before", "other before clear", Value::Null);
    wait_for(|| {
        other
            .snapshot()
            .copy_report(VisibleSelection::All)
            .contains("other before clear")
    });
    let request = a.request_clear();
    let mut result = None;
    wait_for(|| {
        result = request.try_result();
        result.is_some()
    });
    result.unwrap().unwrap();
    other.event(Level::Info, "after", "other after clear", Value::Null);
    wait_for(|| {
        other
            .snapshot()
            .copy_report(VisibleSelection::All)
            .contains("other after clear")
    });
    other.refresh_history();
    wait_for(|| {
        !other
            .snapshot()
            .copy_report(VisibleSelection::All)
            .contains("other before clear")
    });
    assert!(other
        .snapshot()
        .copy_report(VisibleSelection::All)
        .contains("other after clear"));
    assert_eq!(a.shutdown(Duration::from_secs(3)), ShutdownStatus::Drained);
    assert_eq!(b.shutdown(Duration::from_secs(3)), ShutdownStatus::Drained);
}

#[test]
fn large_context_round_trips_as_bounded_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let mut guard = store::start(config(dir.path()));
    let h = guard.handle();
    let value = "\"\\\n".repeat(20_000);
    h.update_context("settings", json!({"frozen":value}));
    wait_for(|| {
        h.snapshot()
            .runs
            .iter()
            .any(|r| r.context["settings"]["frozen"] == value)
    });
    assert_eq!(
        guard.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
    let mut restored = store::start(config(dir.path()));
    assert!(restored
        .snapshot()
        .runs
        .iter()
        .any(|r| r.context["settings"]["frozen"] == value));
    assert_eq!(
        restored.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
}

#[test]
fn reported_errors_keep_original_cause_and_first_capture() {
    let source: Box<dyn std::error::Error + Send + Sync> =
        Box::new(std::io::Error::from_raw_os_error(13));
    let wrapped = reported_error("write", source, OperationContext::default());
    let first = report_error("outer", wrapped.as_ref(), OperationContext::default());
    let second = report_error("ui", wrapped.as_ref(), OperationContext::default());
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    assert_eq!(first.boundary, "write");
    assert_eq!(first.causes[0].os_code, Some(13));
    assert!(wrapped.source().is_some());
    assert!(!first.backtrace_status.is_empty());
}

#[test]
fn report_dates_are_utc_and_run_selection_filters() {
    assert_eq!(utc_timestamp(0), "1970-01-01 00:00:00.000 UTC");
    assert!(utc_timestamp(u64::MAX).contains("out of range"));
    assert_eq!(
        utc_timestamp(1_700_000_000_123),
        "2023-11-14 22:13:20.123 UTC"
    );
    let snapshot = Snapshot {
        runs: vec![
            RunSnapshot {
                launch_id: "one".into(),
                ..Default::default()
            },
            RunSnapshot {
                launch_id: "two".into(),
                ..Default::default()
            },
        ],
        incomplete: Vec::new(),
    };
    let text = snapshot.copy_report(VisibleSelection::Run("one".into()));
    assert!(text.contains("Run one") && !text.contains("Run two"));
}

#[test]
fn clear_hides_empty_historical_runs_but_keeps_current_context() {
    let dir = tempfile::tempdir().unwrap();
    let mut old = store::start(config(dir.path()));
    old.handle()
        .update_context("settings", json!({"owner":"older"}));
    old.handle()
        .event(Level::Info, "old", "older event", Value::Null);
    wait_for(|| {
        old.snapshot()
            .copy_report(VisibleSelection::All)
            .contains("older event")
    });
    assert_eq!(
        old.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
    let mut current = store::start(config(dir.path()));
    current
        .handle()
        .update_context("settings", json!({"owner":"current"}));
    wait_for(|| {
        current
            .snapshot()
            .copy_report(VisibleSelection::All)
            .contains("current")
    });
    let clear = current.request_clear();
    let mut result = None;
    wait_for(|| {
        result = clear.try_result();
        result.is_some()
    });
    result.unwrap().unwrap();
    let snapshot = current.snapshot();
    let text = snapshot.copy_report(VisibleSelection::All);
    assert_eq!(snapshot.runs.len(), 1);
    assert!(text.contains("current") && !text.contains("older"));
    assert_eq!(
        current.shutdown(Duration::from_secs(3)),
        ShutdownStatus::Drained
    );
}

#[test]
fn interruption_classification_keeps_real_errors_distinct() {
    assert!(is_interrupted(&crate::background::Interrupted));
    assert!(!is_interrupted(&std::io::Error::from(
        std::io::ErrorKind::PermissionDenied
    )));
}
