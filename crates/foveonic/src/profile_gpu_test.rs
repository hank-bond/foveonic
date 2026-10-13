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
