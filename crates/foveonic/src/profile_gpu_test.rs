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
