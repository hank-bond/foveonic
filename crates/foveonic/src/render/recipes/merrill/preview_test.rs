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
fn color_preview_support_requires_valid_merrill_geometry_and_refines_the_whole_frame() {
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
        assert!(!settings.regional_preview_available(&metadata));
    }
    settings.rendering = crate::render::Rendering::Monochrome;
    assert!(!settings.fast_preview_available(&metadata));
    settings.rendering = crate::render::Rendering::Color;
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
        "submit reference detail",
        "submit tiled ColorDQ",
        "submit reconstruction",
    ];
    for stage in reconstruction_stages {
        assert!(
            full_profile.host.iter().any(|s| s.stage == stage),
            "{stage}"
        );
    }
    let full_bytes = engine.readback_image(&full).unwrap();
    settings.preview_quality = PreviewQuality::Fast;
    let mut fast_profile = Profile::new(Mode::Wall);
    let fast = image(
        engine
            .render_session(
                &mut session,
                &settings,
                &["final"],
                || false,
                &mut fast_profile,
            )
            .unwrap()
            .unwrap(),
    );
    assert!(!fast_profile
        .host
        .iter()
        .any(|s| s.stage == "submit row banding correction"));
    assert!(
        fast.settings().row_correction,
        "preview omission must not change the requested edit"
    );
    for stage in reconstruction_stages.into_iter().chain(["submit ColorDQ"]) {
        assert!(
            !fast_profile.host.iter().any(|s| s.stage == stage),
            "{stage}"
        );
    }
    assert!(fast.settings().reconstruction.is_some());
    assert_eq!(
        crate::sidecar::encode(fast.settings()).unwrap(),
        saved_edits
    );
    let fast_bytes = engine.readback_image(&fast).unwrap();
    let mut bypassed = settings.clone();
    bypassed.reconstruction = None;
    let unchecked = engine
        .render(&source, &bypassed, &mut Profile::default())
        .unwrap();
    assert_eq!(engine.readback_image(&unchecked).unwrap(), fast_bytes);
    // An explicit advanced ColorDQ choice must also stay out of Fast processing.
    bypassed.reconstruction = settings.reconstruction;
    bypassed.color_dq_gain = Some(1.);
    let manual = engine
        .render(&source, &bypassed, &mut Profile::default())
        .unwrap();
    assert_eq!(engine.readback_image(&manual).unwrap(), fast_bytes);
    assert_eq!(
        fast.description().extent,
        full.description().extent.map(|v| v / 2)
    );
    assert_eq!(fast.view().crop, full.view().crop.map(|v| v / 2));
    assert_eq!(fast.view().extent, full.view().extent.map(|v| v / 2));
    assert_eq!(fast.source_metadata().width, source.metadata.width);
    assert!(bytemuck::cast_slice::<u8, f32>(&fast_bytes)
        .iter()
        .all(|v| v.is_finite()));
    for step in 0..3 {
        if step == 1 {
            settings.exposure = 0.5;
            settings.saturation = 0.7;
            settings.reconstruction.as_mut().unwrap().detail = 1.2;
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
    assert_eq!(engine.readback_image(&fast).unwrap(), fast_bytes);
    settings.preview_quality = PreviewQuality::Fast;
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
}
