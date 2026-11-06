//! Checks native sparse-sensor correction before reduced interchangeable-lens previews.

use super::*;
use crate::{
    inspection::Content,
    profile::Profile,
    render::{ControlCapabilities, PreviewQuality, Rendering},
};

#[test]
#[ignore = "requires GPU and QUATTRO_X3F original for synthetic LOW geometry"]
fn synthetic_low_grid_renders_without_high_blue_storage() {
    let mut source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    assert_eq!(source.storage(), SourceStorage::PackedRgbBluePhases);
    let pixels = source
        .samples()
        .as_chunks::<6>()
        .0
        .iter()
        .flat_map(|v| [v[0], v[1], ((v[2] + v[3]) + v[4] + v[5]) * 0.25])
        .collect();
    source.pixels = pixels;
    source.storage = SourceStorage::InterleavedRgb;
    let metadata = std::sync::Arc::make_mut(&mut source.metadata);
    metadata.width /= 2;
    metadata.height /= 2;
    for name in [
        "ActiveImageArea",
        "KeepImageArea",
        "DarkShieldTop",
        "DarkShieldBottom",
        "DarkShieldColRange",
    ] {
        if let Some(matrix) = metadata.matrices.get_mut(name) {
            for value in &mut matrix.values {
                *value = (*value / 2.).floor();
            }
        }
    }
    let active = metadata.fixed::<4>("ActiveImageArea").unwrap();
    metadata.properties.insert(
        "X3F_IMAGE_WIDTH".into(),
        ((active[2] - active[0] + 1.) as u32).to_string(),
    );
    metadata.properties.insert(
        "X3F_IMAGE_HEIGHT".into(),
        ((active[3] - active[1] + 1.) as u32).to_string(),
    );
    metadata
        .properties
        .insert("RESOLUTION".into(), "LOW".into());
    source.validate().unwrap();
    let original = source.samples().to_vec();
    let engine = Engine::new().unwrap();
    let exports = tempfile::tempdir().unwrap();
    let mut settings = Settings::new("Auto", "Standard");
    let mut controls = controls::default_noise_controls();
    controls.luminance = 2;
    settings.reconstruction = Some(controls);
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        settings.rendering = rendering;
        settings.monochrome.noise = 2;
        let image = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(
            image.view().extent,
            crate::render::native_extent(&source.metadata).unwrap()
        );
        let bytes = engine.readback_image(&image).unwrap();
        let pixels: &[f32] = bytemuck::cast_slice(&bytes);
        assert!(pixels.iter().all(|v| v.is_finite()));
        assert!(pixels.iter().any(|v| *v > 0.));
        if rendering == Rendering::Color {
            for composed in [false, true] {
                let mut detail = settings.clone();
                if composed {
                    detail.crop.bounds = [0.1, 0.2, 0.9, 0.8];
                    detail.rotation.degrees = 17.;
                    detail.output_size = Some([700, 1050]);
                }
                let baseline = if composed {
                    engine
                        .render(&source, &detail, &mut Profile::default())
                        .unwrap()
                } else {
                    image.clone()
                };
                detail.preview_region = Some([32, 32, 160, 160]);
                assert!(!detail.regional_preview_available(&source.metadata));
                let whole = engine
                    .render(&source, &detail, &mut Profile::default())
                    .unwrap();
                assert_eq!(whole.view(), baseline.view());
                assert_eq!(whole.params().mapping, baseline.params().mapping);
                assert_eq!(whole.settings().preview_region, None);
                assert_eq!(
                    engine.readback_image(&whole).unwrap(),
                    engine.readback_image(&baseline).unwrap()
                );
            }
        }
        let tiff = exports.path().join(format!("{rendering:?}.tiff"));
        let dng = exports.path().join(format!("{rendering:?}.dng"));
        let expected = image.view().extent;
        assert_eq!(
            engine
                .write_tiff(&image, &tiff, &mut Profile::default())
                .unwrap(),
            expected
        );
        crate::dng::write_source(&engine, &source, &settings, &dng, &mut Profile::default())
            .unwrap();
    }
    assert_eq!(source.samples(), original);
}

#[test]
#[ignore = "requires GPU and QUATTRO_X3F sd Quattro or Quattro H original"]
fn interchangeable_native_phase_correction_precedes_reduced_reconstruction() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    assert!(calibration::is_interchangeable(&source.metadata));
    let engine = Engine::new().unwrap();
    let cap = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = cap
        .editing_defaults(&source.metadata, Rendering::Color)
        .unwrap();
    settings.white_balance = cap.default_white_balance(&source.metadata).unwrap();
    settings.color_mode = cap.default_color_mode(&source.metadata).unwrap();
    let mut expected = None;
    for quality in [PreviewQuality::Full, PreviewQuality::Fast] {
        settings.preview_quality = quality;
        let mut profile = Profile::default();
        let mut execution = Execution::default();
        let recipe = build(
            &engine,
            &source,
            &settings,
            &["final"],
            &mut profile,
            &mut execution,
        )
        .unwrap();
        let phase = recipe
            .graph
            .outputs
            .iter()
            .position(|o| o.name == "phase-detection.blue")
            .expect("every grid needs native blue-plane sensor correction");
        let reconstruct = recipe
            .graph
            .nodes
            .iter()
            .find(|n| matches!(n.operation, Operation::BlueGuidedReconstruct(_)))
            .unwrap();
        // Follow the blue history specifically, so an orphan correction node
        // cannot satisfy this check while reconstruction uses uncorrected blue.
        let mut pending = vec![reconstruct.inputs[3]];
        let mut reaches_correction = false;
        while let Some(id) = pending.pop() {
            if id == phase {
                reaches_correction = true;
                break;
            }
            pending.extend(&recipe.graph.nodes[recipe.graph.outputs[id].producer].inputs);
        }
        assert!(
            reaches_correction,
            "{quality:?} must reconstruct corrected blue"
        );
        let mut artifacts = engine
            .collect_artifacts(
                &source,
                &settings,
                &recipe.graph,
                recipe.final_id,
                &["phase-detection.blue", "extraction.blue"],
                recipe.params,
                &mut profile,
                &mut execution,
            )
            .unwrap();
        let Content::Image(raw) = artifacts.pop().unwrap().content else {
            panic!("raw blue")
        };
        let Content::Image(corrected) = artifacts.pop().unwrap().content else {
            panic!("corrected blue")
        };
        assert_eq!(
            corrected.description().extent,
            [source.metadata.width as u32, source.metadata.height as u32]
        );
        let pixels = engine.readback_image(&corrected).unwrap();
        let raw = engine.readback_image(&raw).unwrap();
        assert!(pixels != raw, "fixture must exercise the sparse correction");
        if let Some(expected) = &expected {
            assert!(
                pixels == *expected,
                "{quality:?} must use exactly the Full native correction"
            );
        } else {
            expected = Some(pixels);
        }
    }
}
