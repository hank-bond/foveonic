//! Checks JSON timing units, correlation, optional device features and disabled probes without a GPU.

use super::*;
use std::time::Duration;

#[test]
fn span_records_preserve_revision_draw_and_millisecond_boundaries() {
    let origin = Instant::now();
    let record = Record {
        revision: 42,
        draw: 7,
        phase: "draw_prepare",
        origin,
        start: origin + Duration::from_millis(10),
        finish: origin + Duration::from_micros(12345),
    };
    let data = record.data();
    assert_eq!(data["revision"], 42);
    assert_eq!(data["draw"], 7);
    assert_eq!(data["start_ms"], 10.);
    assert_eq!(data["end_ms"], 12.345);
    assert_eq!(
        data["elapsed_ms"],
        Duration::from_micros(2345).as_secs_f64() * 1000.
    );
}
#[test]
fn disabled_probes_do_not_create_spans_or_drawing_state() {
    let probe = DrawProbe(None);
    assert!(probe.prepare().0.is_none());
    probe.prepared();
    assert!(probe.encode().0.is_none());
    drop(Span(None));
}
#[test]
fn durations_saturate_at_initial_clock_boundary() {
    let now = Instant::now();
    assert_eq!(milliseconds(now, now - Duration::from_millis(1)), 0.);
    assert_eq!(milliseconds(now, now + Duration::from_millis(2)), 2.);
}
#[test]
fn wall_and_off_do_not_request_gpu_features() {
    let supported =
        eframe::wgpu::Features::TIMESTAMP_QUERY | eframe::wgpu::Features::FLOAT32_FILTERABLE;
    for mode in [Mode::Off, Mode::Wall] {
        assert!(selected_features(mode, supported).is_empty());
        assert!(check_features(mode, eframe::wgpu::Features::empty()).is_ok());
    }
}
#[test]
fn gpu_diagnostics_require_only_timestamp_queries() {
    let supported =
        eframe::wgpu::Features::TIMESTAMP_QUERY | eframe::wgpu::Features::FLOAT32_FILTERABLE;
    assert_eq!(
        selected_features(Mode::Gpu, supported),
        eframe::wgpu::Features::TIMESTAMP_QUERY
    );
    assert!(selected_features(Mode::Gpu, eframe::wgpu::Features::FLOAT32_FILTERABLE).is_empty());
    assert!(check_features(Mode::Gpu, eframe::wgpu::Features::empty()).is_err());
    assert!(check_features(Mode::Gpu, eframe::wgpu::Features::TIMESTAMP_QUERY).is_ok());
}
#[test]
fn reports_share_the_core_json_envelope() {
    let report = profile::report("span", Mode::Wall, json!({"phase":"a quoted \"stage\""}));
    let roundtrip: serde_json::Value = serde_json::from_str(&report.to_string()).unwrap();
    assert_eq!(roundtrip["data"]["phase"], "a quoted \"stage\"");
    assert_eq!(roundtrip["synchronized"], false);
}
#[test]
fn callback_probe_is_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<DrawProbe>();
}
