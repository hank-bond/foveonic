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
        s.photographic.color_sensitivity = [0.06, 0.87, 0., 0., 0., 0.];
        s.tonal_ranges = crate::render::TonalRanges {
            blacks: 0.22,
            shadows: -0.5,
            highlights: -1.,
            whites: -0.63,
        };
        s.photographic.sharpening.method = PhotographicMethod::Universal;
        s.photographic.sharpening.amount = 0.;
        let mut neutral = s.clone();
        neutral.tonal_ranges = Default::default();
        neutral.photographic.local_contrast = 0.;
        neutral.photographic.color_sensitivity = [0.; 6];
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
        s.preview_quality = PreviewQuality::Draft;
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
        settings.photographic.color_sensitivity[0] = 0.4;
        settings.tonal_ranges.shadows = 0.3;
        settings.photographic.local_contrast = 0.2;
        settings.photographic.sharpening.method = PhotographicMethod::Universal;
        settings.photographic.sharpening.amount = 0.6;
        let authored = serde_json::to_vec(&settings).unwrap();
        let mut explicit = settings.clone();
        explicit.tone_enabled = false;
        explicit.sharpening_enabled = false;
        let expected_image = engine
            .render_dng_base(&source, &explicit, &mut Profile::default())
            .unwrap();
        let native_extent = expected_image.view().extent;
        let expected = engine.readback_image(&expected_image).unwrap();
        drop(expected_image);
        settings.processing_sections.tone = false;
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
