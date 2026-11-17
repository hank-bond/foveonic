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

#[test]
fn retained_observations_survive_off_mode_and_count_nested_capacity() {
    let bytes = |profile: &Profile| {
        let mut retained = crate::recipe::RetainedBytes::default();
        profile.account(&mut retained);
        retained.bytes()
    };
    let mut profile = Profile::new(Mode::Wall);
    profile.host.reserve(5);
    profile.gpu.reserve(3);
    profile.nodes.push(NodeObservation {
        graph_id: 1,
        ordinal: 0,
        operation: "example",
        outputs: vec![NodeOutput {
            id: 0,
            name: String::with_capacity(31),
            required: true,
            cached_available: false,
            requested_for_execution: true,
            published: true,
            resource: Some(ResourceObservation {
                allocation_id: 1,
                generation: 0,
                byte_len: 4096,
                working_scale: 1,
                description: Some(ImageDescription {
                    extent: [16, 16],
                    layout: String::with_capacity(41),
                    space: String::with_capacity(53),
                    view_baked: false,
                }),
                view: None,
            }),
        }],
        executed: true,
        host_milliseconds: None,
        started_ms: 0.,
        publication_ms: None,
        completion_wait_ms: None,
        retirement_ms: None,
        total_ms: None,
        activity: None,
        nested_gpu_start: 0,
        nested_gpu_end: 0,
        shared_gpu_start: None,
        shared_gpu_end: None,
        memory_before: Default::default(),
        memory_after_publication: Default::default(),
        memory_after_retirement: Default::default(),
        diagnostic_overhead_ms: 0.,
    });
    profile.graphs.push(GraphObservation {
        id: 1,
        requested_roots: Vec::with_capacity(3),
        outputs: vec![GraphOutput {
            id: 0,
            name: String::with_capacity(17),
            producer: 0,
            dependencies: Vec::with_capacity(7),
            data_type: String::with_capacity(29),
            session_hint: false,
            sealed_cut: false,
        }],
        boundary_memory_scope: "example",
    });
    let populated = bytes(&profile);
    profile.mode = Mode::Off;
    assert_eq!(bytes(&profile), populated);
    let name = &mut profile.nodes[0].outputs[0].name;
    let before = name.capacity();
    name.reserve(128);
    let growth = name.capacity() - before;
    assert_eq!(bytes(&profile), populated + growth);
    let before = bytes(&profile);
    let node = &profile.nodes[0];
    let output = &node.outputs[0];
    let description = output
        .resource
        .as_ref()
        .unwrap()
        .description
        .as_ref()
        .unwrap();
    let nested = node.outputs.capacity() * size_of::<NodeOutput>()
        + output.name.capacity()
        + description.layout.capacity()
        + description.space.capacity();
    profile.nodes.clear();
    assert_eq!(before - bytes(&profile), nested);
    profile.graphs.clear();
    assert_eq!(
        bytes(&profile),
        profile.host.capacity() * size_of::<Timing>()
            + profile.gpu.capacity() * size_of::<Timing>()
            + profile.nodes.capacity() * size_of::<NodeObservation>()
            + profile.graphs.capacity() * size_of::<GraphObservation>(),
    );
    profile.host.shrink_to_fit();
    profile.gpu.shrink_to_fit();
    profile.nodes.shrink_to_fit();
    profile.graphs.shrink_to_fit();
    assert_eq!(bytes(&profile), 0);
}
