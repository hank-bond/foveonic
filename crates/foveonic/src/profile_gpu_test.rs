//! Verifies wall collection performs no timestamp resolutions, omits historical loading and preserves exact original pixels.

use super::*;
use crate::{
    render::{Engine, Settings},
    source::Source,
};

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn wall_profile_keeps_original_output_without_timestamp_resolutions_or_historical_loading() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("FOVEONIC_FIXTURE_DIR").expect("fixture directory"),
    );
    let source = Source::open(directory.join("blue-parking-sign.x3f")).unwrap();
    let engine = Engine::new().unwrap();
    let settings = Settings::new("Sunlight", "Standard");
    let original = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    let expected = engine.readback_image(&original).unwrap();
    let before = engine.gpu_activity();
    let mut profile = Profile::new(Mode::Wall);
    let measured = engine.render(&source, &settings, &mut profile).unwrap();
    let activity = engine.gpu_activity().since(before);
    assert_eq!(activity.timestamp_resolves, 0);
    assert_eq!(activity.readbacks, 0);
    assert!(profile.gpu.is_empty());
    assert!(profile.nodes.iter().any(|node| node.executed));
    assert!(profile
        .nodes
        .iter()
        .all(|node| node.nested_gpu_start == node.nested_gpu_end));
    assert!(profile
        .nodes
        .iter()
        .flat_map(|node| &node.outputs)
        .all(|output| !output.requested_for_execution
            || (output.required && !output.cached_available)));
    assert!(profile.host.iter().any(|t| t.stage == "prepare tone curve"));
    assert!(!profile
        .host
        .iter()
        .any(|t| t.stage == "file read" || t.stage == "X3F decode wall"));
    assert_eq!(engine.readback_image(&measured).unwrap(), expected);
    let before = engine.gpu_activity();
    let mut view_profile = Profile::new(Mode::Wall);
    let texture = engine
        .view_texture(&measured, Some([64, 96]), &mut view_profile)
        .unwrap();
    assert_eq!(texture.description().extent, [64, 96]);
    assert_eq!(engine.gpu_activity().since(before).timestamp_resolves, 0);
    assert!(view_profile.gpu.is_empty());
    assert!(view_profile
        .host
        .iter()
        .any(|t| t.stage == "submit view texture"));
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn node_observations_distinguish_executed_cached_and_pruned_outputs() {
    let directory = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = Source::open(directory.join("blue-parking-sign.x3f")).unwrap();
    let engine = Engine::new().unwrap();
    let settings = Settings::new("Sunlight", "Standard");
    let mut session = crate::interactive::RenderSession::new(source);
    let mut off = Profile::default();
    engine
        .render_session(&mut session, &settings, &["final"], || false, &mut off)
        .unwrap()
        .unwrap();
    assert!(off.nodes.is_empty());
    assert!(off.graphs.is_empty());
    let readers: Vec<_> = session
        .cache
        .buffers()
        .map(|buffer| (buffer.identity(), buffer.readers()))
        .collect();
    let checkpoint = session.checkpoints();
    assert!(!checkpoint.entries.is_empty());
    let versions: std::collections::HashSet<_> = checkpoint
        .versions
        .iter()
        .map(|version| version.version)
        .collect();
    assert!(checkpoint
        .entries
        .iter()
        .all(|entry| versions.contains(&entry.version)));
    assert!(checkpoint.versions.iter().all(|version| version
        .parents
        .iter()
        .all(|parent| versions.contains(parent))));
    assert_eq!(
        readers,
        session
            .cache
            .buffers()
            .map(|buffer| (buffer.identity(), buffer.readers()))
            .collect::<Vec<_>>(),
        "checkpoint snapshots must not retain GPU readers"
    );
    let mut wall = Profile::new(Mode::Wall);
    engine
        .render_session(&mut session, &settings, &["final"], || false, &mut wall)
        .unwrap()
        .unwrap();
    assert!(wall
        .nodes
        .iter()
        .flat_map(|node| &node.outputs)
        .any(|output| output.required
            && output.cached_available
            && !output.requested_for_execution));
    assert!(wall
        .nodes
        .iter()
        .flat_map(|node| &node.outputs)
        .any(|output| output.cached_available && output.resource.is_some()));
    assert!(!wall.graphs.is_empty());
    for node in &wall.nodes {
        let graph = wall
            .graphs
            .iter()
            .find(|graph| graph.id == node.graph_id)
            .unwrap();
        for output in &node.outputs {
            assert_eq!(graph.outputs[output.id].name, output.name);
        }
        for memory in [
            node.memory_before,
            node.memory_after_publication,
            node.memory_after_retirement,
        ] {
            assert!(
                memory.union_bytes
                    >= memory
                        .graph_bytes
                        .max(memory.scratch_bytes)
                        .max(memory.cache_bytes)
            );
            assert!(
                memory.union_bytes
                    <= memory.graph_bytes + memory.scratch_bytes + memory.cache_bytes
            );
        }
    }
    assert!(wall
        .nodes
        .iter()
        .flat_map(|node| &node.outputs)
        .any(|output| !output.required && !output.requested_for_execution));
    assert!(wall
        .nodes
        .iter()
        .filter(|node| !node.executed)
        .all(|node| node.activity.is_none() && node.host_milliseconds.is_none()));
}

#[test]
#[ignore = "requires a GPU with timestamps and FOVEONIC_FIXTURE_DIR"]
fn gpu_profile_attaches_shared_passes_to_the_node_that_submitted_them() {
    let directory = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = Source::open(directory.join("blue-parking-sign.x3f")).unwrap();
    let engine = Engine::new().unwrap();
    assert!(
        engine
            .device()
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY),
        "this ignored test requires GPU timestamp-query support"
    );
    let mut profile = Profile::new(Mode::Gpu);
    engine
        .render(
            &source,
            &Settings::new("Sunlight", "Standard"),
            &mut profile,
        )
        .unwrap();
    let node = profile
        .nodes
        .iter()
        .find(|node| node.operation == "Color" && node.executed)
        .unwrap();
    let start = node.shared_gpu_start.unwrap();
    let end = node.shared_gpu_end.unwrap();
    assert_eq!(end - start, 1);
    assert_eq!(profile.gpu[start].stage, crate::gpu::Kernel::Color.label());
    assert!(profile.nodes.iter().all(|node| node
        .shared_gpu_start
        .zip(node.shared_gpu_end)
        .is_none_or(|(start, end)| start <= end && end <= profile.gpu.len())));
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn session_node_phases_preserve_work_and_exact_pixels() {
    let directory = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source =
        std::sync::Arc::new(Source::open(directory.join("blue-parking-sign.x3f")).unwrap());
    let engine = Engine::new().unwrap();
    let mut outputs = Vec::new();
    let mut activities = Vec::new();
    for mode in [Mode::Off, Mode::Wall] {
        let engine = engine.executor();
        let mut session = crate::interactive::RenderSession::new(source.clone());
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.working_scale = crate::render::Scale::new(8).unwrap();
        engine
            .render_session(
                &mut session,
                &settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        settings.exposure = 0.25;
        let before = engine.gpu_activity();
        let mut profile = Profile::new(mode);
        let mut artifacts = engine
            .render_session(&mut session, &settings, &["final"], || false, &mut profile)
            .unwrap()
            .unwrap();
        let activity = engine.gpu_activity().since(before);
        activities.push((
            activity.dispatches,
            activity.submissions,
            activity.readbacks,
            activity.timestamp_resolves,
        ));
        let crate::inspection::Content::Image(image) = artifacts.pop().unwrap().content else {
            panic!("image expected")
        };
        outputs.push(engine.readback_image(&image).unwrap());
        if mode == Mode::Off {
            assert!(profile.nodes.is_empty() && profile.host.is_empty());
        } else {
            let executed: Vec<_> = profile.nodes.iter().filter(|n| n.executed).collect();
            assert!(!executed.is_empty());
            for node in &executed {
                let sum = node.host_milliseconds.unwrap()
                    + node.publication_ms.unwrap()
                    + node.completion_wait_ms.unwrap()
                    + node.retirement_ms.unwrap();
                assert!(node.total_ms.unwrap() >= sum, "{node:?}");
            }
            assert!(executed
                .windows(2)
                .all(|pair| pair[0].started_ms + pair[0].total_ms.unwrap() <= pair[1].started_ms));
            assert_eq!(
                executed.len(),
                profile
                    .host
                    .iter()
                    .filter(|t| t.stage == "complete interactive graph stage")
                    .count()
            );
            assert!(profile
                .nodes
                .iter()
                .filter(|n| !n.executed)
                .all(|n| n.total_ms.is_none() && n.completion_wait_ms.is_none()));
        }
    }
    assert_eq!(activities[0], activities[1]);
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR, run serialized"]
fn native_checkpoint_capture_preserves_warm_edit_work_and_pixels() {
    use crate::{diagnostics, inspection::Content, render::RenderSession};
    use std::sync::Arc;
    let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let source = Arc::new(Source::open(&path).unwrap());
    let engine = Engine::new().unwrap();
    let mut base = Settings::new("Sunlight", "Standard");
    base.photographic.tone_method = crate::render::PhotographicMethod::Universal;
    base.photographic.base_response = crate::render::BaseResponse::Natural;
    crate::render::ControlCapabilities::from_metadata(&source.metadata)
        .unwrap()
        .apply_capture_defaults(&mut base, &source.metadata)
        .unwrap();
    let mut exposure = base.clone();
    exposure.exposure = 0.25;
    let mut detail = exposure.clone();
    detail.reconstruction.as_mut().unwrap().texture.fine = 1.3;
    let warm = || {
        let mut session = RenderSession::new(source.clone());
        for settings in [&base, &exposure] {
            engine
                .render_session(
                    &mut session,
                    settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap();
        }
        engine
            .device()
            .poll(wgpu::PollType::Wait)
            .expect("GPU polling failed");
        session
    };
    let mut baseline = warm();
    let before = engine.gpu_activity();
    let mut artifacts = engine
        .render_session(
            &mut baseline,
            &detail,
            &["final"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    engine
        .device()
        .poll(wgpu::PollType::Wait)
        .expect("GPU polling failed");
    let expected_activity = engine.gpu_activity().since(before);
    let Content::Image(image) = artifacts.remove(0).content else {
        panic!("expected image")
    };
    let expected = engine.readback_image(&image).unwrap();
    drop(image);
    drop(baseline);

    let mut session = warm();
    let temporary = tempfile::tempdir().unwrap();
    let directory = std::env::var_os("FOVEONIC_CHECKPOINT_TRACE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("capture"));
    let recorder = diagnostics::Recorder::new();
    recorder
        .start(diagnostics::CaptureConfig::default())
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    let _context = diagnostics::enter(diagnostics::Context {
        task: 77,
        revision: 3,
        photo: diagnostics::photo_id(&path),
        purpose: "current",
        node: None,
    });
    let before = engine.gpu_activity();
    let mut profile = Profile::default();
    let mut artifacts = engine
        .render_session(&mut session, &detail, &["final"], || false, &mut profile)
        .unwrap()
        .unwrap();
    engine
        .device()
        .poll(wgpu::PollType::Wait)
        .expect("GPU polling failed");
    assert_eq!(
        serde_json::to_value(engine.gpu_activity().since(before)).unwrap(),
        serde_json::to_value(expected_activity).unwrap()
    );
    assert!(profile.nodes.is_empty() && profile.graphs.is_empty());
    let Content::Image(image) = artifacts.remove(0).content else {
        panic!("expected image")
    };
    let saved = recorder.stop_save(directory, serde_json::json!({"fixture":"blue-parking-sign.x3f","workload":"warm-exposure-to-detail"})).unwrap().recv().unwrap().unwrap();
    assert!(saved.trace_bytes > 0);
    assert_eq!(engine.readback_image(&image).unwrap(), expected);
}
