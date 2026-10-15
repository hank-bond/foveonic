//! Checks that DNG samples bake technical corrections but not the selected look.

use super::*;
use crate::{
    image::{Layout, Space},
    noise,
    profile::Mode,
};

#[test]
fn export_shader_has_valid_guarded_entry_points_without_hardware() {
    use wgpu::naga::{
        front::wgsl,
        valid::{Capabilities, GlobalUse, ValidationFlags, Validator},
    };
    let source = format!(
        "{}\n{}",
        include_str!("gpu/shaders/sample.wgsl"),
        include_str!("dng/dng.wgsl")
    );
    let module = wgsl::parse_str(&source).unwrap();
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap();
    for (i, entry) in module.entry_points.iter().enumerate() {
        let function = info.get_entry_point(i);
        let mut mask = 0;
        for (handle, var) in module.global_variables.iter() {
            if function[handle].intersects(GlobalUse::WRITE | GlobalUse::ATOMIC) {
                mask |= 1 << var.binding.as_ref().unwrap().binding;
            }
        }
        assert_eq!(
            mask,
            match entry.name.as_str() {
                "convert" => (1 << 2) | (1 << 3),
                "quantize" => 1 << 5,
                _ => panic!("unexpected DNG entry point"),
            }
        );
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn technical_base_defers_rgb_presentation_but_bakes_signal_controls() {
    let engine = Engine::new().unwrap();
    let source = Source::open(
        std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f"),
    )
    .unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(noise::Controls::default());
    settings.sharpen_amount = 0.5;
    let mut profile = Profile::new(Mode::Wall);
    let base = prepare_base(&engine, &source, &settings, &mut profile).unwrap();
    assert_eq!(base.description().layout, Layout::Rgb32Float);
    assert!(matches!(base.description().space, Space::LinearRgb(_)));
    assert_eq!(
        base.description().extent,
        [source.metadata.width as u32, source.metadata.height as u32]
    );
    assert!(profile
        .host
        .iter()
        .any(|entry| entry.stage == "prepare DNG presentation mode"));
    assert!(!profile
        .host
        .iter()
        .any(|entry| entry.stage == "prepare tone curve"));
    let pixels = engine.readback_image(&base).unwrap();
    assert!(bytemuck::cast_slice::<u8, f32>(&pixels)
        .iter()
        .all(|v| v.is_finite()));

    let mut presentation = settings.clone();
    presentation.color_mode = "Vivid".into();
    presentation.exposure = 0.75;
    presentation.tone.contrast = 0.3;
    presentation.tone.shadows = 0.4;
    presentation.tone.highlights = -0.3;
    presentation.saturation = 0.25;
    let changed = prepare_base(&engine, &source, &presentation, &mut Profile::default()).unwrap();
    assert_eq!(changed.settings(), &presentation);
    assert_eq!(changed.view(), base.view());
    assert!(
        engine.readback_image(&changed).unwrap() == pixels,
        "deferred RGB presentation changed the technical pixels"
    );
    drop(changed);

    let mut signal = presentation.clone();
    signal.highlight_threshold_ev = Some(1.);
    let changed = prepare_base(&engine, &source, &signal, &mut Profile::default()).unwrap();
    assert_eq!(changed.settings().highlight_threshold_ev, Some(1.));
    assert!(
        engine.readback_image(&changed).unwrap() != pixels,
        "recovery threshold was incorrectly deferred"
    );
    drop(changed);
    signal = presentation.clone();
    signal.white_balance = "Overcast".into();
    let changed = prepare_base(&engine, &source, &signal, &mut Profile::default()).unwrap();
    assert!(
        engine.readback_image(&changed).unwrap() != pixels,
        "technical WB calibration was incorrectly deferred"
    );
    drop(changed);

    // Later fresh processing cannot mutate an earlier retained export base.
    assert!(
        engine.readback_image(&base).unwrap() == pixels,
        "retained DNG base was overwritten"
    );
    presentation.color_mode = "unknown-presentation".into();
    assert!(prepare_base(&engine, &source, &presentation, &mut Profile::default()).is_err());
}
