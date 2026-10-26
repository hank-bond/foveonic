//! Checks experimental tonal edits through fresh, cached, reduced and regional Merrill recipes.

use crate::{
    image::RenderedImage,
    inspection::Content,
    nodes::sensor::calibration,
    profile::{Mode, Profile},
    render::{Engine, PreviewQuality, RenderSession, Settings},
    source::Source,
};
use std::sync::Arc;

#[test]
fn regional_tonal_support_shares_the_native_guide_grid_at_each_rotation() {
    for rotation in [0, 90, 180, 270] {
        let encode = calibration::EncodeParams {
            source: [4928, 3264, rotation, 0],
            crop: [112, 64, 4816, 3200],
            output: if rotation % 180 == 0 {
                [4704, 3136, 0, 0]
            } else {
                [3136, 4704, 0, 0]
            },
        };
        for rect in [[0, 0, 400, 300], [600, 800, 1100, 1300]] {
            let ordinary = super::region::Prepared::new(encode, rect).unwrap();
            let tonal = super::region::Prepared::with_tonal_support(encode, rect).unwrap();
            for axis in 0..2 {
                assert!(tonal.physical[axis]
                    .is_multiple_of(crate::nodes::finish::tone_equalizer::GUIDE_ALIGNMENT));
                assert!(tonal.physical[axis] <= ordinary.physical[axis]);
                assert!(tonal.physical[axis + 2] >= ordinary.physical[axis + 2]);
                assert!(tonal.physical[axis + 2] <= encode.source[axis]);
            }
            assert_eq!(tonal.encode.output, ordinary.encode.output);
        }
    }
}

fn session_image(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
    profile: &mut Profile,
) -> RenderedImage {
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
#[ignore = "requires GPU and FOVEONIC_TONAL_SOURCE pointing to a Merrill X3F"]
fn gpu_tonal_ranges_match_cached_reduced_and_regional_rendering() {
    let source =
        Arc::new(Source::open(std::env::var_os("FOVEONIC_TONAL_SOURCE").unwrap()).unwrap());
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.photographic.tone_method = crate::render::PhotographicMethod::Universal;
    settings.reconstruction = Some(Default::default());
    let mut session = RenderSession::new(source.clone());
    let baseline = session_image(&engine, &mut session, &settings, &mut Profile::default());
    let baseline_bytes = engine.readback_image(&baseline).unwrap();
    let neutral = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    assert!(engine.readback_image(&neutral).unwrap() == baseline_bytes);
    {
        settings.tonal_ranges.shadows = 0.5;
        settings.tonal_ranges.highlights = -0.5;
        let _ = session_image(&engine, &mut session, &settings, &mut Profile::default());
        settings.tonal_ranges.shadows = 0.6;
        let mut edit_profile = Profile::new(Mode::Wall);
        let cached = session_image(&engine, &mut session, &settings, &mut edit_profile);
        assert!(
            !edit_profile.host.iter().any(|t| matches!(
                t.stage,
                "submit tonal brightness mask"
                    | "submit reference noise base"
                    | "submit calibration"
            )),
            "cached bytes {}: {:?}",
            session.cached_bytes(),
            edit_profile.host
        );
        let fresh = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let full_bytes = engine.readback_image(&fresh).unwrap();
        assert!(
            engine.readback_image(&cached).unwrap() == full_bytes,
            "cached HDR"
        );
        let rect = [600, 800, 1100, 1300];
        settings.preview_region = Some(rect);
        let regional = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let encode = calibration::encode(
            &source.metadata,
            crate::color_space::ColorSpace::from_metadata(&source.metadata).unwrap(),
        )
        .unwrap();
        let geometry = super::region::Prepared::with_tonal_support(encode, rect).unwrap();
        let bytes = engine.readback_image(&regional).unwrap();
        let local: &[f32] = bytemuck::cast_slice(&bytes);
        let full: &[f32] = bytemuck::cast_slice(&full_bytes);
        let [x0, y0, x1, y1] = regional.view().crop;
        let mut maximum = 0_f32;
        for y in y0..y1 {
            for x in x0..x1 {
                let dst = ((y * regional.description().extent[0] + x) * 3) as usize;
                let src = (((y + geometry.physical[1]) * fresh.description().extent[0]
                    + x
                    + geometry.physical[0])
                    * 3) as usize;
                for c in 0..3 {
                    maximum = maximum.max((local[dst + c] - full[src + c]).abs());
                }
            }
        }
        assert!(maximum < 0.00001, "regional HDR: {maximum}");
        settings.preview_region = None;
        let mut exposure_profile = Profile::new(Mode::Wall);
        settings.exposure = 0.5;
        let _ = session_image(&engine, &mut session, &settings, &mut exposure_profile);
        assert!(!exposure_profile
            .host
            .iter()
            .any(|t| t.stage == "submit tonal brightness mask"));
        settings.exposure = 0.;
        settings.preview_quality = PreviewQuality::Fast;
        let reduced = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(reduced.view().extent, baseline.view().extent.map(|v| v / 2));
        let cached = session_image(&engine, &mut session, &settings, &mut Profile::default());
        assert!(
            engine.readback_image(&cached).unwrap() == engine.readback_image(&reduced).unwrap()
        );
        settings.preview_quality = PreviewQuality::Full;
        settings.tonal_ranges.shadows = 0.;
        settings.tonal_ranges.highlights = 0.;
    }
    assert!(engine.readback_image(&baseline).unwrap() == baseline_bytes);
}

#[test]
#[ignore = "requires GPU and FOVEONIC_TONAL_SOURCE pointing to a Merrill X3F"]
fn gpu_tonal_ranges_keep_monochrome_neutral_and_expose_the_mask() {
    let source = Source::open(std::env::var_os("FOVEONIC_TONAL_SOURCE").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.photographic.tone_method = crate::render::PhotographicMethod::Universal;
    settings.rendering = crate::render::Rendering::Monochrome;
    let baseline = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    let expected = engine.readback_image(&baseline).unwrap();
    settings.tonal_ranges.shadows = 0.5;
    let artifacts = engine
        .render_artifacts(
            &source,
            &settings,
            &["tonal-ranges.mask", "tonal-ranges.rgb", "tone.rgb"],
            &mut Profile::default(),
        )
        .unwrap();
    for artifact in artifacts {
        let Content::Image(image) = artifact.content else {
            panic!("expected image")
        };
        let bytes = engine.readback_image(&image).unwrap();
        let pixels: &[f32] = bytemuck::cast_slice(&bytes);
        assert!(pixels.iter().all(|v| v.is_finite()));
        if artifact.name == "tonal-ranges.mask" {
            assert_eq!(
                image.description().layout,
                crate::image::Layout::Scalar32Float
            );
        } else {
            assert!(pixels
                .as_chunks::<3>()
                .0
                .iter()
                .all(|p| p[0] == p[1] && p[1] == p[2]));
        }
    }
    settings.tone_enabled = false;
    let disabled = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    assert!(engine.readback_image(&disabled).unwrap() == expected);
    assert_eq!(settings.tonal_ranges.shadows, 0.5);
}
