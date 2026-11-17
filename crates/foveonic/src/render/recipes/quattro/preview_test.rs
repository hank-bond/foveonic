//! Checks Quattro working grids, native correction histories and source-free preview edits.

use super::*;
use crate::{
    inspection::Content,
    profile::Profile,
    render::{ControlCapabilities, Rendering, Scale},
};

fn preview_divisors(default: &[u32]) -> Vec<u32> {
    let Some(selected) = std::env::var("SMALL_PREVIEW_SCALES").ok() else {
        return default.to_vec();
    };
    let divisors: Vec<u32> = selected.split(',').map(|v| v.parse().unwrap()).collect();
    assert!(!divisors.is_empty());
    divisors
}

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
    source.origin = Default::default();
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
        for divisor in preview_divisors(&[2, 4, 8, 16, 64, 1 << 31]) {
            let mut reduced = settings.clone();
            reduced.working_scale = Scale::new(divisor).unwrap();
            reduced.preview_region = None;
            let result = engine
                .render(&source, &reduced, &mut Profile::default())
                .unwrap();
            let native = crate::render::native_extent(&source.metadata).unwrap();
            assert_eq!(
                result.description().extent,
                reduced.working_scale.extent(native)
            );
            let bytes = engine.readback_image(&result).unwrap();
            let pixels: &[f32] = bytemuck::cast_slice(&bytes);
            assert!(pixels.iter().all(|v| v.is_finite()));
            assert!(
                pixels.iter().any(|v| *v > 0.),
                "{rendering:?} LOW divisor {divisor} has no positive output"
            );
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
    for quality in [Scale::NATIVE, Scale::new(2).unwrap()] {
        settings.working_scale = quality;
        let mut profile = Profile::default();
        let mut execution = Execution::default();
        let mut recipe = build(
            &engine,
            super::super::RecipeInput::Original(&source),
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
        let mut artifacts = {
            let input = crate::render::recipes::RecipeInput::Original(&source);
            let plan = engine
                .plan_artifacts(
                    &settings,
                    &mut recipe.graph,
                    recipe.final_id,
                    &["phase-detection.blue", "extraction.blue"],
                    &mut profile,
                    &mut execution,
                )
                .unwrap();
            let values = engine
                .run_graph(
                    input.context(),
                    &recipe.graph,
                    &plan.roots,
                    &mut profile,
                    &mut execution,
                    crate::render::execute::Completion::Queued,
                )
                .unwrap();
            engine.package_artifacts(input, &settings, &recipe.graph, plan, values)
        }
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

#[test]
#[ignore = "requires GPU and QUATTRO_X3F ordinary DP or sd HIGH original"]
fn small_color_grids_preserve_native_histories_and_requested_controls() {
    let source =
        std::sync::Arc::new(Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap());
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Auto", "Standard");
    settings.reconstruction = Some(controls::default_noise_controls());
    let mut native = None;
    for divisor in preview_divisors(&[1, 2, 4, 8, 16, 64, 1 << 31]) {
        let quality = Scale::new(divisor).unwrap();
        eprintln!(
            "Checking HIGH {:?} color histories and retained edits",
            quality
        );
        settings.working_scale = quality;
        let recipe = build(
            &engine,
            super::super::RecipeInput::Original(&source),
            &settings,
            &["final"],
            &mut Profile::default(),
            &mut Execution::default(),
        )
        .unwrap();
        let restored = recipe
            .graph
            .outputs
            .iter()
            .position(|o| o.name == "highlight-restoration.rgb")
            .unwrap();
        let blue = recipe
            .graph
            .outputs
            .iter()
            .position(|o| o.name == "blue-mismatch.blue")
            .or_else(|| {
                recipe
                    .graph
                    .outputs
                    .iter()
                    .position(|o| o.name == "luma-clusters.blue")
            })
            .unwrap();
        let mask = recipe
            .graph
            .outputs
            .iter()
            .position(|o| o.name == "highlight-restoration.mask")
            .unwrap();
        let values = engine
            .run_graph(
                crate::render::execute::Input::Source(&source),
                &recipe.graph,
                &[restored, blue, mask],
                &mut Profile::default(),
                &mut crate::interactive::Execution::default(),
                crate::render::execute::Completion::Queued,
            )
            .unwrap();
        let histories: Vec<_> = values
            .into_iter()
            .map(|value| engine.gpu.readback(&value.buffer).unwrap())
            .collect();
        if let Some(native) = &native {
            assert!(
                &histories == native,
                "{quality:?} must preserve native RGB, blue and highlight histories"
            );
        } else {
            native = Some(histories);
        }
        if quality == Scale::NATIVE {
            continue;
        }
        let extent = color_grid(&source.metadata, quality).unwrap().extent;
        let blue_extent = output_grid(&source.metadata, quality).unwrap().extent;
        let pools: Vec<_> = recipe
            .graph
            .nodes
            .iter()
            .filter_map(|node| {
                if let Operation::PoolSignal(p) = &node.operation {
                    Some(p)
                } else {
                    None
                }
            })
            .collect();
        let same_size = same_resolution(&source.metadata, quality);
        assert_eq!(
            pools.len(),
            if quality.divisor() >= 4 {
                1
            } else {
                2 + usize::from(!same_size) + usize::from(tiger::supported(&source.metadata))
            }
        );
        assert_eq!(pools[0].output_extent(), extent);
        if same_size {
            assert_eq!(extent, blue_extent);
            if quality.divisor() < 4 {
                assert_eq!(pools[1].output_extent(), extent);
            }
        } else {
            assert_eq!(pools[1].output_extent(), blue_extent);
            assert_eq!(pools[2].output_extent(), extent);
        }
        assert!(pools
            .iter()
            .all(|pool| pool.working_scale() == Some(quality)));
        for node in &recipe.graph.nodes {
            match &node.operation {
                Operation::TwoScaleNoise(p) => assert_eq!(p.extent(), extent),
                Operation::RawColorBlend(p) => assert_eq!(p.extent(), extent),
                Operation::SectorColor(p) => assert_eq!(p.extent(), extent),
                Operation::BlueGuidedReconstruct(p) => {
                    assert_eq!(p.extent(), extent);
                    assert_eq!(p.blue_extent(), blue_extent);
                    assert_eq!(p.output_extent(), blue_extent);
                    assert_eq!(p.doubles_output(), !same_size);
                }
                _ => {}
            }
        }
        let image = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(image.settings().working_scale, quality);
        assert_eq!(image.source_metadata().width, source.metadata.width);
        assert_eq!(image.source_metadata().height, source.metadata.height);
        let expected = quality.extent(crate::render::native_extent(&source.metadata).unwrap());
        assert_eq!(image.description().extent, expected);
        assert_eq!(image.view().extent, expected);
        let bytes = engine.readback_image(&image).unwrap();
        let pixels: &[f32] = bytemuck::cast_slice(&bytes);
        assert!(pixels.iter().all(|v| v.is_finite()));
        assert!(pixels.iter().any(|v| *v > 0.));
        let mut edited = settings.clone();
        let controls = edited.reconstruction.as_mut().unwrap();
        controls.chroma = 4;
        controls.luminance = 4;
        controls.detail = 0.;
        edited.exposure = 0.25;
        let mut session = crate::interactive::RenderSession::new(source.clone());
        let warm = engine
            .render_session(
                &mut session,
                &settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap()
            .pop()
            .unwrap();
        let Content::Image(warm) = warm.content else {
            panic!("final image")
        };
        assert!(engine.readback_image(&warm).unwrap() == bytes);
        let warm = engine
            .render_session(
                &mut session,
                &edited,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap()
            .pop()
            .unwrap();
        let Content::Image(warm) = warm.content else {
            panic!("final image")
        };
        let fresh = engine
            .render(&source, &edited, &mut Profile::default())
            .unwrap();
        let edited_bytes = engine.readback_image(&fresh).unwrap();
        let retained_bytes = engine.readback_image(&warm).unwrap();
        let difference = bytemuck::cast_slice::<u8, f32>(&retained_bytes)
            .iter()
            .zip(bytemuck::cast_slice::<u8, f32>(&edited_bytes))
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        assert!(
            retained_bytes == edited_bytes,
            "{quality:?} retained/fresh maximum difference {difference}"
        );
        assert!(edited_bytes != bytes);
        assert!(engine.readback_image(&image).unwrap() == bytes);
    }
}

#[test]
#[ignore = "requires GPU and QUATTRO_X3F ordinary DP or sd HIGH original"]
fn small_composition_preserves_native_metadata_and_full_inspection() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    for quality in [Scale::new(4).unwrap(), Scale::new(8).unwrap()] {
        let mut settings = Settings::new("Auto", "Standard");
        settings.working_scale = quality;
        let image = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        settings.crop.bounds = [0.1, 0.15, 0.9, 0.85];
        settings.rotation.turns = 1;
        settings.rotation.degrees = 7.;
        let composed = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(composed.settings().working_scale, quality);
        assert_eq!(composed.source_metadata().width, source.metadata.width);
        assert_eq!(composed.source_metadata().height, source.metadata.height);
        assert_eq!(composed.uncropped_view(), image.uncropped_view());
        assert!(
            engine.readback_image(&composed).unwrap() == engine.readback_image(&image).unwrap()
        );
        assert!(composed.view().extent[0] < image.view().extent[1]);
        assert!(composed.view().extent[1] < image.view().extent[0]);
        let artifact = engine
            .render_artifacts(
                &source,
                &settings,
                &["highlight-restoration.rgb"],
                &mut Profile::default(),
            )
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(artifact.settings.working_scale, Scale::NATIVE);
        let Content::Image(native) = artifact.content else {
            panic!("native inspection image")
        };
        assert_eq!(
            native.description().extent,
            calibration::color_extent(&source.metadata).unwrap()
        );
    }
}

#[test]
#[ignore = "requires GPU and QUATTRO_X3F ordinary DP or sd HIGH original"]
fn first_small_edits_restart_from_corrected_pools_without_native_gpu_retention() {
    let source =
        std::sync::Arc::new(Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap());
    let engine = Engine::new().unwrap();
    let native_cell_bytes = source.samples().len() as u64 * 4;
    for quality in [Scale::new(4).unwrap(), Scale::new(8).unwrap()] {
        for (noise_edit, combined_edit) in [(false, false), (true, false), (true, true)] {
            let mut settings = Settings::new("Auto", "Standard");
            settings.working_scale = quality;
            settings.reconstruction = Some(controls::default_noise_controls());
            // Equal-resolution Draft retains more reduced color samples than
            // the doubled route. Several working histories can exceed one
            // native source in aggregate without retaining a native image.
            let extent = output_grid(&source.metadata, quality).unwrap().extent;
            // Recovery evidence packs two RGBA records per working cell.
            let largest_working_image = u64::from(extent[0]) * u64::from(extent[1]) * 32;
            let recipe = build(
                &engine,
                crate::render::recipes::RecipeInput::Original(&source),
                &settings,
                &["final"],
                &mut Profile::default(),
                &mut Execution::default(),
            )
            .unwrap();
            let required = recipe.graph.required(&[recipe.final_id]).unwrap();
            for node in &recipe.graph.nodes {
                if !node.outputs.iter().any(|id| required[*id]) {
                    continue;
                }
                match &node.operation {
                    Operation::RgbDespeckle(p) => assert_eq!(p.extent(), extent),
                    Operation::HighlightRestoration(p) => {
                        assert_eq!(p.extent(), extent);
                        assert!(p.requires_evidence());
                        assert_eq!(node.inputs.len(), 3);
                    }
                    _ => {}
                }
            }
            let mut session = crate::interactive::RenderSession::new(source.clone());
            let mut initial_profile = Profile::new(crate::profile::Mode::Wall);
            let initial = engine
                .render_session(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut initial_profile,
                )
                .unwrap()
                .unwrap()
                .pop()
                .unwrap();
            let Content::Image(initial) = initial.content else {
                panic!("initial image")
            };
            let initial_bytes = engine.readback_image(&initial).unwrap();
            assert!(initial_profile.nodes.iter().any(|n| n.executed));
            if let Some(sparse) = initial_profile
                .nodes
                .iter()
                .find(|n| n.executed && n.operation == "SparseRepair")
            {
                assert_eq!(
                    sparse.activity.unwrap().dispatches,
                    7,
                    "an RGB-only preview must not run the HIGH blue finishing passes"
                );
            }
            assert!(
                initial_profile.nodes.iter().all(|n| !n.executed
                    || !matches!(
                        n.operation,
                        "OrderedBlueRepair"
                            | "BluePattern"
                            | "BlueDespeckle"
                            | "LumaClusters"
                            | "BlueMismatch"
                    )),
                "equal-resolution previews must not execute the separate HIGH blue chain"
            );
            assert!(
                session
                    .cache
                    .buffers()
                    .all(|buffer| buffer.size() <= largest_working_image),
                "initial small preparation must not retain native cells"
            );
            if noise_edit {
                settings.reconstruction.as_mut().unwrap().chroma = 4;
            } else {
                settings.exposure = 0.25;
            }
            if combined_edit {
                let controls = settings.reconstruction.as_mut().unwrap();
                controls.luminance = 4;
                controls.detail = 0.;
                settings.exposure = 0.25;
            }
            let mut profile = Profile::new(crate::profile::Mode::Wall);
            let before = engine.gpu_activity();
            let edited = engine
                .render_session(&mut session, &settings, &["final"], || false, &mut profile)
                .unwrap()
                .unwrap()
                .pop()
                .unwrap();
            let activity = engine.gpu_activity().since(before);
            let Content::Image(edited) = edited.content else {
                panic!("edited image")
            };
            let noise = profile
                .nodes
                .iter()
                .position(|n| n.operation == "TwoScaleNoise")
                .unwrap();
            assert!(
                profile.nodes[..noise].iter().all(|n| !n.executed),
                "native correction and pooling must be cached on the first edit"
            );
            assert_eq!(profile.nodes[noise].executed, noise_edit);
            assert!(
                session
                    .cache
                    .buffers()
                    .all(|buffer| buffer.size() <= largest_working_image),
                "a small restart must not retain the native cells"
            );
            assert!(activity.upload_bytes < native_cell_bytes);
            let fresh = engine
                .render(&source, &settings, &mut Profile::default())
                .unwrap();
            assert!(
                engine.readback_image(&edited).unwrap() == engine.readback_image(&fresh).unwrap()
            );
            assert!(engine.readback_image(&initial).unwrap() == initial_bytes);
            eprintln!("{quality:?} noise_edit={noise_edit} GPU cache={} native_cells={native_cell_bytes} uploads={} dispatches={}", session.cached_bytes(), activity.upload_bytes, activity.dispatches);
        }
    }
}

#[test]
fn component_grids_keep_native_metadata_and_cover_every_sensor_scale() {
    use crate::source::{Matrix, MatrixEncoding, Metadata};
    for (model, camera, native, major) in [
        ("SIGMA dp2 Quattro", 81., [5888, 3672], 3),
        ("SIGMA sd Quattro", 40., [5888, 3776], 3),
        ("SIGMA sd Quattro H", 41., [6656, 4480], 4),
    ] {
        for low in [false, true] {
            let sensor = native.map(|v| if low { v / 2 } else { v });
            let metadata = Metadata {
                width: sensor[0],
                height: sensor[1],
                rotation: 0,
                processing_version: Some([major, 0, 0, 0]),
                properties: [("CAMMODEL".into(), model.into())].into(),
                matrices: [(
                    "CAMERAID".into(),
                    Matrix {
                        encoding: MatrixEncoding::Float32,
                        representation: "f64",
                        dimensions: vec![1],
                        values: vec![camera],
                    },
                )]
                .into(),
                lists: Default::default(),
            };
            assert_eq!(metadata.is_quattro_low(), low);
            let native_color = calibration::color_extent(&metadata).unwrap();
            for exponent in 0..=31 {
                let divisor = 1_u32 << exponent;
                let scale = Scale::new(divisor).unwrap();
                let color = color_grid(&metadata, scale).unwrap();
                let output = output_grid(&metadata, scale).unwrap();
                assert_eq!(color.native, native_color);
                assert_eq!(
                    color.logical,
                    color_scale(&metadata, scale).extent(native_color)
                );
                assert_eq!(output.logical, scale.extent(sensor.map(|v| v as u32)));
                if scale.is_native() {
                    assert_eq!(color.extent, native_color);
                    assert_eq!(color.origin, [0, 0]);
                    assert_eq!(output.extent, sensor.map(|v| v as u32));
                } else {
                    assert!(color.extent.iter().all(|v| v.is_multiple_of(4)));
                    let support = region::CAMERA_SUPPORT.div_ceil(divisor).max(64);
                    assert!(output.origin[0] >= region::CAMERA_SUPPORT.div_ceil(divisor));
                    assert!(output
                        .extent
                        .iter()
                        .zip(output.origin)
                        .zip(output.logical)
                        .all(|((&extent, origin), logical)| extent >= logical + origin * 2));
                    assert_eq!(
                        color.origin,
                        [support
                            / if same_resolution(&metadata, scale) {
                                1
                            } else {
                                2
                            }; 2]
                    );
                }
                assert_eq!(
                    output.extent,
                    color
                        .extent
                        .map(|v| v * if same_resolution(&metadata, scale) {
                            1
                        } else {
                            2
                        })
                );
                assert_eq!(
                    output.origin,
                    color
                        .origin
                        .map(|v| v * if same_resolution(&metadata, scale) {
                            1
                        } else {
                            2
                        })
                );
                assert_eq!([metadata.width, metadata.height], sensor);
            }
        }
    }
}
