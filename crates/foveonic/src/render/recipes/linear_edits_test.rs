//! Checks pre-curve spatial edits, matching DNG gains, bypasses and cached editing on originals.

use crate::{
    image::Space,
    inspection::Content,
    profile::{Mode, Profile},
    render::{
        ControlCapabilities, Engine, PhotographicMethod, PreviewQuality, RenderSession, Rendering,
        Settings,
    },
    source::Source,
};
use std::sync::Arc;

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_dehaze_swatch_reports_auto_at_zero_and_freezes_without_an_image_change() {
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap());
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut settings = caps.editing_defaults(&source.metadata, rendering).unwrap();
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        let neutral = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let neutral = engine.readback_image(&neutral).unwrap();
        settings.dehaze_estimate = true;
        let mut session = RenderSession::new(source.clone());
        let mut automatic_pixels = None;
        let mut color = None;
        for (amount, manual) in [(0., false), (35., false), (35., true), (0., false)] {
            settings.dehaze.amount = amount;
            settings.dehaze.shadow = if manual { color } else { None };
            let mut artifacts = engine
                .render_session(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap();
            let artifact = artifacts.pop().unwrap();
            if manual {
                assert!(
                    artifact.dehaze_estimate.is_none(),
                    "manual color must not request color estimation"
                );
            } else {
                let estimate = artifact
                    .dehaze_estimate
                    .expect("fixture needs an automatic haze color");
                if let Some(expected) = color {
                    assert_eq!(estimate, expected);
                }
                color = Some(estimate);
            }
            let Content::Image(image) = artifact.content else {
                panic!("image")
            };
            let pixels = engine.readback_image(&image).unwrap();
            if amount == 0. {
                assert!(
                    pixels == neutral,
                    "showing Auto must preserve zero output exactly"
                );
            } else if manual {
                assert!(
                    Some(pixels) == automatic_pixels,
                    "freezing Auto must preserve the image exactly"
                );
            } else {
                automatic_pixels = Some(pixels);
            }
        }
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_dehaze_shares_whole_frame_analysis_with_fast_and_regional_correction() {
    use crate::interactive::Execution;
    let source = Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = caps
        .editing_defaults(&source.metadata, Rendering::Color)
        .unwrap();
    settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
    settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
    settings.dehaze.amount = 35.;
    settings.dehaze.shadow = None;
    assert!(settings.fast_preview_available(&source.metadata));
    assert!(settings.regional_preview_available(&source.metadata));
    let mut full_extent = None;
    let mut whole_frame_analysis = None;
    let mut fast_pixels = None;
    for (quality, region) in [
        (PreviewQuality::Full, None),
        (PreviewQuality::Fast, None),
        (PreviewQuality::Full, Some([10, 10, 200, 200])),
    ] {
        settings.preview_quality = quality;
        settings.preview_region = region;
        let mut profile = Profile::default();
        let mut execution = Execution::default();
        let built = super::super::build_camera(
            &engine,
            &source,
            &settings,
            &["final"],
            &mut profile,
            &mut execution,
        )
        .unwrap();
        let mut artifacts = engine
            .collect_artifacts(
                &source,
                &settings,
                &built.graph,
                built.final_id,
                &["dehaze.analysis", "final"],
                built.params,
                &mut profile,
                &mut execution,
            )
            .unwrap();
        let Content::Image(image) = artifacts.pop().unwrap().content else {
            panic!("image")
        };
        let Content::Table { values, .. } = artifacts.pop().unwrap().content else {
            panic!("analysis")
        };
        assert!(values[3] > 0., "fixture needs an Auto estimate");
        if let Some(expected) = &whole_frame_analysis {
            assert_eq!(&values, expected, "{quality:?} {region:?}");
        } else {
            whole_frame_analysis = Some(values);
        }
        let extent = image.view().extent;
        if quality.is_reduced() {
            assert_eq!(Some(extent.map(|v| v * 2)), full_extent);
        } else if region.is_some() {
            assert_eq!(extent, [190, 190]);
        } else {
            full_extent = Some(extent);
        }
        let bytes = engine.readback_image(&image).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .all(|v| v.is_finite()));
        if quality == PreviewQuality::Fast {
            fast_pixels = Some(bytes);
        }
    }
    let mut session = RenderSession::new(Arc::new(source));
    settings.preview_quality = PreviewQuality::Fast;
    settings.preview_region = None;
    for amount in [35., 45., 35.] {
        settings.dehaze.amount = amount;
        let mut profile = Profile::new(Mode::Wall);
        let mut artifacts = engine
            .render_session(&mut session, &settings, &["final"], || false, &mut profile)
            .unwrap()
            .unwrap();
        let Content::Image(image) = artifacts.pop().unwrap().content else {
            panic!("image")
        };
        let actual = engine.readback_image(&image).unwrap();
        assert_eq!(actual == *fast_pixels.as_ref().unwrap(), amount == 35.);
        if amount == 45. {
            assert!(!profile
                .host
                .iter()
                .any(|t| t.stage == "estimate haze color"));
        }
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_dehaze_matches_cached_edits_dng_and_full_preview() {
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap());
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut settings = caps.editing_defaults(&source.metadata, rendering).unwrap();
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        settings.dehaze.amount = 35.;
        let started = std::time::Instant::now();
        let mut session = RenderSession::new(source.clone());
        let mut artifacts = engine
            .render_session(
                &mut session,
                &settings,
                &["dehaze.input", "linear.edits"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        let Content::Image(after) = artifacts.pop().unwrap().content else {
            panic!("image")
        };
        let Content::Image(before) = artifacts.pop().unwrap().content else {
            panic!("image")
        };
        let white = f64::from(after.description().linear_white);
        let expected = engine.readback_image(&after).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&expected)
            .iter()
            .all(|v| v.is_finite()));
        assert!(
            engine.readback_image(&before).unwrap() != expected,
            "Auto must correct the original fixture"
        );
        let picked = (1..10)
            .flat_map(|y| (1..10).map(move |x| [x as f32 / 10., y as f32 / 10.]))
            .find_map(|uv| engine.pick_haze_color(&before, uv).ok())
            .expect("fixture needs at least one usable haze-color patch");
        assert!(picked
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v)));
        drop(after);
        drop(before);
        let reference_ms = started.elapsed().as_secs_f64() * 1000.;
        let stats = super::super::estimated_white_balance(
            &engine,
            &source,
            &settings,
            &mut Profile::default(),
        )
        .unwrap();
        let appearance =
            super::super::prepare_dng_appearance(&source.metadata, &settings, stats).unwrap();
        let inverse = if settings.is_monochrome() {
            appearance.technical_from_monochrome().unwrap()
        } else {
            appearance.technical_from_native().unwrap()
        }
        .inverse()
        .unwrap();
        let dng = engine
            .render_dng_base(&source, &settings, &mut Profile::default())
            .unwrap();
        let bytes = engine.readback_image(&dng).unwrap();
        drop(dng);
        let actual: &[[f32; 3]] = bytemuck::cast_slice(&bytes);
        let target: &[[f32; 3]] = bytemuck::cast_slice(&expected);
        for i in (0..target.len()).step_by(97) {
            let rgb = inverse.apply(actual[i].map(f64::from));
            for c in 0..3 {
                assert!(
                    (rgb[c] * white - f64::from(target[i][c])).abs()
                        < 0.03 + f64::from(target[i][c].abs()) * 0.00003,
                    "{rendering:?} at {i}: {rgb:?} != {:?}",
                    target[i]
                );
            }
            if settings.is_monochrome() {
                assert_eq!(target[i][0], target[i][1]);
                assert_eq!(target[i][1], target[i][2]);
            }
        }
        drop(bytes);
        let mut warm_ms = 0.;
        for amount in [45., 35.] {
            settings.dehaze.amount = amount;
            settings.preview_quality = PreviewQuality::Fast;
            let start = std::time::Instant::now();
            let mut profile = Profile::new(Mode::Wall);
            let mut artifacts = engine
                .render_session(
                    &mut session,
                    &settings,
                    &["linear.edits"],
                    || false,
                    &mut profile,
                )
                .unwrap()
                .unwrap();
            let Content::Image(image) = artifacts.remove(0).content else {
                panic!("image")
            };
            let cached = engine.readback_image(&image).unwrap();
            if amount == 35. {
                assert!(
                    cached == expected,
                    "cached output must match the fresh result"
                );
            } else {
                assert!(cached != expected, "Amount must change the output");
            }
            warm_ms = start.elapsed().as_secs_f64() * 1000.;
            assert!(
                !profile
                    .host
                    .iter()
                    .any(|t| t.stage == "estimate haze color"),
                "Amount changes must reuse the estimate"
            );
        }
        eprintln!(
            "{rendering:?}: Dehaze original {reference_ms:.1} ms, warm amount {warm_ms:.1} ms"
        );
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_selected_lightness_match_dng_and_cached_color_and_monochrome() {
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap());
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut settings = caps.editing_defaults(&source.metadata, rendering).unwrap();
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        settings
            .photographic
            .color_editor
            .advanced
            .push(crate::render::AdvancedColorRange {
                width: 360.,
                adjustment: crate::nodes::finish::color_editor::Adjustment {
                    lightness: -0.3,
                    ..Default::default()
                },
                ..Default::default()
            });
        settings.exposure = -0.5;
        let mut artifacts = engine
            .render_artifacts(
                &source,
                &settings,
                &["linear.edits"],
                &mut Profile::default(),
            )
            .unwrap();
        let Content::Image(reference) = artifacts.remove(0).content else {
            panic!("image")
        };
        let white = reference.description().linear_white as f64;
        let expected = engine.readback_image(&reference).unwrap();
        drop(reference);
        let image = engine
            .render_dng_base(&source, &settings, &mut Profile::default())
            .unwrap();
        let actual = engine.readback_image(&image).unwrap();
        drop(image);
        settings.color_range_preview = None;
        let statistics = super::super::estimated_white_balance(
            &engine,
            &source,
            &settings,
            &mut Profile::default(),
        )
        .unwrap();
        let appearance =
            super::super::prepare_dng_appearance(&source.metadata, &settings, statistics).unwrap();
        let reverse = if settings.is_monochrome() {
            appearance.technical_from_monochrome().unwrap()
        } else {
            appearance.technical_from_native().unwrap()
        };
        let forward = reverse.inverse().unwrap();
        let expected_values: &[[f32; 3]] = bytemuck::cast_slice(&expected);
        let actual_values: &[[f32; 3]] = bytemuck::cast_slice(&actual);
        assert_eq!(expected_values.len(), actual_values.len());
        for i in (0..expected_values.len()).step_by(13) {
            let projected = forward.apply(actual_values[i].map(f64::from));
            for c in 0..3 {
                let target = f64::from(expected_values[i][c]);
                assert!(
                    (projected[c] * white - target).abs() < 0.03 + target.abs() * 0.00002,
                    "{rendering:?} pixel {i}: {projected:?} vs {:?}",
                    expected_values[i]
                );
            }
            if settings.is_monochrome() {
                assert_eq!(expected_values[i][0], expected_values[i][1]);
                assert_eq!(expected_values[i][1], expected_values[i][2]);
                assert_eq!(actual_values[i][0], actual_values[i][1]);
                assert_eq!(actual_values[i][1], actual_values[i][2]);
            }
        }
        let mut session = RenderSession::new(source.clone());
        for enabled in [true, false, true] {
            settings.photographic.color_editor.advanced.ranges[0].enabled = enabled;
            let mut result = engine
                .render_session(
                    &mut session,
                    &settings,
                    &["linear.edits"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap();
            let Content::Image(image) = result.remove(0).content else {
                panic!("image")
            };
            let cached = engine.readback_image(&image).unwrap();
            if enabled {
                assert_eq!(cached, expected);
            } else {
                assert_ne!(cached, expected);
            }
        }
    }
}

#[test]
#[ignore = "requires GPU, FOVEONIC_FINISH_SOURCE, optional FOVEONIC_EDIT_RECIPE"]
fn original_spatial_edits_bake_same_gain_into_dng_and_keep_appearance_separate() {
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap());
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let engine = Engine::new().unwrap();
    let saved = std::env::var_os("FOVEONIC_EDIT_RECIPE")
        .map(|p| crate::sidecar::decode(&std::fs::read(p).unwrap()).unwrap());
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut s = saved
            .clone()
            .unwrap_or_else(|| caps.editing_defaults(&source.metadata, rendering).unwrap());
        s.rendering = rendering;
        if s.white_balance.is_empty() {
            s.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        }
        if s.color_mode.is_empty() {
            s.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        }
        s.photographic.tone_method = PhotographicMethod::Universal;
        s.photographic.color_editor = crate::render::ColorEditorControls::from_lightness([
            0.06,
            (0.06 + 0.87) * 0.5,
            0.87,
            0.,
            0.,
            0.,
            (0. + 0.) * 0.5,
            0.,
        ]);
        s.tonal_ranges = crate::render::TonalRanges {
            blacks: 0.22,
            shadows: -0.5,
            highlights: -1.,
            whites: -0.63,
        };
        s.photographic.sharpening.method = PhotographicMethod::Universal;
        s.photographic.sharpening.amount = 0.;
        s.photographic.brightness = 0.25;
        let mut neutral = s.clone();
        neutral.photographic.brightness = 0.;
        neutral.tonal_ranges = Default::default();
        neutral.photographic.local_contrast = 0.;
        neutral.photographic.color_editor = Default::default();
        let plain = engine
            .render_dng_base(&source, &neutral, &mut Profile::default())
            .unwrap();
        let native_extent = plain.description().extent;
        let plain_bytes = engine.readback_image(&plain).unwrap();
        drop(plain);
        for amount in [0.21, 1., -1.] {
            s.photographic.local_contrast = amount;
            s.photographic.sharpening.amount = 0.8;
            let mut a = engine
                .render_artifacts(
                    &source,
                    &s,
                    &["linear.edits-input", "linear.edits"],
                    &mut Profile::default(),
                )
                .unwrap();
            let mut buffers = Vec::new();
            for image in a.drain(..) {
                let Content::Image(image) = image.content else {
                    panic!("image")
                };
                assert!(matches!(image.description().space, Space::LinearRgb(_)));
                let white = match caps.family {
                    crate::source::ProcessingFamily::Merrill => 4095,
                    crate::source::ProcessingFamily::Classic => 4096,
                    crate::source::ProcessingFamily::Quattro => 16384,
                };
                assert_eq!(image.description().linear_white, white);
                buffers.push(engine.readback_image(&image).unwrap());
            }
            let before: &[[f32; 3]] = bytemuck::cast_slice(&buffers[0]);
            let after: &[[f32; 3]] = bytemuck::cast_slice(&buffers[1]);
            let baked = engine
                .render_dng_base(&source, &s, &mut Profile::default())
                .unwrap();
            let bytes = engine.readback_image(&baked).unwrap();
            drop(baked);
            assert!(
                bytes != plain_bytes,
                "active spatial edits must change DNG samples"
            );
            let actual: &[[f32; 3]] = bytemuck::cast_slice(&bytes);
            let original: &[[f32; 3]] = bytemuck::cast_slice(&plain_bytes);
            for i in (0..actual.len()).step_by(11) {
                let anchor = (0..3)
                    .max_by(|a, b| before[i][*a].abs().total_cmp(&before[i][*b].abs()))
                    .unwrap();
                let gain = if before[i][anchor] != 0. {
                    after[i][anchor] / before[i][anchor]
                } else {
                    1.
                };
                for c in 0..3 {
                    let expected = original[i][c] * gain;
                    assert!(actual[i][c].is_finite());
                    assert!(
                        (actual[i][c] - expected).abs() < 0.02 + expected.abs() * 0.000002,
                        "{rendering:?} amount {amount}, pixel {i}: {} vs {expected}",
                        actual[i][c]
                    );
                }
            }
        }
        let mut hidden = s.clone();
        hidden.tone_enabled = false;
        hidden.color_editor_enabled = false;
        hidden.sharpening_enabled = false;
        let bypass = engine
            .render_dng_base(&source, &hidden, &mut Profile::default())
            .unwrap();
        assert_eq!(
            engine.readback_image(&bypass).unwrap(),
            plain_bytes,
            "tone bypass changes baked samples"
        );
        drop(bypass);
        s.preview_quality = PreviewQuality::Fast;
        s.preview_region = Some([10, 10, 200, 200]);
        let full = engine
            .render_dng_base(&source, &s, &mut Profile::default())
            .unwrap();
        assert_eq!(full.description().extent, native_extent);
        engine.readback_image(&full).unwrap();
        drop(full);
        s.preview_quality = PreviewQuality::Full;
        s.preview_region = None;
        let mut session = RenderSession::new(source.clone());
        let render = |session: &mut RenderSession, s: &Settings, p: &mut Profile| {
            let mut a = engine
                .render_session(session, s, &["final"], || false, p)
                .unwrap()
                .unwrap();
            let Content::Image(image) = a.remove(0).content else {
                panic!("image")
            };
            image
        };
        let _ = render(&mut session, &s, &mut Profile::default());
        s.photographic.local_contrast = 0.31;
        let mut p = Profile::new(Mode::Wall);
        let cached = render(&mut session, &s, &mut p);
        assert!(
            !p.host.iter().any(|t| matches!(
                t.stage,
                "upload sensor phase cells"
                    | "filter channel noise"
                    | "submit calibration"
                    | "submit reference noise base"
                    | "submit tonal brightness mask"
            )),
            "Local Contrast edit rebuilt unchanged processing: {:?}",
            p.host
        );
        let fresh = engine.render(&source, &s, &mut Profile::default()).unwrap();
        assert_eq!(
            engine.readback_image(&cached).unwrap(),
            engine.readback_image(&fresh).unwrap()
        );
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_output_sections_bypass_edits_without_changing_authored_settings() {
    let source = Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let engine = Engine::new().unwrap();
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut settings = caps.editing_defaults(&source.metadata, rendering).unwrap();
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        settings.photographic.tone_method = PhotographicMethod::Universal;
        settings.photographic.saturation_method = crate::render::PhotographicMethod::Universal;
        settings.photographic.brightness = 0.4;
        settings.photographic.perceptual.saturation = 0.2;
        settings.photographic.color_editor.bands[0].lightness = 0.4;
        settings.tonal_ranges.shadows = 0.3;
        settings.photographic.local_contrast = 0.2;
        settings.photographic.sharpening.method = PhotographicMethod::Universal;
        settings.photographic.sharpening.amount = 0.6;
        let authored = serde_json::to_vec(&settings).unwrap();
        let mut explicit = settings.clone();
        explicit.tone_enabled = false;
        explicit.saturation_enabled = false;
        explicit.sharpening_enabled = false;
        let expected_image = engine
            .render_dng_base(&source, &explicit, &mut Profile::default())
            .unwrap();
        let native_extent = expected_image.view().extent;
        let expected = engine.readback_image(&expected_image).unwrap();
        drop(expected_image);
        settings.processing_sections.tone = false;
        settings.processing_sections.saturation = false;
        settings.processing_sections.sharpening = false;
        let output = engine
            .render_dng_base(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(engine.readback_image(&output).unwrap(), expected);
        drop(output);
        for sections in [
            crate::render::ProcessingSections {
                noise: false,
                ..Default::default()
            },
            crate::render::ProcessingSections {
                detail: false,
                ..Default::default()
            },
            crate::render::ProcessingSections {
                corrections: false,
                ..Default::default()
            },
        ] {
            settings.processing_sections = sections;
            let output = engine
                .render(&source, &settings, &mut Profile::default())
                .unwrap_or_else(|error| panic!("{rendering:?} {sections:?}: {error}"));
            let bytes = engine.readback_image(&output).unwrap();
            assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
                .iter()
                .all(|v| v.is_finite()));
            drop(output);
            drop(bytes);
            let output = engine
                .render_dng_base(&source, &settings, &mut Profile::default())
                .unwrap();
            assert_eq!(output.view().extent, native_extent);
            let bytes = engine.readback_image(&output).unwrap();
            assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
                .iter()
                .all(|v| v.is_finite()));
        }
        assert_eq!(serde_json::to_vec(&settings).unwrap(), authored);
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_perceptual_neutral_and_cached_edits_keep_reconstruction_unchanged() {
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap());
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let engine = Engine::new().unwrap();
    let mut s = caps
        .editing_defaults(&source.metadata, Rendering::Color)
        .unwrap();
    s.white_balance = caps.default_white_balance(&source.metadata).unwrap();
    s.color_mode = caps.default_color_mode(&source.metadata).unwrap();
    s.photographic.tone_method = PhotographicMethod::Reference;
    let baseline = engine.render(&source, &s, &mut Profile::default()).unwrap();
    let expected = engine.readback_image(&baseline).unwrap();
    drop(baseline);
    s.photographic.saturation_method = crate::render::PhotographicMethod::Universal;
    let neutral = engine.render(&source, &s, &mut Profile::default()).unwrap();
    assert_eq!(engine.readback_image(&neutral).unwrap(), expected);
    drop(neutral);
    drop(expected);
    s.tonal_ranges.shadows = 0.3;
    s.tonal_ranges.highlights = -0.4;
    s.photographic.brightness = 0.3;
    s.photographic.perceptual.saturation = 0.15;
    s.photographic.perceptual.contrast = -0.3;
    let mut session = RenderSession::new(source.clone());
    let _ = engine
        .render_session(
            &mut session,
            &s,
            &["final"],
            || false,
            &mut Profile::default(),
        )
        .unwrap();
    s.photographic.perceptual.contrast = 0.4;
    s.prevent_color_clipping = false;
    s.photographic.perceptual.saturation = 2.;
    s.photographic.color_editor.master.saturation = 2.;
    let mut range = crate::render::AdvancedColorRange {
        width: 360.,
        ..Default::default()
    };
    range.adjustment.saturation = 2.;
    s.photographic.color_editor.advanced.push(range);
    let mut profile = Profile::new(Mode::Wall);
    let mut cached = engine
        .render_session(&mut session, &s, &["final"], || false, &mut profile)
        .unwrap()
        .unwrap();
    assert!(
        !profile.host.iter().any(|t| matches!(
            t.stage,
            "upload sensor phase cells" | "submit calibration" | "submit reference noise base"
        )),
        "grading edit rebuilt reconstruction: {:?}",
        profile.host
    );
    let Content::Image(cached) = cached.remove(0).content else {
        panic!("image")
    };
    let expected = engine.readback_image(&cached).unwrap();
    drop(cached);
    let fresh = engine.render(&source, &s, &mut Profile::default()).unwrap();
    assert_eq!(engine.readback_image(&fresh).unwrap(), expected);
}

#[test]
#[ignore = "requires a GPU"]
fn scope_normalization_follows_declared_linear_white() {
    use crate::image::{Description, RenderedImage, Space, View};
    let engine = Engine::new().unwrap();
    let space = crate::color_space::ColorSpace::Srgb;
    let levels = [0.08_f32, 0.21, 0.53, 0.82];
    let mut expected = None;
    for white in [4095, 4096, 16384] {
        let values: Vec<f32> = levels.iter().flat_map(|v| [v * white as f32; 3]).collect();
        let mut description = Description::new([4, 1], Space::LinearRgb(space));
        description.linear_white = white;
        let image = RenderedImage {
            buffer: engine
                .gpu
                .upload("Synthetic linear codes", &values)
                .unwrap(),
            description,
            view: View {
                crop: [0, 0, 4, 1],
                rotation: 0,
                extent: [4, 1],
            },
            metadata: Arc::new(crate::source::Metadata {
                width: 4,
                height: 1,
                rotation: 0,
                processing_version: None,
                properties: Default::default(),
                lists: Default::default(),
                matrices: Default::default(),
            }),
            settings: Settings::default(),
            dehaze_basis: None,
            owner: engine.owner.clone(),
        };
        let scopes =
            crate::scopes::analyze(&engine.gpu, &image, None, &mut Profile::default()).unwrap();
        assert_eq!(scopes.linear_white, white);
        if let Some(expected) = &expected {
            assert_eq!(&scopes.counts, expected);
        } else {
            expected = Some(scopes.counts);
        }
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_reference_sharpening_bypass_matches_neutral_universal() {
    let source = Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let engine = Engine::new().unwrap();
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut settings = caps.editing_defaults(&source.metadata, rendering).unwrap();
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        settings.sharpen_amount = Some(1.3);
        settings.sharpening_enabled = false;
        let image = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let baseline = engine.readback_image(&image).unwrap();
        drop(image);
        settings.sharpening_enabled = true;
        settings.photographic.sharpening.method = PhotographicMethod::Universal;
        settings.photographic.sharpening.amount = 0.;
        let image = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(engine.readback_image(&image).unwrap(), baseline);
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_hsl_dng_reprojects_to_the_same_pre_curve_rgb() {
    check_chromatic_dng_reprojection(false, true);
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_perceptual_dng_reprojects_to_the_same_pre_curve_rgb() {
    check_chromatic_dng_reprojection(true, true);
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_unlocked_saturation_dng_reprojects_global_basic_and_advanced() {
    check_chromatic_dng_reprojection(false, false);
    check_chromatic_dng_reprojection(true, false);
}

fn check_chromatic_dng_reprojection(perceptual: bool, prevent: bool) {
    let source = Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = caps
        .editing_defaults(&source.metadata, Rendering::Color)
        .unwrap();
    if settings.white_balance.is_empty() {
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
    }
    if settings.color_mode.is_empty() {
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
    }
    settings.photographic.color_editor.master.hue = 0.3;
    settings.white_balance_bias = crate::render::WhiteBalanceBias {
        neutral: None,
        warmth: 0.35,
        tint: -0.2,
    };
    settings.photographic.color_editor.master.saturation = -0.2;
    settings.prevent_color_clipping = prevent;
    if !prevent {
        settings.photographic.color_editor.master.saturation = 2.;
        let mut range = crate::render::AdvancedColorRange {
            width: 360.,
            ..Default::default()
        };
        range.adjustment.saturation = 2.;
        settings.photographic.color_editor.advanced.push(range);
    }
    settings.photographic.color_editor.bands[0].lightness = 0.1;
    // Exercise Universal saturation independently of the Tone method.
    settings.photographic.tone_method = if perceptual {
        PhotographicMethod::Reference
    } else {
        PhotographicMethod::Universal
    };
    if perceptual {
        settings.photographic.saturation_method = crate::render::PhotographicMethod::Universal;
        settings.photographic.brightness = 0.2;
        settings.photographic.perceptual.saturation = if prevent { 0.3 } else { 2. };
        settings.photographic.perceptual.contrast = -0.4;
        settings.tonal_ranges.highlights = -0.5;
        settings.photographic.perceptual.bands[0].saturation = -0.2;
    }
    settings.tonal_ranges.shadows = 0.2;
    settings.photographic.local_contrast = 0.2;
    settings.photographic.sharpening.method = PhotographicMethod::Universal;
    settings.photographic.sharpening.amount = 0.3;
    let mut artifacts = engine
        .render_artifacts(
            &source,
            &settings,
            &["linear.edits"],
            &mut Profile::default(),
        )
        .unwrap();
    let Content::Image(reference) = artifacts.remove(0).content else {
        panic!("image")
    };
    let white = reference.description().linear_white as f64;
    let expected = engine.readback_image(&reference).unwrap();
    drop(reference);
    let result = engine
        .render_dng_base(&source, &settings, &mut Profile::default())
        .unwrap();
    let extent = result.description().extent;
    let actual = engine.readback_image(&result).unwrap();
    drop(result);
    let statistics =
        super::super::estimated_white_balance(&engine, &source, &settings, &mut Profile::default())
            .unwrap();
    let appearance =
        super::super::prepare_dng_appearance(&source.metadata, &settings, statistics).unwrap();
    let project = appearance
        .technical_from_native()
        .unwrap()
        .inverse()
        .unwrap();
    let expected: &[[f32; 3]] = bytemuck::cast_slice(&expected);
    let actual: &[[f32; 3]] = bytemuck::cast_slice(&actual);
    assert_eq!(expected.len(), actual.len());
    for i in (0..actual.len()).step_by(13) {
        let projected = project.apply(actual[i].map(f64::from));
        for c in 0..3 {
            let value = projected[c] * white;
            let target = f64::from(expected[i][c]);
            assert!(value.is_finite());
            assert!(
                (value - target).abs() < 0.02 + target.abs() * 0.00001,
                "pixel {i} c{c}: {value} vs {target}"
            );
        }
    }
    settings.preview_quality = PreviewQuality::Fast;
    settings.preview_region = Some([10, 10, 320, 240]);
    settings.output_size = Some([120, 80]);
    let native = engine
        .render_dng_base(&source, &settings, &mut Profile::default())
        .unwrap();
    assert_eq!(native.description().extent, extent);
    assert_eq!(
        engine.readback_image(&native).unwrap(),
        bytemuck::cast_slice::<[f32; 3], u8>(actual)
    );
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_color_editor_bypass_is_independent_in_photos_and_dng() {
    let source = Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = caps
        .editing_defaults(&source.metadata, Rendering::Color)
        .unwrap();
    if settings.white_balance.is_empty() {
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
    }
    if settings.color_mode.is_empty() {
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
    }
    settings.tone_enabled = false;
    settings.photographic.color_editor.master.hue = 0.4;
    settings.photographic.color_editor.master.saturation = -0.2;
    settings.photographic.color_editor.bands[0].lightness = 0.2;
    let authored = settings.photographic.color_editor;
    for dng in [false, true] {
        let render = |s: &Settings| {
            let mut profile = Profile::new(Mode::Wall);
            let image = if dng {
                engine.render_dng_base(&source, s, &mut profile)
            } else {
                engine.render(&source, s, &mut profile)
            }
            .unwrap();
            (engine.readback_image(&image).unwrap(), profile)
        };
        let mut neutral = settings.clone();
        neutral.photographic.color_editor = Default::default();
        let expected = render(&neutral).0;
        let mut disabled = settings.clone();
        disabled.color_editor_enabled = false;
        let (off, profile) = render(&disabled);
        assert_eq!(off, expected);
        assert!(!profile
            .host
            .iter()
            .any(|t| t.stage == "submit linear Color Editor"));
        assert_ne!(
            render(&settings).0,
            expected,
            "Tone bypass must leave Color Editor active"
        );
        let mut excluded = settings.clone();
        excluded.processing_sections.color_editor = false;
        assert_eq!(render(&excluded).0, expected);
        assert_eq!(disabled.photographic.color_editor, authored);
        assert_eq!(excluded.photographic.color_editor, authored);
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_white_balance_bias_keeps_technical_dng_samples_and_changes_editable_neutral() {
    for custom in [false, true] {
        check_white_balance_dng(custom);
    }
}

fn check_white_balance_dng(custom: bool) {
    let source = Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = caps
        .editing_defaults(&source.metadata, Rendering::Color)
        .unwrap();
    settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
    settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
    let original = engine
        .render_dng_base(&source, &settings, &mut Profile::default())
        .unwrap();
    let samples = engine.readback_image(&original).unwrap();
    drop(original);
    let stats =
        super::super::estimated_white_balance(&engine, &source, &settings, &mut Profile::default())
            .unwrap();
    let original =
        super::super::prepare_dng_appearance(&source.metadata, &settings, stats).unwrap();
    let baseline = engine
        .render_artifacts(
            &source,
            &settings,
            &["linear.edits"],
            &mut Profile::default(),
        )
        .unwrap()
        .remove(0);
    let Content::Image(baseline) = baseline.content else {
        panic!("image")
    };
    let baseline_space = match baseline.description().space {
        crate::image::Space::LinearRgb(space) => space,
        _ => panic!("linear RGB"),
    };
    let baseline_pixels = engine.readback_image(&baseline).unwrap();
    drop(baseline);
    settings.white_balance_bias = crate::render::WhiteBalanceBias {
        neutral: custom.then_some([1.1, 1., 0.7]),
        warmth: 0.35,
        tint: -0.2,
    };
    let biased = engine
        .render_dng_base(&source, &settings, &mut Profile::default())
        .unwrap();
    assert_eq!(engine.readback_image(&biased).unwrap(), samples);
    drop(biased);
    let stats =
        super::super::estimated_white_balance(&engine, &source, &settings, &mut Profile::default())
            .unwrap();
    let appearance =
        super::super::prepare_dng_appearance(&source.metadata, &settings, stats).unwrap();
    assert_ne!(appearance.neutral, original.neutral);
    assert_eq!(appearance.mapping.0, original.mapping.0);
    assert_eq!(appearance.forward.0, original.forward.0);
    let rendered = engine
        .render_artifacts(
            &source,
            &settings,
            &["linear.edits"],
            &mut Profile::default(),
        )
        .unwrap()
        .remove(0);
    let Content::Image(rendered) = rendered.content else {
        panic!("image")
    };
    let expected = engine.readback_image(&rendered).unwrap();
    let project = appearance
        .technical_from_native()
        .unwrap()
        .inverse()
        .unwrap();
    let technical: &[[f32; 3]] = bytemuck::cast_slice(&samples);
    let expected: &[[f32; 3]] = bytemuck::cast_slice(&expected);
    let baseline_pixels: &[[f32; 3]] = bytemuck::cast_slice(&baseline_pixels);
    let bias = crate::dng::appearance::Matrix(settings.white_balance_bias.matrix(baseline_space));
    let original_project = original.technical_from_native().unwrap().inverse().unwrap();
    assert_eq!(technical.len(), expected.len());
    for i in (0..technical.len()).step_by(97) {
        let actual = project.apply(technical[i].map(f64::from));
        let native = bias.apply(original_project.apply(technical[i].map(f64::from)));
        let filtered = bias.apply(baseline_pixels[i].map(f64::from));
        for c in 0..3 {
            assert!((actual[c] - native[c]).abs() < 1e-9 + native[c].abs() * 1e-10);
            let expected = f64::from(expected[i][c]);
            assert!(
                (filtered[c] - expected).abs() < 0.02 + expected.abs() * 0.00001,
                "{i} c{c}: {} vs {expected}",
                filtered[c]
            );
        }
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_advanced_color_builds_full_fast_and_regional_previews() {
    let source = Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = caps
        .editing_defaults(&source.metadata, Rendering::Color)
        .unwrap();
    settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
    settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
    settings
        .photographic
        .color_editor
        .advanced
        .push(crate::render::AdvancedColorRange {
            width: 360.,
            adjustment: crate::nodes::finish::color_editor::Adjustment {
                lightness: -0.3,
                ..Default::default()
            },
            ..Default::default()
        });
    let engine = Engine::new().unwrap();
    for (quality, region) in [
        (PreviewQuality::Full, None),
        (PreviewQuality::Fast, None),
        (PreviewQuality::Full, Some([10, 10, 200, 200])),
    ] {
        if quality == PreviewQuality::Fast && !settings.fast_preview_available(&source.metadata) {
            continue;
        }
        if region.is_some() && !settings.regional_preview_available(&source.metadata) {
            continue;
        }
        settings.preview_quality = quality;
        settings.preview_region = region;
        let image = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let bytes = engine.readback_image(&image).unwrap();
        let pixels: &[f32] = bytemuck::cast_slice(&bytes);
        assert!(!pixels.is_empty());
        assert!(
            pixels.iter().all(|v| v.is_finite()),
            "{quality:?} {region:?}"
        );
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE"]
fn original_auto_exposure_matches_manual_preview_cache_and_dng_appearance() {
    use crate::render::AutoExposureCompensation::{Both, Bracket, Dial};
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap());
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let compensation =
        crate::source::CaptureExposureCompensation::from_properties(&source.metadata.properties);
    for (rendering, method, choice) in [
        (Rendering::Color, PhotographicMethod::Reference, Both),
        (Rendering::Color, PhotographicMethod::Universal, Bracket),
        (Rendering::Monochrome, PhotographicMethod::Reference, Dial),
        (Rendering::Monochrome, PhotographicMethod::Universal, Both),
    ] {
        let mut manual = caps.editing_defaults(&source.metadata, rendering).unwrap();
        manual.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        manual.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        manual.photographic.tone_method = method;
        if method == PhotographicMethod::Universal {
            manual.photographic.base_response = crate::render::BaseResponse::Natural;
        }
        manual.exposure = choice.inverse(compensation).unwrap();
        manual.output_size = Some([320, 213]);
        let baseline = engine
            .render(&source, &manual, &mut Profile::default())
            .unwrap();
        let baseline = engine.readback_image(&baseline).unwrap();
        let mut auto = manual.clone();
        auto.exposure = 2.75;
        auto.exposure_auto = true;
        auto.exposure_auto_compensation = choice;
        let mut session = RenderSession::new(source.clone());
        for _ in 0..2 {
            let mut images = engine
                .render_session(
                    &mut session,
                    &auto,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap();
            let Content::Image(image) = images.remove(0).content else {
                panic!("photo")
            };
            assert_eq!(
                baseline,
                engine.readback_image(&image).unwrap(),
                "Auto/manual preview mismatch {rendering:?} {method:?} {choice:?}"
            );
            assert!(image.settings.exposure_auto);
            assert_eq!(image.settings.exposure, 2.75);
        }
        let expected = engine
            .render_dng_base(&source, &manual, &mut Profile::default())
            .unwrap();
        let expected = engine.readback_image(&expected).unwrap();
        let actual = engine
            .render_dng_base(&source, &auto, &mut Profile::default())
            .unwrap();
        assert_eq!(
            expected,
            engine.readback_image(&actual).unwrap(),
            "Auto/manual DNG samples mismatch"
        );
        let statistics = super::super::estimated_white_balance(
            &engine,
            &source,
            &manual,
            &mut Profile::default(),
        )
        .unwrap();
        let expected = super::super::prepare_dng_appearance(&source.metadata, &manual, statistics)
            .unwrap()
            .finish(1.)
            .unwrap();
        let actual = super::super::prepare_dng_appearance(&source.metadata, &auto, statistics)
            .unwrap()
            .finish(1.)
            .unwrap();
        assert_eq!(expected.baseline, actual.baseline);
        assert_eq!(expected.curve, actual.curve);
        assert_eq!(expected.look, actual.look);
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE pointing to a supported X3F"]
fn original_adaptive_tone_shares_the_architecture_analysis_grid_across_previews() {
    use crate::{
        graph::Operation,
        interactive::Execution,
        nodes::finish::adaptive_tone::{apply_with_analysis, Samples},
        render::BaseResponse,
    };
    let source = Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut settings = caps.editing_defaults(&source.metadata, rendering).unwrap();
        settings.photographic.tone_method = PhotographicMethod::Universal;
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        for response in [BaseResponse::Natural, BaseResponse::Strong] {
            settings.photographic.base_response = response;
            let qualities = if settings.fast_preview_available(&source.metadata) {
                vec![PreviewQuality::Full, PreviewQuality::Fast]
            } else {
                vec![PreviewQuality::Full]
            };
            let mut expected_analysis = None;
            let mut expected_fit = None;
            let mut full_extent = None;
            for quality in qualities {
                settings.preview_quality = quality;
                let mut profile = Profile::default();
                let mut execution = Execution::default();
                let built = super::super::build_camera(
                    &engine,
                    &source,
                    &settings,
                    &["final"],
                    &mut profile,
                    &mut execution,
                )
                .unwrap();
                let (params, analysis_params, curve) = built
                    .graph
                    .nodes
                    .iter()
                    .find_map(|n| match &n.operation {
                        Operation::AdaptiveTone {
                            params,
                            analysis,
                            curve,
                            ..
                        } => Some((*params, *analysis, curve.clone())),
                        _ => None,
                    })
                    .unwrap();
                let artifacts = engine
                    .collect_artifacts(
                        &source,
                        &settings,
                        &built.graph,
                        built.final_id,
                        &[
                            "linear.edits-input",
                            "base-response.mask",
                            "base-response.analysis",
                            "base-response.analysis-mask",
                            "base-response.rgb",
                        ],
                        built.params,
                        &mut profile,
                        &mut execution,
                    )
                    .unwrap();
                let images: Vec<_> = artifacts
                    .into_iter()
                    .map(|a| match a.content {
                        Content::Image(i) => i,
                        _ => panic!("expected image"),
                    })
                    .collect();
                let [input, mask, analysis_rgb, analysis_mask, actual] = images.as_slice() else {
                    panic!("missing inputs")
                };
                if quality == PreviewQuality::Full {
                    if caps.family == crate::source::ProcessingFamily::Classic {
                        assert_eq!(analysis_rgb.description.extent, input.description.extent);
                    } else {
                        for (small, large) in analysis_rgb
                            .description
                            .extent
                            .into_iter()
                            .zip(input.description.extent)
                        {
                            assert!((small as f32 / large as f32 - 0.5).abs() < 0.01);
                        }
                    }
                }
                let gpu = engine.test_gpu();
                let bytes = gpu.readback(&analysis_rgb.buffer).unwrap();
                let guide = gpu.readback(&analysis_mask.buffer).unwrap();
                if let Some((rgb, mask, key)) = &expected_analysis {
                    assert_eq!(
                        &bytes, rgb,
                        "{response:?} {quality:?} changed analysis pixels"
                    );
                    assert_eq!(
                        &guide, mask,
                        "{response:?} {quality:?} changed analysis guide"
                    );
                    assert!(
                        analysis_params.key() == *key,
                        "preview quality changed fit parameters"
                    );
                } else {
                    expected_analysis = Some((bytes, guide, analysis_params.key()));
                }
                if let Some(native) = full_extent {
                    for (small, large) in analysis_rgb.description.extent.into_iter().zip(native) {
                        assert!(
                        (small as f32 / large as f32 - 0.5).abs() < 0.01,
                        "analysis must use quarter pixels: analysis {:?}, main {:?}, baseline {:?}, quality {:?}", analysis_rgb.description.extent, input.description.extent, native, quality
                    );
                    }
                } else {
                    full_extent = Some(input.description.extent);
                }
                let replay = apply_with_analysis(
                    gpu,
                    Samples {
                        rgb: &input.buffer,
                        mask: &mask.buffer,
                    },
                    Samples {
                        rgb: &analysis_rgb.buffer,
                        mask: &analysis_mask.buffer,
                    },
                    &curve,
                    params,
                    analysis_params,
                    &mut Profile::default(),
                )
                .unwrap();
                assert_eq!(
                    gpu.readback(&replay.rgb).unwrap(),
                    gpu.readback(&actual.buffer).unwrap()
                );
                let fit = gpu.readback(&replay.amounts).unwrap();
                if let Some(expected) = &expected_fit {
                    assert_eq!(
                        &fit, expected,
                        "{response:?} {quality:?} changed the fitted response"
                    );
                } else {
                    expected_fit = Some(fit);
                }
                assert!(
                    bytemuck::cast_slice::<u8, f32>(&gpu.readback(&actual.buffer).unwrap())
                        .iter()
                        .all(|v| v.is_finite())
                );
            }
        }
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR containing example photographs"]
fn original_reference_saturation_policy_changes_all_families_and_invalidates_cached_output() {
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::new().unwrap();
    for name in ["sunlit-red-flower.x3f", "sd15.x3f", "dp3-quattro.x3f"] {
        let source = Arc::new(Source::open(root.join(name)).unwrap());
        let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
        let mut settings = caps
            .editing_defaults(&source.metadata, Rendering::Color)
            .unwrap();
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        settings.photographic.saturation_method = PhotographicMethod::Reference;
        settings.saturation = 2.;
        settings.output_size = Some([400, 400]);
        let mut session = RenderSession::new(source.clone());
        let mut outputs = Vec::new();
        for protect in [false, true, false] {
            settings.prevent_color_clipping = protect;
            let mut artifacts = engine
                .render_session(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap();
            let Content::Image(image) = artifacts.pop().unwrap().content else {
                panic!("image")
            };
            outputs.push(engine.readback_image(&image).unwrap());
        }
        assert_ne!(
            outputs[0], outputs[1],
            "{name}: protected Reference must change vivid colors"
        );
        assert_eq!(
            outputs[0], outputs[2],
            "{name}: disabling protection must restore native saturation"
        );
        settings.prevent_color_clipping = true;
        let fresh = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(
            engine.readback_image(&fresh).unwrap(),
            outputs[1],
            "{name}: cached and fresh policy must agree"
        );
        let protected_profile =
            crate::render::dng_appearance(&source.metadata, &settings, Default::default())
                .unwrap()
                .finish(1.)
                .unwrap();
        settings.prevent_color_clipping = false;
        let unprotected_profile =
            crate::render::dng_appearance(&source.metadata, &settings, Default::default())
                .unwrap()
                .finish(1.)
                .unwrap();
        assert_ne!(
            protected_profile.look, unprotected_profile.look,
            "{name}: DNG appearance must include saturation protection"
        );
    }
}
