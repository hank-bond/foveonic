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
    settings.sharpen_amount = Some(0.5);
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

#[test]
#[ignore = "requires a GPU and CLASSIC_X3F, optionally CLASSIC_DNG_DIR for reader validation"]
fn classic_dng_defers_appearance_and_preserves_editable_samples() {
    let engine = Engine::new().unwrap();
    let source = Source::open(std::env::var_os("CLASSIC_X3F").unwrap()).unwrap();
    let mut settings = Settings::new(
        std::env::var("CLASSIC_WHITE_BALANCE")
            .as_deref()
            .unwrap_or("Sunlight"),
        "Standard",
    );
    settings.reconstruction = Some(Default::default());
    settings.sharpen_amount = Some(1.);
    let prepare = |settings: &Settings| {
        let statistics = crate::render::estimated_white_balance(
            &engine,
            &source,
            settings,
            &mut Profile::default(),
        )
        .unwrap();
        let appearance =
            crate::render::dng_appearance(&source.metadata, settings, statistics).unwrap();
        let base = prepare_base(&engine, &source, settings, &mut Profile::default()).unwrap();
        assert_eq!(base.description().space, Space::LinearCodeRgb);
        let bytes = engine.readback_image(&base).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .all(|v| v.is_finite()));
        let (packed, scale) = engine
            .encode_dng_base(&base, &appearance, &mut Profile::default())
            .unwrap();
        (base, bytes, packed, scale, appearance)
    };
    let (base, bytes, packed, scale, appearance) = prepare(&settings);
    let mut changed = settings.clone();
    changed.exposure = 0.75;
    changed.tone.contrast = 0.3;
    changed.tone.shadows = 0.4;
    changed.tone.highlights = -0.3;
    changed.saturation = 0.5;
    if let Some(mode) = source
        .metadata
        .lists
        .get("ColorModeCompensations")
        .and_then(|list| {
            list.names()
                .find(|name| !name.eq_ignore_ascii_case("Standard"))
        })
    {
        changed.color_mode = mode.to_string();
    }
    let (changed_base, changed_bytes, changed_packed, changed_scale, changed_appearance) =
        prepare(&changed);
    assert_eq!(bytes, changed_bytes, "appearance changed technical base");
    assert_eq!(
        packed, changed_packed,
        "appearance changed stored DNG samples"
    );
    assert_eq!(scale, changed_scale);
    let first_profile = appearance.finish(scale).unwrap();
    let changed_profile = changed_appearance.finish(scale).unwrap();
    assert_ne!(first_profile.curve, changed_profile.curve);
    assert_ne!(first_profile.look, changed_profile.look);
    drop(changed_base);
    changed = settings.clone();
    changed.sharpen_amount = Some(0.);
    let (changed_base, changed_bytes, _, _, _) = prepare(&changed);
    assert_ne!(
        bytes, changed_bytes,
        "spatial detail was incorrectly deferred"
    );
    drop(changed_base);
    changed = settings.clone();
    changed.white_balance = "Overcast".into();
    let (changed_base, changed_bytes, _, _, _) = prepare(&changed);
    assert_ne!(
        bytes, changed_bytes,
        "WB calibration was incorrectly deferred"
    );
    drop(changed_base);
    assert_eq!(
        bytes,
        engine.readback_image(&base).unwrap(),
        "retained base was overwritten"
    );
    changed = settings.clone();
    changed.color_mode = "unavailable mode".into();
    assert!(appearance::Prepared::new(&source.metadata, &changed).is_err());
    assert!(prepare_base(&engine, &source, &changed, &mut Profile::default()).is_err());
    if let Some(dir) = std::env::var_os("CLASSIC_DNG_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let model = source
            .metadata
            .properties
            .get("CAMMODEL")
            .unwrap()
            .replace(' ', "-");
        for (suffix, settings) in [
            ("default", settings.clone()),
            ("adjusted", {
                let mut s = settings.clone();
                s.exposure = 0.75;
                s.saturation = 0.5;
                s.tone.contrast = 0.3;
                s
            }),
        ] {
            let path = dir.join(format!("{model}-{suffix}.dng"));
            write_source(&engine, &source, &settings, &path, &mut Profile::default()).unwrap();
            let file = std::fs::read(&path).unwrap();
            // Read LONG directory entries directly. The TIFF crate rejects
            // DNG's LinearRaw photometric interpretation before exposing tags.
            assert_eq!(&file[..4], b"II\x2a\0");
            let u16_at = |p| u16::from_le_bytes(file[p..p + 2].try_into().unwrap());
            let u32_at = |p| u32::from_le_bytes(file[p..p + 4].try_into().unwrap());
            let directory = u32_at(4) as usize;
            let longs = |tag| {
                let entry = (0..u16_at(directory) as usize)
                    .map(|i| directory + 2 + i * 12)
                    .find(|p| u16_at(*p) == tag)
                    .unwrap();
                assert_eq!(u16_at(entry + 2), 4);
                let count = u32_at(entry + 4) as usize;
                let start = if count == 1 {
                    entry + 8
                } else {
                    u32_at(entry + 8) as usize
                };
                (0..count)
                    .map(|i| u32_at(start + i * 4))
                    .collect::<Vec<_>>()
            };
            assert_eq!([longs(256)[0], longs(257)[0]], base.view().extent);
            assert_eq!(longs(50717), [65535; 3]);
            let offsets = longs(273);
            let counts = longs(279);
            let stored: Vec<u8> = offsets
                .iter()
                .zip(counts)
                .flat_map(|(offset, count)| {
                    file[*offset as usize..(*offset + count) as usize]
                        .iter()
                        .copied()
                })
                .collect();
            let expected: Vec<u8> = packed
                .as_chunks::<8>()
                .0
                .iter()
                .flat_map(|pixel| pixel[..6].iter().copied())
                .collect();
            assert_eq!(stored, expected, "packaging changed the uint16 samples");
            let mut photo_settings = settings.clone();
            photo_settings.tiff_color_space = Some(crate::render::TiffColorSpace::Srgb);
            let photo = engine
                .render_artifacts(
                    &source,
                    &photo_settings,
                    &["final"],
                    &mut Profile::default(),
                )
                .unwrap()
                .pop()
                .unwrap();
            let crate::inspection::Content::Image(photo) = photo.content else {
                panic!("missing photographic image")
            };
            let photographic = dir.join(format!("{model}-{suffix}-photographic.tiff"));
            // This optional directory holds reproducible test outputs.
            if photographic.exists() {
                std::fs::remove_file(&photographic).unwrap();
            }
            engine
                .write_tiff(&photo, &photographic, &mut Profile::default())
                .unwrap();
            eprintln!("Classic DNG export: {} scale={scale}", path.display());
        }
    }
}

#[test]
#[ignore = "requires a GPU and CLASSIC_X3F, optionally CLASSIC_DNG_DIR for reader validation"]
fn classic_monochrome_dng_bakes_mixture_and_defers_response() {
    let engine = Engine::new().unwrap();
    let source = Source::open(std::env::var_os("CLASSIC_X3F").unwrap()).unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.rendering = crate::render::Rendering::Monochrome;
    settings.reconstruction = Some(Default::default());
    let prepare = |settings: &Settings| {
        let statistics = crate::render::estimated_white_balance(
            &engine,
            &source,
            settings,
            &mut Profile::default(),
        )
        .unwrap();
        let appearance =
            crate::render::dng_appearance(&source.metadata, settings, statistics).unwrap();
        assert_eq!(appearance.neutral, [1.; 3]);
        assert!(appearance.baked_color_mode);
        let base = prepare_base(&engine, &source, settings, &mut Profile::default()).unwrap();
        let bytes = engine.readback_image(&base).unwrap();
        let (packed, scale) = engine
            .encode_dng_base(&base, &appearance, &mut Profile::default())
            .unwrap();
        (bytes, packed, scale, appearance)
    };
    let (bytes, packed, scale, appearance) = prepare(&settings);
    for pixel in packed.as_chunks::<8>().0 {
        let channel = |c: usize| u16::from_le_bytes(pixel[c * 2..c * 2 + 2].try_into().unwrap());
        assert_eq!(channel(0), channel(1));
        assert_eq!(channel(1), channel(2));
    }
    let samples: &[f32] = bytemuck::cast_slice(&bytes);
    assert!(samples.iter().all(|v| v.is_finite()));
    let maximum = samples.iter().copied().fold(0., f32::max);
    assert!(maximum > 1., "empty monochrome technical base");
    let spread = samples
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| {
            p[..3].iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - p[..3].iter().copied().fold(f32::INFINITY, f32::min)
        })
        .fold(0., f32::max);
    assert!(
        spread < maximum * 0.002,
        "non-neutral monochrome base: {spread}/{maximum}"
    );

    let mut changed = settings.clone();
    changed.white_balance.clear();
    let (_, changed_packed, changed_scale, _) = prepare(&changed);
    assert_eq!(
        packed, changed_packed,
        "inactive color WB changed mono samples"
    );
    assert_eq!(scale, changed_scale);

    changed = settings.clone();
    changed.exposure = 0.75;
    changed.tone.contrast = 0.3;
    changed.tone.shadows = 0.4;
    let (_, changed_packed, changed_scale, changed_appearance) = prepare(&changed);
    assert_eq!(packed, changed_packed, "tone/exposure changed mono samples");
    assert_eq!(scale, changed_scale);
    assert_ne!(
        appearance.finish(scale).unwrap().curve,
        changed_appearance.finish(scale).unwrap().curve
    );

    let mut exports = vec![
        ("monochrome", settings.clone()),
        ("monochrome-adjusted", changed),
    ];
    if source
        .metadata
        .lists
        .get("ColorModeCompensations")
        .is_some_and(|list| {
            list.names()
                .any(|name| name.trim().eq_ignore_ascii_case("Vivid"))
        })
    {
        let mut vivid = settings.clone();
        vivid.color_mode = "Vivid".into();
        let (_, vivid_packed, _, _) = prepare(&vivid);
        assert_ne!(
            packed, vivid_packed,
            "monochrome mode mixture was incorrectly deferred"
        );
        exports.push(("monochrome-vivid", vivid));
    }
    if let Some(dir) = std::env::var_os("CLASSIC_DNG_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let model = source.metadata.properties["CAMMODEL"].replace(' ', "-");
        for (suffix, mut settings) in exports {
            let destination = dir.join(format!("{model}-{suffix}.dng"));
            write_source(
                &engine,
                &source,
                &settings,
                &destination,
                &mut Profile::default(),
            )
            .unwrap();
            let exported = std::fs::read(&destination).unwrap();
            let profile_name = format!("Foveonic Monochrome {}", settings.color_mode);
            assert!(exported
                .windows(profile_name.len())
                .any(|w| w == profile_name.as_bytes()));
            assert!(!exported
                .windows(b"\"mono_noise\"".len())
                .any(|w| w == b"\"mono_noise\""));
            settings.tiff_color_space = Some(crate::render::TiffColorSpace::Srgb);
            let artifact = engine
                .render_artifacts(&source, &settings, &["final"], &mut Profile::default())
                .unwrap()
                .pop()
                .unwrap();
            let crate::inspection::Content::Image(photo) = artifact.content else {
                panic!("missing monochrome photograph")
            };
            let destination = dir.join(format!("{model}-{suffix}-photographic.tiff"));
            if destination.exists() {
                std::fs::remove_file(&destination).unwrap();
            }
            engine
                .write_tiff(&photo, &destination, &mut Profile::default())
                .unwrap();
        }
    }
}
