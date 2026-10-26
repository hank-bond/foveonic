//! Verifies exact interactive checkpoint reuse, invalidation, retained ownership and safe cancellation.

use super::*;
use std::{cell::Cell, path::PathBuf};

// Exact assertions deliberately summarize failures instead of dumping full
// photographic byte vectors. They do not rerender or change GPU scheduling.
fn assert_same_pixels(label: &str, actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len(), "{label}: readback lengths");
    assert_eq!(actual.len() % 4, 0, "{label}: float storage alignment");
    if actual == expected {
        return;
    }
    let mut count = 0;
    let mut largest = 0_f32;
    let mut first = None;
    for (i, (a, b)) in actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_chunks::<4>().0)
        .enumerate()
    {
        if a != b {
            let a = f32::from_le_bytes(*a);
            let b = f32::from_le_bytes(*b);
            first.get_or_insert((i, a, b));
            largest = largest.max((a - b).abs());
            count += 1;
        }
    }
    panic!(
        "{label}: {count} differing float components, max absolute difference={largest}, first={first:?}"
    );
}

#[test]
fn column_banding_changes_have_a_distinct_parameter_identity() {
    let settings = Settings {
        reconstruction: Some(Default::default()),
        ..Default::default()
    };
    for banding in 0..=4 {
        let mut edited = settings.clone();
        edited.reconstruction.as_mut().unwrap().banding = banding;
        assert_eq!(
            settings.changed_parameters(&edited),
            if banding == 2 {
                vec![]
            } else {
                vec!["banding-noise"]
            }
        );
        assert_eq!(settings.row_correction, edited.row_correction);
    }
}

#[test]
fn recovery_threshold_changes_have_a_distinct_parameter_identity() {
    let automatic = Settings::default();
    for value in [Some(0.), Some(1.), Some(-2.), Some(2.)] {
        let manual = Settings {
            highlight_threshold_ev: value,
            ..automatic.clone()
        };
        assert_eq!(
            automatic.changed_parameters(&manual),
            vec!["highlight-threshold"]
        );
        assert_eq!(
            manual.changed_parameters(&automatic),
            vec!["highlight-threshold"]
        );
    }
    assert!(automatic.changed_parameters(&automatic).is_empty());
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn recovery_threshold_edits_match_fresh_outputs_and_keep_retained_images() {
    let dir = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(Source::open(dir.join("sunlit-red-flower.x3f")).unwrap());
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.output_size = Some([313, 470]);
    for reconstruction in [false, true] {
        settings.reconstruction = reconstruction.then(Default::default);
        settings.highlight_threshold_ev = None;
        settings.highlight_restoration = true;
        let retained = session_image(&engine, &mut session, &settings);
        let before = engine.readback_image(&retained).unwrap();
        let mut changed = false;
        for ev in [Some(0.), Some(1.), Some(-2.), Some(2.), None] {
            settings.highlight_threshold_ev = ev;
            let cached = session_image(&engine, &mut session, &settings);
            let actual = engine.readback_image(&cached).unwrap();
            let fresh = engine
                .render(&session.source, &settings, &mut Profile::default())
                .unwrap();
            assert_same_pixels(
                "recovery threshold cached/fresh",
                &actual,
                &engine.readback_image(&fresh).unwrap(),
            );
            if ev.is_none() || ev == Some(0.) {
                assert_same_pixels("Auto/manual zero", &actual, &before);
            } else {
                changed |= actual != before;
            }
        }
        assert!(changed, "threshold did not change the red-flower render");
        settings.highlight_restoration = false;
        settings.highlight_threshold_ev = Some(1.);
        let off = session_image(&engine, &mut session, &settings);
        settings.highlight_threshold_ev = Some(-1.);
        let still_off = session_image(&engine, &mut session, &settings);
        assert_same_pixels(
            "disabled threshold",
            &engine.readback_image(&off).unwrap(),
            &engine.readback_image(&still_off).unwrap(),
        );
        settings.highlight_restoration = true;
        let on = session_image(&engine, &mut session, &settings);
        let fresh = engine
            .render(&session.source, &settings, &mut Profile::default())
            .unwrap();
        assert_same_pixels(
            "re-enabled recovery",
            &engine.readback_image(&on).unwrap(),
            &engine.readback_image(&fresh).unwrap(),
        );
        assert_same_pixels(
            "retained Auto",
            &engine.readback_image(&retained).unwrap(),
            &before,
        );
    }
}

fn session_image(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
) -> RenderedImage {
    let mut artifacts = engine
        .render_session(
            session,
            settings,
            &["final"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    match artifacts.pop().unwrap().content {
        crate::inspection::Content::Image(image) => *image,
        _ => panic!("image expected"),
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR; checks all six full-resolution originals"]
fn active_inputs_match_fresh_outputs_and_keep_retained_images_immutable() {
    let dir = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::new().unwrap();
    for name in [
        "blue-parking-sign",
        "glossy-leaves-and-buds",
        "rocks-and-tree-shadows",
        "sunlit-red-flower",
        "turquoise-still-life",
        "yellow-flowers-and-stroller",
    ] {
        let mut session =
            RenderSession::new(Source::open(dir.join(format!("{name}.x3f"))).unwrap());
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        settings.exposure = 0.5;
        settings.tone.contrast = 0.25;
        settings.tone.shadows = 0.1;
        settings.tone.highlights = -0.2;
        settings.saturation = 1.2;
        settings.sharpen_amount = Some(0.75);
        settings.output_size = Some(if session.metadata().rotation % 180 == 90 {
            [800, 1200]
        } else {
            [1200, 800]
        });
        let retained = session_image(&engine, &mut session, &settings);
        let before = engine.gpu.readback(&retained.buffer).unwrap();
        let fresh = engine
            .render(&session.source, &settings, &mut Profile::default())
            .unwrap();
        assert_same_pixels(
            &format!("cold {name}"),
            &before,
            &engine.gpu.readback(&fresh.buffer).unwrap(),
        );
        drop(fresh);
        if let Some(goldens) = std::env::var_os("FOVEONIC_PERFORMANCE_GOLDENS") {
            assert_same_pixels(
                &format!("retained performance golden {name}"),
                &before,
                &std::fs::read(PathBuf::from(goldens).join(format!("{name}.f32"))).unwrap(),
            );
        }
        let warm = session_image(&engine, &mut session, &settings);
        let exports = PathBuf::from(
            std::env::var_os("FOVEONIC_INTERACTION_EVIDENCE").unwrap_or_else(|| {
                std::env::temp_dir()
                    .join(format!("foveonic-cache-{}", std::process::id()))
                    .into()
            }),
        );
        std::fs::create_dir_all(&exports).unwrap();
        let cold_tiff = exports.join(format!("{name}-cold.tiff"));
        let warm_tiff = exports.join(format!("{name}-warm.tiff"));
        engine
            .write_tiff(&retained, &cold_tiff, &mut Profile::default())
            .unwrap();
        engine
            .write_tiff(&warm, &warm_tiff, &mut Profile::default())
            .unwrap();
        assert_eq!(
            std::fs::read(cold_tiff).unwrap(),
            std::fs::read(warm_tiff).unwrap()
        );
        drop(warm);
        for step in 0..15 {
            match step {
                0..=4 => settings.reconstruction.as_mut().unwrap().texture.fine = step as f32 * 0.4,
                5 => settings.exposure = 0.7,
                6 => settings.tone.shadows = 0.2,
                7 => settings.saturation = 0.8,
                8 => settings.sharpen_amount = Some(0.),
                9 => settings.sharpen_amount = Some(0.6),
                10 => settings.reconstruction = None,
                11 => settings.white_balance = "Overcast".into(),
                12 => {
                    settings.repair = None;
                    settings.despeckle = false;
                    settings.row_correction = false;
                    settings.highlight_restoration = false;
                }
                13 => {
                    settings.reconstruction = Some(Default::default());
                    settings.color_dq_gain = Some(0.);
                }
                14 => {
                    settings.white_balance = session.metadata().lists["WhiteBalanceGains"]
                        .names()
                        .into_iter()
                        .find(|name| *name != "Sunlight")
                        .unwrap()
                        .to_string()
                }
                _ => unreachable!(),
            }
            // Detail edits need all of their unchanged inputs ready. Switching edit
            // groups may need one render to prepare a different set of inputs.
            let activity = engine.gpu_activity();
            let cached = session_image(&engine, &mut session, &settings);
            let delta = engine.gpu_activity().since(activity);
            if step < 5 {
                assert!(
                    delta.dispatches <= 12,
                    "unexpected expensive work {name} {step}: {delta:?}"
                );
            }
            let actual = engine.gpu.readback(&cached.buffer).unwrap();
            let fresh = engine
                .render(&session.source, &settings, &mut Profile::default())
                .unwrap();
            let expected = engine.gpu.readback(&fresh.buffer).unwrap();
            assert_same_pixels(&format!("{name} step {step}"), &actual, &expected);
            assert!(session.cached_bytes() <= 1024 * 1024 * 1024);
        }
        assert_same_pixels(
            &format!("retained {name}"),
            &engine.gpu.readback(&retained.buffer).unwrap(),
            &before,
        );
        // Cancel after some stage/preparation checks, then safely resume using any valid checkpoints.
        let checks = Cell::new(0);
        let cancelled = engine
            .render_session(
                &mut session,
                &settings,
                &["final"],
                || {
                    checks.set(checks.get() + 1);
                    checks.get() > 25
                },
                &mut Profile::default(),
            )
            .unwrap();
        assert!(cancelled.is_none());
        let resumed = session_image(&engine, &mut session, &settings);
        let fresh = engine
            .render(&session.source, &settings, &mut Profile::default())
            .unwrap();
        assert_same_pixels(
            &format!("resumed {name}"),
            &engine.gpu.readback(&resumed.buffer).unwrap(),
            &engine.gpu.readback(&fresh.buffer).unwrap(),
        );
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR; checks independent cleanup and texture edits"]
fn noise_and_texture_inputs_match_fresh_outputs() {
    let path = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(Source::open(path).unwrap());
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    settings.sharpen_amount = Some(0.75);
    let retained = session_image(&engine, &mut session, &settings);
    let original = engine.gpu.readback(&retained.buffer).unwrap();
    let mut cases = Vec::new();
    for choice in [0, 1, 3, 4] {
        let mut chroma = settings.clone();
        chroma.reconstruction.as_mut().unwrap().chroma = choice;
        cases.push((format!("chroma-{choice}"), chroma));
        let mut luma = settings.clone();
        luma.reconstruction.as_mut().unwrap().luminance = choice;
        cases.push((format!("luma-{choice}"), luma));
        let mut banding = settings.clone();
        banding.reconstruction.as_mut().unwrap().banding = choice;
        cases.push((format!("banding-{choice}"), banding));
    }
    for amount in [0.25, 1.75] {
        let mut changed = settings.clone();
        changed.reconstruction.as_mut().unwrap().texture.coarse = amount;
        cases.push((format!("coarse-{amount}"), changed));
    }
    for (label, settings) in cases {
        for movement in 0..2 {
            let mut edited = settings.clone();
            if movement == 1 {
                edited.reconstruction.as_mut().unwrap().texture.fine = 1.4;
            }
            let cached = session_image(&engine, &mut session, &edited);
            let bytes = engine.gpu.readback(&cached.buffer).unwrap();
            let fresh = engine
                .render(&session.source, &edited, &mut Profile::default())
                .unwrap();
            assert_same_pixels(
                &format!("{label} movement={movement}"),
                &bytes,
                &engine.gpu.readback(&fresh.buffer).unwrap(),
            );
            assert!(session.cached_bytes() <= 1024 * 1024 * 1024);
        }
    }
    assert_same_pixels(
        "retained noise/texture input",
        &original,
        &engine.gpu.readback(&retained.buffer).unwrap(),
    );
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn sessions_bind_inputs_to_engine_and_selective_roots() {
    let path = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let mut session = RenderSession::new(Source::open(path).unwrap());
    let settings = Settings::new("Sunlight", "Standard");
    let first = Engine::new().unwrap();
    let retained = session_image(&first, &mut session, &settings);
    let original = first.gpu.readback(&retained.buffer).unwrap();
    let second = Engine::new().unwrap();
    let rebound = session_image(&second, &mut session, &settings);
    assert_same_pixels(
        "engine rebound",
        &original,
        &second.gpu.readback(&rebound.buffer).unwrap(),
    );
    assert!(second
        .view_texture(&retained, None, &mut Profile::default())
        .is_err());
    let cancelled = second
        .render_session(
            &mut session,
            &settings,
            &["sensor.layers"],
            || true,
            &mut Profile::default(),
        )
        .unwrap();
    assert!(cancelled.is_none());
    let before = second.gpu_activity();
    let artifacts = second
        .render_session(
            &mut session,
            &settings,
            &["sensor.layers"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(artifacts[0].name, "sensor.layers");
    assert_eq!(second.gpu_activity().since(before).dispatches, 0);
    let mut changed = settings.clone();
    changed.reconstruction = Some(Default::default());
    let before = second.gpu_activity();
    let cancelled = second
        .render_session(
            &mut session,
            &changed,
            &["final"],
            || second.gpu_activity().since(before).dispatches >= 4,
            &mut Profile::default(),
        )
        .unwrap();
    assert!(cancelled.is_none());
    assert!(second.gpu_activity().since(before).dispatches >= 4);
    let resumed = session_image(&second, &mut session, &changed);
    let fresh = second
        .render(&session.source, &changed, &mut Profile::default())
        .unwrap();
    assert_same_pixels(
        "engine rebound resumed",
        &second.gpu.readback(&resumed.buffer).unwrap(),
        &second.gpu.readback(&fresh.buffer).unwrap(),
    );
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_CACHE_X3F"]
fn first_exposure_edit_reuses_finishing_input_and_matches_fresh() {
    let source = Source::open(std::env::var_os("FOVEONIC_CACHE_X3F").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source);
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    let retained = session_image(&engine, &mut session, &settings);
    let retained_pixels = engine.readback_image(&retained).unwrap();
    let initial_bytes = session.cached_bytes();
    assert!(initial_bytes <= 1024 * 1024 * 1024);
    for exposure in [0.5, 0.75] {
        settings.exposure = exposure;
        let mut profile = Profile::new(crate::profile::Mode::Gpu);
        let results = engine
            .render_session(&mut session, &settings, &["final"], || false, &mut profile)
            .unwrap()
            .unwrap();
        let crate::inspection::Content::Image(cached) = &results[0].content else {
            panic!("expected image")
        };
        engine.device().poll(wgpu::Maintain::Wait);
        assert_eq!(
            profile.gpu.len(),
            1,
            "exposure should dispatch only its photographic response: {:?}",
            profile.gpu
        );
        let actual = engine.readback_image(cached).unwrap();
        let fresh = engine
            .render(&session.source, &settings, &mut Profile::default())
            .unwrap();
        assert_same_pixels(
            "first/repeated exposure cached/fresh",
            &actual,
            &engine.readback_image(&fresh).unwrap(),
        );
        assert_eq!(engine.readback_image(&retained).unwrap(), retained_pixels);
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn texture_edits_reuse_filtering_and_match_fresh_images() {
    let path = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let source = std::sync::Arc::new(Source::open(path).unwrap());
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source.clone());
    let mut settings = Settings::new("Sunlight", "Standard");
    let c = crate::noise::Controls::default();
    settings.reconstruction = Some(c);
    let base = crate::noise::prepare(&source.metadata, "Sunlight", c).unwrap();
    let pattern = crate::noise::pattern_parameters(&source.metadata, base)
        .unwrap()
        .key();
    for choice in 0..=4 {
        let selected = crate::noise::Controls {
            chroma: choice,
            luminance: 4 - choice,
            ..c
        };
        let parameters = crate::noise::prepare(&source.metadata, "Sunlight", selected).unwrap();
        assert_eq!(bytemuck::bytes_of(&parameters), bytemuck::bytes_of(&base));
        assert!(
            crate::noise::pattern_parameters(&source.metadata, parameters)
                .unwrap()
                .key()
                == pattern
        );
    }
    let retained = session_image(&engine, &mut session, &settings);
    let retained_bytes = engine.readback_image(&retained).unwrap();
    for (fine, coarse, chroma, luma, fine_size, coarse_size) in [
        (1., 0.25, 2, 2, 1., 1.),
        (1., 0.25, 2, 2, 0.83, 1.37),
        (1., 0.25, 2, 2, 2., 2.),
        (0., 0., 4, 4, 1., 1.),
        (1., 1., 0, 0, 1., 1.),
    ] {
        let c = settings.reconstruction.as_mut().unwrap();
        c.texture.fine = fine;
        c.texture.coarse = coarse;
        c.texture.fine_size = fine_size;
        c.texture.coarse_size = coarse_size;
        c.chroma = chroma;
        c.luminance = luma;
        let mut profile = Profile::new(crate::profile::Mode::Wall);
        let mut result = engine
            .render_session(&mut session, &settings, &["final"], || false, &mut profile)
            .unwrap()
            .unwrap();
        let crate::inspection::Content::Image(cached) = result.pop().unwrap().content else {
            panic!("expected image")
        };
        engine.device().poll(wgpu::Maintain::Wait);
        assert!(!profile.host.iter().any(|p| matches!(
            p.stage,
            "submit reference noise base"
                | "submit reference column conditioning"
                | "submit reference luminance ratio"
                | "submit reference pattern conditioning"
        )));
        let bytes = engine.readback_image(&cached).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .all(|v| v.is_finite()));
        let fresh = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_same_pixels(
            "texture cached/fresh",
            &bytes,
            &engine.readback_image(&fresh).unwrap(),
        );
    }
    let names = ["texture.change"];
    let mut mask_profile = Profile::new(crate::profile::Mode::Wall);
    let artifacts = engine
        .render_session(&mut session, &settings, &names, || false, &mut mask_profile)
        .unwrap()
        .unwrap();
    assert_eq!(artifacts.len(), names.len());
    assert!(!mask_profile
        .host
        .iter()
        .any(|p| p.stage == "submit tiled camera color"));
    for artifact in artifacts {
        let crate::inspection::Content::Image(image) = artifact.content else {
            panic!("expected band image")
        };
        assert_eq!(image.description.space, crate::image::Space::LuminanceRatio);
        assert_eq!(
            image.description.extent,
            [source.metadata.width as u32, source.metadata.height as u32]
        );
        let bytes = engine.readback_image(&image).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .all(|v| v.is_finite()));
        if artifact.name == "texture.change" {
            assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
                .iter()
                .all(|v| *v == 1.));
        }
    }
    settings.rendering = crate::render::Rendering::Monochrome;
    let mono_auto = settings.effective_sharpen_amount(&source.metadata).unwrap();
    settings.reconstruction.as_mut().unwrap().texture.fine = 0.4;
    assert_eq!(
        settings.effective_sharpen_amount(&source.metadata).unwrap(),
        mono_auto
    );
    assert_same_pixels(
        "immutable prior texture image",
        &retained_bytes,
        &engine.readback_image(&retained).unwrap(),
    );
}
