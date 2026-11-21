//! Checks disabled GUI probes and capture device capabilities without creating a GPU.

use super::*;
use crate::rendering::request::RequestId;

#[test]
fn painter_pass_identity_matches_the_last_completed_ui_pass() {
    let ctx = eframe::egui::Context::default();
    let observed = std::cell::Cell::new(0);
    for _ in 0..2 {
        let _ = ctx.run(Default::default(), |ctx| {
            observed.set(ctx.cumulative_pass_nr())
        });
        assert_eq!(ctx.cumulative_pass_nr().saturating_sub(1), observed.get());
    }
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
fn device_capability_allows_later_manual_gpu_capture_without_enabling_queries() {
    let supported =
        eframe::wgpu::Features::TIMESTAMP_QUERY | eframe::wgpu::Features::FLOAT32_FILTERABLE;
    assert_eq!(
        required_features(supported),
        eframe::wgpu::Features::TIMESTAMP_QUERY
    );
    assert!(required_features(eframe::wgpu::Features::FLOAT32_FILTERABLE).is_empty());
}
#[test]
fn gui_profiles_keep_aggregate_vectors_disabled() {
    assert_eq!(mode(), Mode::Off);
    let mut profile = foveonic::profile::Profile::new(mode());
    assert_eq!(profile.measure("gui work", || 42), 42);
    assert!(profile.host.is_empty());
    assert!(profile.nodes.is_empty());
    assert!(profile.gpu.is_empty());
}
#[test]
fn callback_probe_is_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<DrawProbe>();
}

#[test]
fn disabled_snapshots_are_lazy() {
    let formatted = std::cell::Cell::new(false);
    emit("disabled_test", || {
        formatted.set(true);
        serde_json::json!({"unused":"snapshot"})
    });
    assert!(!formatted.get());
}

#[test]
fn task_context_reuses_coordinator_identity_without_owning_photo_data() {
    let stamp = crate::rendering::coordinator::Stamp {
        id: crate::rendering::coordinator::TaskId(7),
        target: crate::rendering::coordinator::Target {
            path: "photo.x3f".into(),
            purpose: crate::rendering::request::Purpose::Current,
            patch: false,
        },
        request_id: RequestId(42),
    };
    let context = context(&stamp);
    assert_eq!(context.task, 7);
    assert_eq!(context.revision, 42);
    assert_eq!(context.purpose, "current");
    assert_eq!(
        context.photo,
        context_for(8, 43, &stamp.target.path, "detail").photo
    );
    assert_ne!(
        context.photo,
        context_for(7, 42, std::path::Path::new("other.x3f"), "current").photo
    );
}

#[test]
fn draw_identity_uses_delivered_task_even_after_a_new_presentation_revision() {
    let delivered = DeliveredTask {
        id: 7,
        revision: 42,
        purpose: "current",
    };
    let restored = delivered.image(99, std::path::Path::new("a.x3f"), "finish.base", 8);
    assert_eq!(restored.context.task, 7);
    assert_eq!(restored.context.revision, 42);
    assert_eq!(restored.presentation_revision, 99);
    assert_eq!(restored.artifact, "finish.base");
    assert_eq!(restored.scale, 8);
    let sibling = DeliveredTask { id: 8, ..delivered }.image(
        99,
        std::path::Path::new("b.x3f"),
        "tone.rgb",
        8,
    );
    assert_eq!(restored.context.revision, sibling.context.revision);
    assert_ne!(restored.context.task, sibling.context.task);
    assert_ne!(restored.context.photo, sibling.context.photo);
}
