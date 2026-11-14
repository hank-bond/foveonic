//! Checks explicit modes, disabled collection, stage units, JSON context and cumulative activity deltas.

use super::*;

#[test]
fn modes_are_explicit_and_only_gpu_is_synchronized() {
    for (name, mode) in [("off", Mode::Off), ("wall", Mode::Wall), ("gpu", Mode::Gpu)] {
        assert_eq!(name.parse::<Mode>().unwrap(), mode);
        assert_eq!(report("test", mode, ())["synchronized"], mode == Mode::Gpu);
    }
    assert!(Mode::Wall
        .check_gpu_features(wgpu::Features::empty())
        .is_ok());
    assert!(Mode::Gpu
        .check_gpu_features(wgpu::Features::empty())
        .is_err());
    assert!(Mode::Gpu
        .check_gpu_features(wgpu::Features::TIMESTAMP_QUERY)
        .is_ok());
    assert!("true".parse::<Mode>().is_err());
    assert!("".parse::<Mode>().is_err());
}
#[test]
fn off_executes_work_without_collecting_timings() {
    let mut profile = Profile::default();
    assert_eq!(profile.measure("work", || 42), 42);
    assert!(profile.host.is_empty());
    assert!(profile.gpu.is_empty());
    assert!(profile.nodes.is_empty());
    assert!(serde_json::to_value(&profile)
        .unwrap()
        .get("nodes")
        .is_none());
}
#[test]
fn wall_collects_current_host_stages_and_serializable_settings() {
    let mut profile = Profile::new(Mode::Wall);
    profile.measure("prepare example", || ());
    assert_eq!(profile.host.len(), 1);
    assert!(profile.host[0].milliseconds >= 0.);
    assert!(profile.gpu.is_empty());
    let settings = crate::render::Settings::new("Sunlight", "Standard");
    let value = report(
        "processing",
        Mode::Wall,
        serde_json::json!({"settings":settings,"profile":profile}),
    );
    assert_eq!(value["data"]["settings"]["white_balance"], "Sunlight");
    assert_eq!(value["data"]["profile"]["mode"], "wall");
}

#[test]
fn render_settings_snapshot_preserves_transient_render_inputs() {
    let mut settings = crate::render::Settings::new("Sunlight", "Standard");
    settings.processing_quality = crate::render::ProcessingQuality::Speed;
    settings.exposure = -0.75;
    settings.working_scale = crate::render::Scale::new(8).unwrap();
    settings.preview_region = Some([1, 2, 3, 4]);
    settings.color_range_preview = Some(2);
    settings.dehaze_estimate = true;
    settings.processing_sections.noise = false;
    settings.adaptive_tone_tuning.natural.blacks = 0.2;
    let json = serde_json::to_vec(&RenderSettingsSnapshot::from(&settings)).unwrap();
    let restored: RenderSettingsSnapshot = serde_json::from_slice(&json).unwrap();
    assert_eq!(restored.into_settings(), settings);
}
#[test]
fn activity_deltas_are_per_engine_not_memory_residency() {
    let before = GpuActivity {
        storage_bytes: 100,
        submissions: 2,
        ..Default::default()
    };
    let after = GpuActivity {
        storage_bytes: 200,
        submissions: 5,
        ..Default::default()
    };
    let delta = after.since(before);
    assert_eq!(delta.storage_bytes, 100);
    assert_eq!(delta.submissions, 3);
}
