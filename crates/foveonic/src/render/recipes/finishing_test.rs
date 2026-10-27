//! Checks shared editing across original camera families, caches and export boundaries.
use super::*;
use crate::render::Engine;
use crate::{
    inspection::Content,
    profile::Profile,
    render::{ControlCapabilities, PhotographicMethod, PreviewQuality, RenderSession, Rendering},
    source::{ProcessingFamily, Source},
};
use std::sync::Arc;

#[test]
fn tone_selection_bypasses_inactive_controls_without_overwriting_them() {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.tone.shadows = 0.4;
    settings.tone.highlights = -0.3;
    settings.tonal_ranges.shadows = 0.6;
    settings.photographic.local_contrast = 0.2;
    for method in [Method::Reference, Method::Universal, Method::Reference] {
        settings.photographic.tone_method = method;
        let saved = settings.clone();
        let effective = settings.active_tone_settings();
        if method == Method::Reference {
            assert!(effective.tonal_ranges.is_neutral());
            assert_eq!(effective.photographic.local_contrast, 0.);
            assert_eq!(effective.tone, settings.tone);
        } else {
            assert_eq!(effective.tone.shadows, 0.);
            assert_eq!(effective.tone.highlights, 0.);
            assert_eq!(effective.tonal_ranges, settings.tonal_ranges);
            assert_eq!(effective.photographic.local_contrast, 0.2);
        }
        assert_eq!(settings, saved);
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE pointing to a supported X3F"]
fn gpu_tone_method_ignores_inactive_edits_in_the_rendered_result() {
    let source = Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut settings = caps.editing_defaults(&source.metadata, rendering).unwrap();
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        for method in [Method::Reference, Method::Universal] {
            settings.photographic.tone_method = method;
            let baseline = engine
                .render(&source, &settings, &mut Profile::default())
                .unwrap();
            let mut hidden = settings.clone();
            if method == Method::Reference {
                hidden.tonal_ranges.shadows = 0.7;
                hidden.photographic.local_contrast = 0.8;
            } else {
                hidden.tone.shadows = 0.7;
                hidden.tone.highlights = -0.6;
            }
            let image = engine
                .render(&source, &hidden, &mut Profile::default())
                .unwrap();
            assert!(
                engine.readback_image(&baseline).unwrap() == engine.readback_image(&image).unwrap(),
                "hidden controls affected {rendering:?} {method:?}"
            );
        }
    }
}

fn session_image(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
    profile: &mut Profile,
) -> crate::image::RenderedImage {
    let mut artifacts = engine
        .render_session(session, settings, &["final"], || false, profile)
        .unwrap()
        .unwrap();
    let Content::Image(image) = artifacts.remove(0).content else {
        panic!("expected image")
    };
    *image
}
#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE pointing to a supported X3F"]
fn gpu_shared_finishing_original_color_mono_cache_and_dng() {
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap());
    let engine = Engine::new().unwrap();
    let capabilities = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut settings = capabilities
            .editing_defaults(&source.metadata, rendering)
            .unwrap();
        if settings.white_balance.is_empty() {
            settings.white_balance = capabilities
                .default_white_balance(&source.metadata)
                .unwrap();
        }
        if settings.color_mode.is_empty() {
            settings.color_mode = capabilities.default_color_mode(&source.metadata).unwrap();
        }
        settings.rendering = rendering;
        settings.validate_for_camera(&source.metadata).unwrap();
        let reference = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let baseline = engine.readback_image(&reference).unwrap();
        drop(reference);
        // Universal neutral tone retains the camera appearance exactly.
        let mut neutral = settings.clone();
        neutral.photographic.tone_method = PhotographicMethod::Universal;
        let image = engine
            .render(&source, &neutral, &mut Profile::default())
            .unwrap();
        assert!(
            engine.readback_image(&image).unwrap() == baseline,
            "neutral universal tone"
        );
        drop(image);
        drop(baseline);
        let dng = engine
            .render_dng_base(&source, &settings, &mut Profile::default())
            .unwrap();
        let dng_bytes = engine.readback_image(&dng).unwrap();
        drop(dng);
        settings.photographic.tone_method = PhotographicMethod::Universal;
        settings.tone.contrast = 0.4;
        settings.saturation = 1.3;
        settings.tonal_ranges.shadows = 0.3;
        settings.tonal_ranges.highlights = -0.25;
        settings.photographic.local_contrast = 0.3;
        settings.photographic.sharpening.method = PhotographicMethod::Universal;
        settings.photographic.sharpening.amount = 0.6;
        if rendering == Rendering::Monochrome {
            settings.photographic.color_editor =
                crate::render::ColorEditorControls::from_lightness([0.3, 0.1, -0.2, 0., -0.4, 0.]);
        }
        settings.validate_for_camera(&source.metadata).unwrap();
        let mut session = RenderSession::new(source.clone());
        let image = session_image(&engine, &mut session, &settings, &mut Profile::default());
        let first = engine.readback_image(&image).unwrap();
        drop(image);
        assert!(bytemuck::cast_slice::<u8, f32>(&first)
            .iter()
            .all(|v| v.is_finite()));
        let fresh = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert!(
            engine.readback_image(&fresh).unwrap() == first,
            "initial cache/fresh"
        );
        drop(fresh);
        drop(first);
        settings.photographic.sharpening.amount = 0.8;
        let image = session_image(&engine, &mut session, &settings, &mut Profile::default());
        let cached = engine.readback_image(&image).unwrap();
        drop(image);
        let fresh = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert!(
            engine.readback_image(&fresh).unwrap() == cached,
            "edited cache/fresh"
        );
        drop(fresh);
        drop(cached);
        if rendering == Rendering::Monochrome {
            settings.photographic.color_editor.bands[0].lightness = 0.5;
            let mut profile = Profile::new(crate::profile::Mode::Wall);
            let image = session_image(&engine, &mut session, &settings, &mut profile);
            drop(image);
            assert!(
                !profile.host.iter().any(|t| t.stage == "submit calibration"),
                "guide should be cached: {:?}",
                profile.host
            );
        }
        // Neutral spatial edits preserve the established technical DNG base.
        let ordinary = capabilities
            .editing_defaults(&source.metadata, rendering)
            .unwrap();
        settings.tone = ordinary.tone;
        settings.saturation = ordinary.saturation;
        settings.tonal_ranges = ordinary.tonal_ranges;
        settings.photographic.color_editor = Default::default();
        settings.photographic.local_contrast = 0.;
        settings.photographic.sharpening = ordinary.photographic.sharpening;
        let dng = engine
            .render_dng_base(&source, &settings, &mut Profile::default())
            .unwrap();
        assert!(
            engine.readback_image(&dng).unwrap() == dng_bytes,
            "DNG boundary"
        );
        drop(dng);
        if rendering == Rendering::Color && settings.fast_preview_available(&source.metadata) {
            settings.preview_quality = PreviewQuality::Fast;
            let image = engine
                .render(&source, &settings, &mut Profile::default())
                .unwrap();
            assert!(engine.readback_image(&image).is_ok());
        }
    }
}

fn view_pixel(image: &crate::image::RenderedImage, bytes: &[u8], x: u32, y: u32) -> [f32; 3] {
    let view = image.view();
    let [l, t, r, b] = view.crop;
    let [x, y] = match view.rotation {
        0 => [l + x, t + y],
        90 => [l + y, b - 1 - x],
        180 => [r - 1 - x, b - 1 - y],
        270 => [r - 1 - y, t + x],
        _ => panic!("rotation"),
    };
    let offset = ((y * image.description().extent[0] + x) * 3) as usize;
    bytemuck::cast_slice::<u8, f32>(bytes)[offset..offset + 3]
        .try_into()
        .unwrap()
}
#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE pointing to a regional-capable X3F"]
fn gpu_shared_spatial_finishing_matches_full_and_regional_views() {
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap());
    let engine = Engine::new().unwrap();
    let capabilities = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = capabilities
        .editing_defaults(&source.metadata, Rendering::Color)
        .unwrap();
    if settings.white_balance.is_empty() {
        settings.white_balance = capabilities
            .default_white_balance(&source.metadata)
            .unwrap();
    }
    if settings.color_mode.is_empty() {
        settings.color_mode = capabilities.default_color_mode(&source.metadata).unwrap();
    }
    settings.photographic.color_editor =
        crate::render::ColorEditorControls::from_lightness([0.2, 0., -0.1, 0., 0.15, 0.]);
    settings.photographic.tone_method = PhotographicMethod::Universal;
    settings.photographic.color_editor.master.hue = 0.3;
    settings.photographic.color_editor.master.saturation = -0.2;
    settings.photographic.local_contrast = 0.4;
    settings.photographic.sharpening.method = PhotographicMethod::Universal;
    settings.photographic.sharpening.amount = 0.8;
    settings.tone.contrast = 0.4;
    settings.saturation = 1.2;
    settings.tonal_ranges.shadows = 0.4;
    settings.tonal_ranges.highlights = -0.3;
    let full = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    let full_bytes = engine.readback_image(&full).unwrap();
    let [w, h] = full.view().extent;
    let mut session = RenderSession::new(source.clone());
    settings.preview_quality = PreviewQuality::Draft;
    let overview = session_image(&engine, &mut session, &settings, &mut Profile::default());
    engine
        .view_texture(&overview, None, &mut Profile::default())
        .unwrap();
    drop(overview);
    settings.preview_quality = PreviewQuality::Full;
    for (index, [x, y]) in [[0, 0], [w / 2, h / 2], [w - 320, h - 240]]
        .into_iter()
        .enumerate()
    {
        let rect = [x, y, x + 320, y + 240];
        settings.preview_region = Some(rect);
        let region = session_image(&engine, &mut session, &settings, &mut Profile::default());
        assert_eq!(region.view().extent, [320, 240]);
        let bytes = engine.readback_image(&region).unwrap();
        let mut maximum = 0_f32;
        let mut sum = 0_f64;
        let mut count = 0;
        for yy in 0..240 {
            for xx in 0..320 {
                for (a, b) in view_pixel(&region, &bytes, xx, yy)
                    .into_iter()
                    .zip(view_pixel(&full, &full_bytes, x + xx, y + yy))
                {
                    let error = (a - b).abs();
                    maximum = maximum.max(error);
                    sum += f64::from(error);
                    count += 1;
                }
            }
        }
        eprintln!(
            "shared region {x},{y}: max {maximum}, mean {}",
            sum / count as f64
        );
        assert!(
            maximum < 0.0001,
            "regional mismatch at {x},{y}: max {maximum}, mean {}",
            sum / count as f64
        );
        if index == 0 {
            settings.preview_region = None;
            let mut profile = Profile::new(crate::profile::Mode::Wall);
            let continuation = session_image(&engine, &mut session, &settings, &mut profile);
            assert!(
                engine.readback_image(&continuation).unwrap() == full_bytes,
                "Smart whole-frame continuation"
            );
            assert!(
                !profile
                    .host
                    .iter()
                    .any(|t| matches!(t.stage, "submit calibration" | "extract sensor phases")),
                "continuation repeated sensor work: {:?}",
                profile.host
            );
        }
    }
    if source.metadata.processing_family().unwrap() == ProcessingFamily::Merrill {
        settings.preview_region = None;
        settings.tonal_ranges = Default::default();
        settings.photographic.local_contrast = 0.;
        let full = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let bytes = engine.readback_image(&full).unwrap();
        settings.preview_region = Some([w / 2, h / 2, w / 2 + 320, h / 2 + 240]);
        let region = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let small = engine.readback_image(&region).unwrap();
        assert!(
            region.description().extent[0] < 640 && region.description().extent[1] < 640,
            "sharpen-only viewport reserved excessive support: {:?}",
            region.description().extent
        );
        for y in 0..240 {
            for x in 0..320 {
                assert_eq!(
                    view_pixel(&region, &small, x, y),
                    view_pixel(&full, &bytes, w / 2 + x, h / 2 + y)
                );
            }
        }
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FINISH_SOURCE pointing to a Merrill X3F"]
fn gpu_merrill_color_sensitivity_matches_baked_dng_and_keeps_guide_independent() {
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_FINISH_SOURCE").unwrap()).unwrap());
    let engine = Engine::new().unwrap();
    let caps = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut canonical_guide = None;
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        let mut settings = caps.editing_defaults(&source.metadata, rendering).unwrap();
        settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
        settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
        let plain = engine
            .render_dng_base(&source, &settings, &mut Profile::default())
            .unwrap();
        let plain_bytes = engine.readback_image(&plain).unwrap();
        drop(plain);
        settings.photographic.color_editor =
            crate::render::ColorEditorControls::from_lightness([0.3, -0.1, 0.1, 0., -0.2, 0.1]);
        let names = [
            "color-editor.input",
            "color-editor.rgb",
            "color-editor.guide",
        ];
        let mut artifacts = engine
            .render_artifacts(&source, &settings, &names, &mut Profile::default())
            .unwrap();
        let mut buffers = Vec::new();
        for artifact in artifacts.drain(..) {
            let Content::Image(image) = artifact.content else {
                panic!("image")
            };
            buffers.push(engine.readback_image(&image).unwrap());
        }
        if let Some(guide) = &canonical_guide {
            assert!(
                *guide == buffers[2],
                "same classifier in both rendering modes"
            );
        } else {
            canonical_guide = Some(buffers[2].clone());
        }
        let before: &[f32] = bytemuck::cast_slice(&buffers[0]);
        let after: &[f32] = bytemuck::cast_slice(&buffers[1]);
        let baked = engine
            .render_dng_base(&source, &settings, &mut Profile::default())
            .unwrap();
        let baked_bytes = engine.readback_image(&baked).unwrap();
        drop(baked);
        assert!(
            plain_bytes != baked_bytes,
            "DNG must bake active sensitivity"
        );
        let plain: &[f32] = bytemuck::cast_slice(&plain_bytes);
        let baked: &[f32] = bytemuck::cast_slice(&baked_bytes);
        for i in (0..plain.len()).step_by(37) {
            if before[i].abs() > 0.1 {
                let expected = plain[i] * (after[i] / before[i]);
                assert!(
                    (baked[i] - expected).abs() < 0.02 + expected.abs() * 0.000001,
                    "preview/DNG gain at {i}: {} vs {expected}",
                    baked[i]
                );
            }
        }
        let mut session = RenderSession::new(source.clone());
        let _ = session_image(&engine, &mut session, &settings, &mut Profile::default());
        settings.photographic.color_editor.bands[0].lightness = 0.5;
        let mut profile = Profile::new(crate::profile::Mode::Wall);
        let cached = session_image(&engine, &mut session, &settings, &mut profile);
        assert!(
            !profile.host.iter().any(|t| matches!(
                t.stage,
                "submit calibration" | "submit reference noise base"
            )),
            "sensitivity edit rebuilt its inputs: {:?}",
            profile.host
        );
        let fresh = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert!(engine.readback_image(&cached).unwrap() == engine.readback_image(&fresh).unwrap());
        drop(cached);
        drop(fresh);
        settings.tone_enabled = false;
        let bypass = engine
            .render_dng_base(&source, &settings, &mut Profile::default())
            .unwrap();
        assert!(
            plain_bytes == engine.readback_image(&bypass).unwrap(),
            "section bypass excludes sensitivity from DNG"
        );
        if rendering == Rendering::Color {
            settings.tone_enabled = true;
            for quality in [PreviewQuality::Fast, PreviewQuality::Draft] {
                settings.preview_quality = quality;
                let image = engine
                    .render(&source, &settings, &mut Profile::default())
                    .unwrap();
                assert!(
                    bytemuck::cast_slice::<u8, f32>(&engine.readback_image(&image).unwrap())
                        .iter()
                        .all(|v| v.is_finite())
                );
                let dng = engine
                    .render_dng_base(&source, &settings, &mut Profile::default())
                    .unwrap();
                assert_eq!(
                    dng.description().extent,
                    [source.metadata.width as u32, source.metadata.height as u32]
                );
            }
        }
    }
}
