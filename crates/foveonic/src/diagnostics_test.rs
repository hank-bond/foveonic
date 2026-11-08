//! Checks lazy disabled instrumentation, correlation ownership and bounded Perfetto captures.

use super::*;
use std::{path::PathBuf, sync::Mutex};

static TEST: Mutex<()> = Mutex::new(());

#[test]
fn off_recording_does_not_evaluate_fields_or_change_context() {
    let _serial = TEST.lock().unwrap();
    assert!(!enabled());
    event(Category::Cpu, "disabled", |_| {
        panic!("disabled event evaluated fields")
    });
    let _scope = scope_with(Category::Cpu, "disabled", |_| {
        panic!("disabled scope evaluated fields")
    });
    let _context = enter(Context {
        task: 9,
        ..Default::default()
    });
    assert!(current_context().is_none());
    assert!(timestamp().is_none());
}

#[test]
fn sdk_capture_connects_two_threads_coalescing_counters_and_late_observations() {
    let _serial = TEST.lock().unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let directory = std::env::var_os("FOVEONIC_DIAGNOSTIC_SPIKE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("capture"));
    let recorder = Recorder::new();
    recorder
        .start(CaptureConfig {
            mode: Mode::Gpu,
            buffer_kib: 1024,
        })
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    let first_capture = capture_id().unwrap();
    let context = Context {
        task: 7,
        revision: 11,
        photo: 3,
        purpose: "current",
        node: None,
    };
    let _context = enter(context);
    flow(Category::Coordinator, "task.enqueued", 7, false, |fields| {
        fields.str("source", "local-decoded");
    });
    event(Category::Coordinator, "task.coalesced", |fields| {
        fields.u64("attempt_revision", 12);
    });
    counter(Category::Coordinator, "queue.depth", 1.);
    let before = timestamp().unwrap();
    std::thread::spawn(move || {
        with_context(Some(context), || {
            assert_eq!(current_context(), Some(context));
            let _cpu = scope(Category::Cpu, "spike.prepare");
            flow(Category::Cpu, "task.started", 7, false, |_| {});
            event(Category::Io, "input.read", |fields| {
                fields.str("storage", "local-decoded").u64("bytes", 4096);
            });
            with_context(None, || assert!(current_context().is_none()));
            assert_eq!(current_context(), Some(context));
        })
    })
    .join()
    .unwrap();
    let middle = timestamp().unwrap();
    event(Category::Cpu, "spike.overlap", |_| {});
    let end = timestamp().unwrap();
    interval(Category::Cpu, "spike.elapsed", before, end, |fields| {
        fields.u64("observation", 1);
    });
    interval(
        Category::Cpu,
        "spike.elapsed",
        middle,
        timestamp().unwrap(),
        |fields| {
            fields.u64("observation", 2);
        },
    );
    assert!(gpu_interval(
        "gpu.observed",
        &GpuInterval {
            capture_id: first_capture,
            submission: 19,
            clock_id: 1,
            start_tick: 100,
            end_tick: 160,
            nanoseconds_per_tick: 2.,
        },
        |fields| {
            fields.bool("synthetic", true);
        }
    ));
    flow(Category::Presentation, "image.used", 7, true, |fields| {
        fields.u64("artifact", 23);
    });
    counter(Category::Coordinator, "queue.depth", 0.);
    let saved = recorder
        .stop_save(
            directory,
            serde_json::json!({
                "scenario":"sdk-spike", "gpu_observations":"synthetic", "hardware":"CPU-only"
            }),
        )
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    assert!(!enabled());
    assert!(saved.trace_bytes > 0 && saved.trace_bytes < 2 * 1024 * 1024);
    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&saved.summary).unwrap()).unwrap();
    assert_eq!(summary["complete"], true);
    assert_eq!(summary["loss_checked"], false);
    assert_eq!(
        std::fs::metadata(&saved.trace).unwrap().len(),
        saved.trace_bytes
    );

    // Late device callbacks from a previous capture cannot be relabelled as new work.
    recorder
        .start(CaptureConfig {
            mode: Mode::Gpu,
            buffer_kib: 64,
        })
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    assert_ne!(capture_id(), Some(first_capture));
    assert!(!gpu_interval(
        "gpu.stale",
        &GpuInterval {
            capture_id: first_capture,
            submission: 19,
            clock_id: 1,
            start_tick: 100,
            end_tick: 160,
            nanoseconds_per_tick: 2.,
        },
        |_| panic!("old capture evaluated GPU fields")
    ));
    recorder
        .stop_save(temporary.path().join("second"), serde_json::json!({}))
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
}

#[test]
fn sdk_ring_buffer_bounds_capture_and_unsupported_configs_are_rejected() {
    let _serial = TEST.lock().unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let recorder = Recorder::new();
    assert!(recorder
        .start(CaptureConfig {
            mode: Mode::Off,
            buffer_kib: 64
        })
        .unwrap()
        .recv()
        .unwrap()
        .is_err());
    recorder
        .start(CaptureConfig {
            mode: Mode::Wall,
            buffer_kib: 64,
        })
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    let duplicate = Recorder::new();
    assert!(duplicate
        .start(CaptureConfig::default())
        .unwrap()
        .recv()
        .unwrap()
        .is_err());
    let text = "x".repeat(512);
    for index in 0..10_000 {
        event(Category::Scenario, "overflow", |fields| {
            fields.u64("index", index).str("payload", &text);
        });
    }
    std::thread::sleep(std::time::Duration::from_millis(1200));
    event(Category::Scenario, "overflow-tail-after-reset", |_| {});
    let directory = std::env::var_os("FOVEONIC_DIAGNOSTIC_SPIKE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned())
        .join("bounded");
    let saved = recorder
        .stop_save(
            directory,
            serde_json::json!({"scenario":"bounded-overflow"}),
        )
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    assert!(
        saved.trace_bytes < 256 * 1024,
        "trace bytes={}",
        saved.trace_bytes
    );
}

#[test]
fn capture_transition_never_blocks_emission_or_accepts_an_old_generation() {
    let _serial = TEST.lock().unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let recorder = Recorder::new();
    recorder
        .start(CaptureConfig {
            mode: Mode::Gpu,
            buffer_kib: 64,
        })
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    let old = capture_id().unwrap();
    let old_scope = scope(Category::Cpu, "old.scope");
    {
        // Other parallel tests can emit into the global capture. Allow those
        // brief writes, but a scope retaining its lock must still fail promptly.
        let deadline = Instant::now() + std::time::Duration::from_secs(1);
        let _transition = loop {
            match EMISSION.try_write() {
                Ok(transition) => break transition,
                Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::yield_now();
                }
                Err(error) => panic!("scope retained an emission lock: {error}"),
            }
        };
        event(Category::Ui, "transition", |_| {
            panic!("blocked emitter evaluated fields")
        });
        let _scope = scope_with(Category::Cpu, "transition", |_| {
            panic!("blocked scope evaluated fields")
        });
        assert!(!event_in_capture(
            old,
            Category::Gpu,
            "transition",
            |_| panic!("blocked callback evaluated fields")
        ));
    }
    assert!(TRANSITION_DROPS.load(Ordering::Relaxed) >= 3);
    recorder
        .stop_save(
            temporary.path().join("old"),
            serde_json::json!({"scenario":"capture-transition"}),
        )
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    recorder
        .start(CaptureConfig {
            mode: Mode::Gpu,
            buffer_kib: 64,
        })
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    let current_scope = scope(Category::Cpu, "current.scope");
    drop(old_scope);
    assert!(!event_in_capture(old, Category::Gpu, "stale", |_| panic!(
        "stale callback evaluated fields"
    )));
    drop(current_scope);
    recorder
        .stop_save(
            temporary.path().join("current"),
            serde_json::json!({"scenario":"capture-generation"}),
        )
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
}

#[test]
#[ignore = "CPU-only diagnostic microbenchmark, run with --release --nocapture"]
fn disabled_and_enabled_recording_overhead() {
    let _serial = TEST.lock().unwrap();
    let temporary = tempfile::tempdir().unwrap();
    const CALLS: usize = 100_000;
    let measure = |enabled: bool| {
        let began = Instant::now();
        for index in 0..CALLS {
            let _scope = scope(Category::Cpu, std::hint::black_box("overhead.scope"));
            event(Category::Cpu, "overhead.event", |fields| {
                fields.u64("index", index as u64);
            });
        }
        (enabled, began.elapsed().as_nanos() as f64 / CALLS as f64)
    };
    let off = measure(false);
    let recorder = Recorder::new();
    recorder
        .start(CaptureConfig {
            mode: Mode::Wall,
            buffer_kib: 1024,
        })
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    let on = measure(true);
    recorder
        .stop_save(
            temporary.path().join("overhead"),
            serde_json::json!({"scenario":"recorder-overhead"}),
        )
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    println!(
        "{}",
        serde_json::json!({"scenario":"recorder-overhead", "calls":CALLS,
        "off_scope_plus_event_ns":off.1,"enabled_scope_plus_event_ns":on.1,
        "timing_kind":"instrumentation microbenchmark, not application latency"})
    );
}
