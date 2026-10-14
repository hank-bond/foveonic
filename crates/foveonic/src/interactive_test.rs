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
        settings.sensor_gain = 1.3;
        settings.exposure = 0.5;
        settings.tone.contrast = 0.25;
        settings.tone.shadows = 0.1;
        settings.tone.highlights = -0.2;
        settings.saturation = 1.2;
        settings.sharpen_amount = 0.75;
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
                0..=4 => settings.reconstruction.as_mut().unwrap().detail = step as f32 * 0.4,
                5 => settings.exposure = 0.7,
                6 => settings.tone.shadows = 0.2,
                7 => settings.saturation = 0.8,
                8 => settings.sharpen_amount = 0.,
                9 => settings.sharpen_amount = 0.6,
                10 => settings.reconstruction = None,
                11 => settings.sensor_gain = 0.,
                12 => {
                    settings.sensor_gain = 1.;
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
        eprintln!(
            "CACHE-EQUALITY {name} passed bytes={}",
            session.cached_bytes()
        );
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR; checks coupled noise recipes and every advanced override"]
fn noise_and_override_inputs_match_fresh_outputs() {
    use crate::noise::Parameter;
    let path = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(Source::open(path).unwrap());
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    settings.sensor_gain = 1.3;
    settings.sharpen_amount = 0.75;
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
    for parameter in Parameter::ALL {
        for fraction in [0.25, 0.75] {
            let mut changed = settings.clone();
            let bounds = parameter.bounds();
            changed.reconstruction.as_mut().unwrap().overrides.set(
                parameter,
                Some(*bounds.start() + (*bounds.end() - *bounds.start()) * fraction),
            );
            cases.push((format!("{}-{fraction}", parameter.key()), changed));
        }
    }
    for (label, settings) in cases {
        for movement in 0..2 {
            let mut edited = settings.clone();
            if movement == 1 {
                edited.reconstruction.as_mut().unwrap().detail = 1.4;
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
        eprintln!("NOISE-CACHE {label} exact");
    }
    assert_same_pixels(
        "retained noise/override input",
        &original,
        &engine.gpu.readback(&retained.buffer).unwrap(),
    );
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR; prints matched completed Detail feedback"]
fn detail_feedback_profile() {
    let path = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let mut session = RenderSession::new(Source::open(path).unwrap());
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    settings.sensor_gain = 1.3;
    settings.sharpen_amount = 0.75;
    let _warm = session_image(&engine, &mut session, &settings);
    engine.device().poll(wgpu::Maintain::Wait);
    for iteration in 0..6 {
        settings.reconstruction.as_mut().unwrap().detail = iteration as f32 * 0.2;
        let start = std::time::Instant::now();
        let fresh = engine
            .render(&session.source, &settings, &mut Profile::default())
            .unwrap();
        let texture = engine
            .view_texture(&fresh, None, &mut Profile::default())
            .unwrap();
        engine.device().poll(wgpu::Maintain::Wait);
        let fresh_ms = start.elapsed().as_secs_f64() * 1000.;
        drop(texture);
        drop(fresh);
        let start = std::time::Instant::now();
        let before = engine.gpu_activity();
        let cached = session_image(&engine, &mut session, &settings);
        let texture = engine
            .view_texture(&cached, None, &mut Profile::default())
            .unwrap();
        engine.device().poll(wgpu::Maintain::Wait);
        eprintln!(
            "DETAIL iteration={iteration} fresh_image_ms={fresh_ms:.3} cached_image_ms={:.3} cache_bytes={} dispatches={}",
            start.elapsed().as_secs_f64() * 1000.,
            session.cached_bytes(),
            engine.gpu_activity().since(before).dispatches
        );
        drop(texture);
    }
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
    let _ = engine
        .render_session(&mut session, &settings, &["final"], || false, &mut profile)
        .unwrap();
    eprintln!(
        "DETAIL-PROFILE {}",
        serde_json::to_string(&profile).unwrap()
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
    changed.sensor_gain = 1.7;
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
