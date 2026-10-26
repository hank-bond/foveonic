//! Checks reduced framing, optional reconstruction bypass, and immutable Full refinement.

use super::*;

fn encoding(rotation: u32) -> EncodeParams {
    let mut output = [4704, 3136, 0, 0];
    if matches!(rotation, 90 | 270) {
        output.swap(0, 1);
    }
    EncodeParams {
        source: [4928, 3264, rotation, 0],
        crop: [112, 64, 4816, 3200],
        output,
    }
}

#[test]
fn half_grid_preserves_crop_orientation_and_explicit_output_size() {
    for rotation in [0, 90, 180, 270] {
        let native = encoding(rotation);
        let grid = Grid::new(native, None).unwrap();
        assert_eq!(grid.reduction.output_extent(), [2464, 1632]);
        assert_eq!(grid.encode.source, [2464, 1632, rotation, 0]);
        assert_eq!(grid.encode.crop, native.crop.map(|v| v / 2));
        assert_eq!(
            grid.encode.output[..2],
            native.output[..2].iter().map(|v| v / 2).collect::<Vec<_>>()
        );
        let sized = Grid::new(native, Some([1200, 800])).unwrap();
        assert_eq!(sized.encode.output[..2], [1200, 800]);
    }
    let mut odd = encoding(0);
    odd.crop[0] += 1;
    assert!(Grid::new(odd, None).is_err());
    let mut misaligned = encoding(0);
    misaligned.source[0] -= 1;
    assert!(Grid::new(misaligned, None).is_err());
    misaligned.source[0] -= 1;
    assert!(Grid::new(misaligned, None).is_ok());
}

#[test]
fn color_preview_support_requires_valid_merrill_geometry_and_supports_native_regions() {
    use crate::source::{Matrix, MatrixEncoding, Metadata};
    let mut metadata = Metadata {
        width: 4928,
        height: 3264,
        rotation: 0,
        processing_version: None,
        properties: [("CAMMODEL".into(), "SIGMA DP2 Merrill".into())].into(),
        matrices: [(
            "ActiveImageArea".into(),
            Matrix {
                encoding: MatrixEncoding::Uint32,
                representation: "test",
                dimensions: vec![4],
                values: vec![112., 64., 4815., 3199.],
            },
        )]
        .into(),
        lists: Default::default(),
    };
    let mut settings = crate::render::Settings::default();
    for rotation in [0, 90, 180, 270] {
        metadata.rotation = rotation;
        assert!(settings.fast_preview_available(&metadata));
        assert!(settings.regional_preview_available(&metadata));
    }
    settings.rendering = crate::render::Rendering::Monochrome;
    assert!(!settings.fast_preview_available(&metadata));
    settings.rendering = crate::render::Rendering::Color;
    settings.output_size = Some([1200, 800]);
    assert!(!settings.regional_preview_available(&metadata));
    settings.output_size = None;
    metadata.matrices.get_mut("ActiveImageArea").unwrap().values[0] += 1.;
    assert!(!settings.fast_preview_available(&metadata));
    metadata.matrices.clear();
    assert!(!settings.fast_preview_available(&metadata));
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn reduced_render_matches_cached_edits_and_keeps_full_images_and_inspection_native() {
    use crate::{
        image::RenderedImage,
        inspection::Content,
        profile::{Mode, Profile},
        render::{Engine, PreviewQuality, RenderSession, Settings},
        source::Source,
    };
    use std::path::PathBuf;

    fn image(artifacts: Vec<crate::inspection::Artifact>) -> RenderedImage {
        let Content::Image(image) = artifacts.into_iter().next().unwrap().content else {
            panic!("expected an image")
        };
        *image
    }
    let root = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = std::sync::Arc::new(Source::open(root.join("blue-parking-sign.x3f")).unwrap());
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source.clone());
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    let saved_edits = crate::sidecar::encode(&settings).unwrap();
    let mut full_profile = Profile::new(Mode::Wall);
    let full = engine
        .render(&source, &settings, &mut full_profile)
        .unwrap();
    assert!(full_profile
        .host
        .iter()
        .any(|s| s.stage == "submit row banding correction"));
    let reconstruction_stages = [
        "submit reference noise base",
        "submit reference column conditioning",
        "submit reference luminance ratio",
        "submit reference pattern conditioning",
        "submit texture restoration",
        "submit tiled ColorDQ",
        "submit color tile projection",
    ];
    for stage in reconstruction_stages {
        assert!(
            full_profile.host.iter().any(|s| s.stage == stage),
            "{stage}"
        );
    }
    let full_bytes = engine.readback_image(&full).unwrap();
    settings.preview_quality = PreviewQuality::Draft;
    let mut draft_profile = Profile::new(Mode::Wall);
    let draft = image(
        engine
            .render_session(
                &mut session,
                &settings,
                &["final"],
                || false,
                &mut draft_profile,
            )
            .unwrap()
            .unwrap(),
    );
    assert!(!draft_profile
        .host
        .iter()
        .any(|s| s.stage == "submit row banding correction"));
    assert!(
        draft.settings().row_correction,
        "preview omission must not change the requested edit"
    );
    for stage in reconstruction_stages.into_iter().chain(["submit ColorDQ"]) {
        assert!(
            !draft_profile.host.iter().any(|s| s.stage == stage),
            "{stage}"
        );
    }
    assert!(draft.settings().reconstruction.is_some());
    assert_eq!(
        crate::sidecar::encode(draft.settings()).unwrap(),
        saved_edits
    );
    let draft_bytes = engine.readback_image(&draft).unwrap();
    let mut standalone = settings.clone();
    standalone.preview_quality = PreviewQuality::Fast;
    let mut standalone_profile = Profile::new(Mode::Wall);
    let standalone_image = image(
        engine
            .render_session(
                &mut session,
                &standalone,
                &["final"],
                || false,
                &mut standalone_profile,
            )
            .unwrap()
            .unwrap(),
    );
    for stage in reconstruction_stages {
        assert!(
            standalone_profile.host.iter().any(|s| s.stage == stage),
            "{stage}"
        );
    }
    assert!(!standalone_profile
        .host
        .iter()
        .any(|s| s.stage == "submit row banding correction"));
    assert_eq!(
        standalone_image.description().extent,
        draft.description().extent
    );
    assert_eq!(
        crate::sidecar::encode(standalone_image.settings()).unwrap(),
        saved_edits
    );
    let standalone_bytes = engine.readback_image(&standalone_image).unwrap();
    assert_ne!(standalone_bytes, draft_bytes);
    assert!(bytemuck::cast_slice::<u8, f32>(&standalone_bytes)
        .iter()
        .all(|v| v.is_finite()));
    let fresh = engine
        .render(&source, &standalone, &mut Profile::default())
        .unwrap();
    assert_eq!(engine.readback_image(&fresh).unwrap(), standalone_bytes);
    standalone.reconstruction = None;
    let unchecked = engine
        .render(&source, &standalone, &mut Profile::default())
        .unwrap();
    assert_ne!(engine.readback_image(&unchecked).unwrap(), standalone_bytes);
    // Stronger cleanup keeps the same camera-derived base in Fast.
    standalone.reconstruction = Some(crate::noise::Controls {
        luminance: 4,
        ..Default::default()
    });
    let stronger = engine
        .render(&source, &standalone, &mut Profile::default())
        .unwrap();
    let stronger_bytes = engine.readback_image(&stronger).unwrap();
    assert!(bytemuck::cast_slice::<u8, f32>(&stronger_bytes)
        .iter()
        .all(|v| v.is_finite()));
    assert_ne!(stronger_bytes, standalone_bytes);
    let mut bypassed = settings.clone();
    bypassed.reconstruction = None;
    let unchecked = engine
        .render(&source, &bypassed, &mut Profile::default())
        .unwrap();
    assert_eq!(engine.readback_image(&unchecked).unwrap(), draft_bytes);
    // Smart drafts also omit explicit advanced ColorDQ choices.
    bypassed.reconstruction = settings.reconstruction;
    bypassed.color_dq_gain = Some(1.);
    let manual = engine
        .render(&source, &bypassed, &mut Profile::default())
        .unwrap();
    assert_eq!(engine.readback_image(&manual).unwrap(), draft_bytes);
    assert_eq!(
        draft.description().extent,
        full.description().extent.map(|v| v / 2)
    );
    assert_eq!(draft.view().crop, full.view().crop.map(|v| v / 2));
    assert_eq!(draft.view().extent, full.view().extent.map(|v| v / 2));
    assert_eq!(draft.source_metadata().width, source.metadata.width);
    assert!(bytemuck::cast_slice::<u8, f32>(&draft_bytes)
        .iter()
        .all(|v| v.is_finite()));
    for step in 0..3 {
        if step == 1 {
            settings.exposure = 0.5;
            settings.saturation = 0.7;
            settings.reconstruction.as_mut().unwrap().texture.fine = 1.2;
        }
        if step == 2 {
            settings = full.settings().clone();
        }
        let cached = image(
            engine
                .render_session(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap(),
        );
        let fresh = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(
            engine.readback_image(&cached).unwrap(),
            engine.readback_image(&fresh).unwrap(),
            "quality/edit step {step}"
        );
        if step == 2 {
            assert_eq!(engine.readback_image(&cached).unwrap(), full_bytes);
        }
    }
    assert_eq!(engine.readback_image(&full).unwrap(), full_bytes);
    assert_eq!(engine.readback_image(&draft).unwrap(), draft_bytes);
    assert_eq!(
        engine.readback_image(&standalone_image).unwrap(),
        standalone_bytes
    );
    settings.preview_quality = PreviewQuality::Draft;
    let inspection = image(
        engine
            .render_artifacts(
                &source,
                &settings,
                &["calibration.layers"],
                &mut Profile::default(),
            )
            .unwrap(),
    );
    assert_eq!(inspection.description().extent, full.description().extent);
    let dng = engine
        .render_dng_base(&source, &settings, &mut Profile::default())
        .unwrap();
    assert_eq!(dng.description().extent, full.description().extent);
    let c = settings.reconstruction.as_mut().unwrap();
    c.texture.coarse = 0.25;
    for quality in [PreviewQuality::Fast, PreviewQuality::Draft] {
        settings.preview_quality = quality;
        let mut profile = Profile::new(Mode::Wall);
        let reduced = engine.render(&source, &settings, &mut profile).unwrap();
        assert_eq!(
            reduced.description().extent,
            full.description().extent.map(|v| v / 2)
        );
        assert_eq!(
            profile
                .host
                .iter()
                .any(|p| p.stage == "submit texture restoration"),
            quality == PreviewQuality::Fast
        );
        assert!(
            bytemuck::cast_slice::<u8, f32>(&engine.readback_image(&reduced).unwrap())
                .iter()
                .all(|v| v.is_finite())
        );
    }
    let mut profile = Profile::new(Mode::Wall);
    let dng = engine
        .render_dng_base(&source, &settings, &mut profile)
        .unwrap();
    assert_eq!(dng.description().extent, full.description().extent);
    assert!(profile
        .host
        .iter()
        .any(|p| p.stage == "submit texture restoration"));
}

#[test]
fn regional_views_keep_native_crop_and_all_four_rotations() {
    for (rotation, expected) in [
        (0, [212, 264, 512, 564]),
        (90, [312, 2800, 612, 3100]),
        (180, [4416, 2700, 4716, 3000]),
        (270, [4316, 164, 4616, 464]),
    ] {
        let region =
            super::super::region::Prepared::new(encoding(rotation), [100, 200, 400, 500]).unwrap();
        assert_eq!(
            region.physical,
            [
                expected[0] - 4,
                expected[1] - 4,
                expected[2] + 4,
                expected[3] + 4
            ]
        );
        assert_eq!(region.encode.crop, [4, 4, 304, 304]);
        assert_eq!(region.encode.output[..2], [300, 300]);
        assert_eq!(region.encode.source[2], rotation);
    }
    assert!(super::super::region::Prepared::new(encoding(0), [0, 0, 0, 10]).is_err());
    assert!(super::super::region::Prepared::new(encoding(0), [0, 0, 99999, 10]).is_err());
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn regional_detail_matches_full_and_continues_without_repeating_sensor_corrections() {
    use crate::{
        inspection::Content,
        profile::{Mode, Profile},
        render::{Engine, PreviewQuality, RenderSession, Settings},
        source::Source,
    };
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::new().unwrap();
    for (rotation, global_context) in [
        (0, false),
        (90, false),
        (180, false),
        (270, false),
        (0, true),
    ] {
        let mut source = Source::open(root.join("blue-parking-sign.x3f")).unwrap();
        std::sync::Arc::make_mut(&mut source.metadata).rotation = rotation;
        if global_context {
            // Exercise the whole-image reduced branch on the same immutable sensor samples.
            let row = [100., 1., 0.5, 10., 0.025, 0.7, 0.5, 4., 1., 1., 0., 1.5, 1.];
            let mut rows = [row, row];
            rows[1][0] = 12800.;
            std::sync::Arc::make_mut(&mut source.metadata)
                .matrices
                .insert(
                    "ISONoiseSettings".into(),
                    crate::source::Matrix {
                        encoding: crate::source::MatrixEncoding::Float32,
                        representation: "test reduced global context",
                        dimensions: vec![2, 13],
                        values: rows.into_iter().flatten().collect(),
                    },
                );
        }
        let source = std::sync::Arc::new(source);
        for (noise, tuned) in [(false, false), (true, false), (true, true)] {
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.reconstruction = noise.then(Default::default);
            if tuned {
                settings.reconstruction.as_mut().unwrap().texture = crate::noise::TextureControls {
                    fine: 0.8,
                    coarse: 0.25,
                    fine_size: 2.,
                    coarse_size: 2.,
                };
            }
            if global_context && noise {
                assert_eq!(
                    crate::noise::prepare(&source.metadata, "Sunlight", Default::default())
                        .unwrap()
                        .reduction[0],
                    0.5
                );
            }
            settings.sharpen_amount = Some(0.75);
            settings.saturation = 0.8;
            let full = engine
                .render(&source, &settings, &mut Profile::default())
                .unwrap();
            let full_bytes = engine.readback_image(&full).unwrap();
            let full_pixels: &[f32] = bytemuck::cast_slice(&full_bytes);
            let mut session = RenderSession::new(source.clone());
            settings.preview_quality = PreviewQuality::Draft;
            let overview = engine
                .render_session(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap();
            settings.preview_quality = PreviewQuality::Full;
            // A central region crosses both native tile seams. A corner exercises sensor support.
            let [w, h] = full.view().extent;
            for rect in [
                [w / 2 - 150, h / 2 - 150, w / 2 + 150, h / 2 + 150],
                [0, 0, 200, 220],
            ] {
                settings.preview_region = Some(rect);
                let geometry = super::super::region::Prepared::new(
                    crate::nodes::sensor::calibration::encode(
                        &source.metadata,
                        crate::color_space::ColorSpace::from_metadata(&source.metadata).unwrap(),
                    )
                    .unwrap(),
                    rect,
                )
                .unwrap();
                let mut profile = Profile::new(Mode::Wall);
                let artifacts = engine
                    .render_session(&mut session, &settings, &["final"], || false, &mut profile)
                    .unwrap()
                    .unwrap();
                let Content::Image(detail) = &artifacts[0].content else {
                    panic!("expected detail")
                };
                assert_eq!(detail.view().extent, [rect[2] - rect[0], rect[3] - rect[1]]);
                assert!(detail.description().extent[0] < full.description().extent[0]);
                let bytes = engine.readback_image(detail).unwrap();
                let pixels: &[f32] = bytemuck::cast_slice(&bytes);
                let [x0, y0, x1, y1] = detail.view().crop;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let dst = ((y * detail.description().extent[0] + x) * 3) as usize;
                        let src = (((y + geometry.physical[1]) * full.description().extent[0]
                            + x
                            + geometry.physical[0])
                            * 3) as usize;
                        assert_eq!(
                            &pixels[dst..dst + 3],
                            &full_pixels[src..src + 3],
                            "rotation {rotation}, noise {noise}, pixel {x},{y}"
                        );
                    }
                }
                settings.preview_region = None;
                let mut continuation = Profile::new(Mode::Wall);
                let artifacts = engine
                    .render_session(
                        &mut session,
                        &settings,
                        &["final"],
                        || false,
                        &mut continuation,
                    )
                    .unwrap()
                    .unwrap();
                assert!(
                    !continuation.host.iter().any(|s| matches!(
                        s.stage,
                        "submit calibration"
                            | "submit repair"
                            | "submit highlight restoration"
                            | "submit despeckle"
                            | "submit row banding correction"
                            | "prepare reduced whole-image noise context"
                    )),
                    "continuation repeated prerequisites: {:?}",
                    continuation.host
                );
                let Content::Image(refined) = &artifacts[0].content else {
                    panic!("expected Full")
                };
                assert_eq!(engine.readback_image(refined).unwrap(), full_bytes);
                assert_eq!(
                    engine.readback_image(detail).unwrap(),
                    bytes,
                    "continuation mutated detail"
                );
            }
            drop(overview);
        }
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn independent_groups_match_neutral_bypass_and_cached_refinement() {
    use crate::{
        inspection::Content,
        profile::{Mode, Profile},
        render::{Engine, RenderSession, Settings},
        source::Source,
    };
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = std::sync::Arc::new(Source::open(root.join("blue-parking-sign.x3f")).unwrap());
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    let reference = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    let reference = engine.readback_image(&reference).unwrap();
    let controls = settings.reconstruction.as_mut().unwrap();
    controls.detail_enabled = false;
    controls.texture = crate::noise::TextureControls {
        fine: 1.5,
        coarse: 0.2,
        fine_size: 0.75,
        coarse_size: 1.8,
    };
    let neutral = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    assert!(
        reference == engine.readback_image(&neutral).unwrap(),
        "disabled detail must match neutral processing"
    );
    drop(reference);
    // An enabled but neutral Detail section needs no filtering when Noise is off.
    let mut neutral_only = settings.clone();
    let controls = neutral_only.reconstruction.as_mut().unwrap();
    controls.noise_enabled = false;
    controls.detail_enabled = true;
    controls.texture.fine = 1.;
    controls.texture.coarse = 1.;
    let mut profile = Profile::new(Mode::Wall);
    let image = engine.render(&source, &neutral_only, &mut profile).unwrap();
    assert!(!profile.host.iter().any(|p| matches!(
        p.stage,
        "submit reference noise base"
            | "submit reference pattern conditioning"
            | "submit texture restoration"
    )));
    neutral_only.reconstruction = None;
    let bypass = engine
        .render(&source, &neutral_only, &mut Profile::default())
        .unwrap();
    assert!(engine.readback_image(&image).unwrap() == engine.readback_image(&bypass).unwrap());
    let mut session = RenderSession::new(source.clone());
    for (noise, detail) in [
        (true, false),
        (false, true),
        (false, false),
        (true, true),
        (false, true),
    ] {
        let controls = settings.reconstruction.as_mut().unwrap();
        controls.noise_enabled = noise;
        controls.detail_enabled = detail;
        let mut profile = Profile::new(Mode::Wall);
        let fresh = engine.render(&source, &settings, &mut profile).unwrap();
        let fresh = engine.readback_image(&fresh).unwrap();
        let has_column = profile
            .host
            .iter()
            .any(|s| s.stage == "submit reference column conditioning");
        assert_eq!(has_column, noise);
        let artifacts = engine
            .render_session(
                &mut session,
                &settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        let Content::Image(cached) = &artifacts[0].content else {
            panic!("expected image")
        };
        assert!(
            fresh == engine.readback_image(cached).unwrap(),
            "cached groups noise={noise}, detail={detail}"
        );
        if !noise && !detail {
            let mut bypass = settings.clone();
            bypass.reconstruction = None;
            let bypass = engine
                .render(&source, &bypass, &mut Profile::default())
                .unwrap();
            assert!(
                fresh == engine.readback_image(&bypass).unwrap(),
                "both disabled must match the baseline"
            );
        }
        if !noise && detail {
            settings.preview_region = Some([1000, 900, 1300, 1200]);
            let region = engine
                .render(&source, &settings, &mut Profile::default())
                .unwrap();
            let geometry = super::super::region::Prepared::new(
                crate::nodes::sensor::calibration::encode(
                    &source.metadata,
                    crate::color_space::ColorSpace::from_metadata(&source.metadata).unwrap(),
                )
                .unwrap(),
                settings.preview_region.unwrap(),
            )
            .unwrap();
            let bytes = engine.readback_image(&region).unwrap();
            let full: &[f32] = bytemuck::cast_slice(&fresh);
            let pixels: &[f32] = bytemuck::cast_slice(&bytes);
            let [x0, y0, x1, y1] = region.view().crop;
            for y in y0..y1 {
                for x in x0..x1 {
                    let dst = ((y * region.description().extent[0] + x) * 3) as usize;
                    let src = (((y + geometry.physical[1]) * neutral.description().extent[0]
                        + x
                        + geometry.physical[0])
                        * 3) as usize;
                    assert_eq!(
                        &pixels[dst..dst + 3],
                        &full[src..src + 3],
                        "detail-only region {x},{y}"
                    );
                }
            }
            settings.preview_region = None;
        }
    }
    settings.preview_region = None;
    settings.exposure = 0.7;
    settings.saturation = 1.2;
    settings.tone.contrast = 0.3;
    settings.sharpen_amount = Some(0.75);
    settings.tone_enabled = false;
    settings.sharpening_enabled = false;
    let image = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    assert_eq!(image.settings(), &settings);
    let mut neutral = settings.processing_settings().into_owned();
    neutral.tone_enabled = true;
    neutral.sharpening_enabled = true;
    let expected = engine
        .render(&source, &neutral, &mut Profile::default())
        .unwrap();
    assert!(engine.readback_image(&image).unwrap() == engine.readback_image(&expected).unwrap());
}
