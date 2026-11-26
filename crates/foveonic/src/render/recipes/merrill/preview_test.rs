//! Checks working-grid framing, selected reconstruction and immutable native refinement.

use super::*;

fn encoding(rotation: u32) -> EncodeParams {
    let mut output = [4704, 3136, 0, 0];
    if matches!(rotation, 90 | 270) {
        output.swap(0, 1);
    }
    EncodeParams {
        mapping: [[0.; 4]; 2],
        source: [4928, 3264, rotation, 0],
        crop: [112, 64, 4816, 3200],
        output,
    }
}

fn framing(grid: &Grid) -> EncodeParams {
    let mut graph = Graph::default();
    let rgb = graph
        .add(
            Operation::RenderedInput {
                data: crate::graph::DataType::LinearRgb(ColorSpace::Srgb),
            },
            &[],
        )
        .unwrap();
    let mut recipe = BuiltRecipe {
        graph,
        final_id: rgb,
        params: grid.encode,
        rgb_to_native: [[1., 0., 0.], [0., 1., 0.]],
        rgb_extent: grid.grid.extent,
        rgb_bounds: grid.encode.crop,
    };
    grid.frame(&mut recipe, ColorSpace::Srgb).unwrap();
    let Operation::ResampleRgb { params, .. } = &recipe.graph.nodes.last().unwrap().operation
    else {
        panic!("missing reduced camera frame")
    };
    *params
}

#[test]
fn reduced_camera_frame_places_pooled_pixel_centers_on_native_pixel_centers() {
    for rotation in [0, 90, 180, 270] {
        for divisor in [2, 4, 8, 16] {
            let mut native = encoding(rotation);
            native.crop[0] += 1;
            native.crop[1] += 3;
            let grid = Grid::new(native, None, Scale::new(divisor).unwrap()).unwrap();
            let params = framing(&grid);
            for uv in [[0.01_f32, 0.03_f32], [0.5, 0.5], [0.97, 0.99]] {
                for axis in 0..2 {
                    let row = params.mapping[axis];
                    let working = row[0] * uv[0] + row[1] * uv[1] + row[2];
                    // Pooling averages indices 0..d-1. Its first sample is at
                    // native index (d-1)/2, not the block's left/top edge.
                    let actual = (working - grid.grid.origin[axis] as f32) * divisor as f32
                        + (divisor - 1) as f32 * 0.5;
                    let expected = native.crop[axis] as f32
                        + uv[axis] * (native.crop[axis + 2] - native.crop[axis]) as f32
                        - 0.5;
                    assert!((actual - expected).abs() < 0.001,
                        "rotation {rotation}, divisor {divisor}, axis {axis}: native {actual}, expected {expected}");
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn pooled_ramps_keep_native_registration_through_camera_framing() {
    use crate::{gpu::Gpu, nodes::finish::resize, profile::Profile};
    let gpu = Gpu::test_executor();
    let pixels: Vec<f32> = (0..96)
        .flat_map(|y| (0..128).flat_map(move |x| [x as f32, y as f32, 1.]))
        .collect();
    let input = gpu.upload("Native registration ramps", &pixels).unwrap();
    let native = EncodeParams {
        source: [128, 96, 0, 0],
        crop: [9, 7, 119, 87],
        output: [110, 80, 0, 0],
        mapping: [[0.; 4]; 2],
    };
    for divisor in [2, 4, 8] {
        let grid = Grid::new(native, None, Scale::new(divisor).unwrap()).unwrap();
        let pooled = pool::apply(&gpu, &input, &grid.reduction, &mut Profile::default()).unwrap();
        let params = framing(&grid);
        let (output, _) = resize::apply(&gpu, &pooled, &params, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let values: &[f32] = bytemuck::cast_slice(&bytes);
        let [w, h, _, _] = params.output;
        let mut maximum = 0_f32;
        for y in 2..h - 2 {
            for x in 2..w - 2 {
                let expected = [
                    9. + (x as f32 + 0.5) * 110. / w as f32 - 0.5,
                    7. + (y as f32 + 0.5) * 80. / h as f32 - 0.5,
                ];
                for axis in 0..2 {
                    maximum = maximum
                        .max((values[((y * w + x) * 3) as usize + axis] - expected[axis]).abs());
                }
            }
        }
        eprintln!("divisor {divisor}: maximum native registration error {maximum}");
        // The existing photographic interpolator has sixteen fractional phases.
        // Allow its sub-cell approximation, never a half-cell framing displacement.
        assert!(maximum < divisor as f32 * 0.125);
    }
}

#[test]
fn recovery_and_speckle_share_one_sensor_reduction_per_grid() {
    let mut graph = Graph::default();
    let source = graph.add(Operation::Source, &[]).unwrap();
    let draft = Grid::new(encoding(0), None, Scale::new(4).unwrap()).unwrap();
    let thumb = Grid::new(encoding(0), None, Scale::new(8).unwrap()).unwrap();
    let pooled = draft.append(&mut graph, source).unwrap();
    assert_eq!(draft.append(&mut graph, source).unwrap(), pooled);
    assert_eq!(graph.nodes.len(), 2);
    let smaller = thumb.append(&mut graph, source).unwrap();
    assert_ne!(smaller, pooled);
    assert_eq!(graph.sensor_data_outputs, [pooled, smaller]);
}

#[test]
fn half_grid_preserves_crop_orientation_and_explicit_output_size() {
    for rotation in [0, 90, 180, 270] {
        let native = encoding(rotation);
        let grid = Grid::new(native, None, crate::render::Scale::new(2).unwrap()).unwrap();
        assert_eq!(grid.grid.logical, [2464, 1632]);
        assert_eq!(grid.reduction.output_extent(), grid.grid.extent);
        assert_eq!(
            grid.encode.source,
            [grid.grid.extent[0], grid.grid.extent[1], rotation, 0]
        );
        assert_eq!(grid.encode.crop, grid.grid.bounds(native.crop));
        assert_eq!(
            grid.encode.output[..2],
            native.output[..2].iter().map(|v| v / 2).collect::<Vec<_>>()
        );
        let sized = Grid::new(
            native,
            Some([1200, 800]),
            crate::render::Scale::new(2).unwrap(),
        )
        .unwrap();
        assert_eq!(sized.encode.output[..2], [1200, 800]);
    }
    let mut odd = encoding(0);
    odd.crop[0] += 1;
    assert!(Grid::new(odd, None, crate::render::Scale::new(2).unwrap()).is_ok());
    let mut misaligned = encoding(0);
    misaligned.source[0] -= 1;
    assert!(Grid::new(misaligned, None, crate::render::Scale::new(2).unwrap()).is_ok());
    misaligned.source[0] -= 1;
    assert!(Grid::new(misaligned, None, crate::render::Scale::new(2).unwrap()).is_ok());
}

#[test]
fn arbitrary_sensor_scales_cover_edges_and_align_every_reconstruction_ratio() {
    use bytemuck::Zeroable;
    for divisor in [2, 4, 8, 16, 32, 64, 256, 1 << 31] {
        let scale = Scale::new(divisor).unwrap();
        let mut native = encoding(0);
        native.source[0] -= 1;
        native.crop[0] += 1;
        let grid = Grid::new(native, None, scale).unwrap();
        assert_eq!(grid.grid.logical, scale.extent([4927, 3264]));
        assert!(grid.grid.extent.into_iter().all(|v| v.is_multiple_of(8)));
        assert_eq!(grid.encode.output[..2], scale.extent([4704, 3136]));
        assert_eq!(grid.reduction.working_scale(), Some(scale));
        for reduction in [0.375, 0.5, 0.625, 0.75, 0.875, 1.] {
            let mut params = crate::noise::Parameters::zeroed();
            params.source = [grid.grid.native[0], grid.grid.native[1], 4, 4];
            params.tile = [1360, 944, 64, 21];
            params.area = [112, 64, 4816, 3200];
            params.geometry = [8, 8, 11, 0];
            params.column[1] = 39.;
            params.column[2] = 40.;
            params.model = [[2.; 4], [3.; 4], [4.; 4]];
            params.mono = [2., 3., 4., 0.];
            params.filter = [2., 3., 4., 0.9];
            params.edge = [2., 3., 4., 100.];
            params.reduction[0] = reduction;
            let working = grid.noise(params).unwrap();
            assert_eq!(working.source[..2], grid.grid.extent);
            assert_eq!(working.source[2..], [1, 1]);
            assert_eq!(working.column[2], 40_u32.div_ceil(divisor).max(1) as f32);
            let std = pool::independent_noise_std(scale);
            for (native, actual) in params.model.iter().zip(working.model) {
                assert_eq!(actual, native.map(|v| v * (std * std)));
            }
            assert_eq!(working.mono, [2. * std, 3. * std, 4. * std, 0.]);
            assert_eq!(working.filter[..3], params.filter[..3]);
            assert_eq!(working.filter[3], params.filter[3].powf(divisor as f32));
            if divisor <= 64 {
                assert!(
                    (working.filter[3].powf(1. / divisor as f32) - params.filter[3]).abs() < 1e-6
                );
            }
            assert_eq!(
                working.sampling,
                [divisor, 32_u32.div_ceil(divisor).max(1), 0, 0]
            );
            assert_eq!(working.edge, params.edge);
            assert!(working.reduced_tile_extent().is_ok());
        }
    }
}

#[test]
fn native_regions_support_color_monochrome_and_camera_rotations() {
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
        assert!(settings.regional_preview_available(&metadata));
    }
    settings.rendering = crate::render::Rendering::Monochrome;
    assert!(settings.regional_preview_available(&metadata));
    settings.rendering = crate::render::Rendering::Color;
    settings.output_size = Some([1200, 800]);
    assert!(!settings.regional_preview_available(&metadata));
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn reduced_render_matches_cached_edits_and_keeps_full_images_and_inspection_native() {
    use crate::{
        inspection::Content,
        profile::{Mode, Profile},
        render::{Engine, NativePhoto, Scale, Settings},
        source::Source,
    };
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = std::sync::Arc::new(Source::open(root.join("blue-parking-sign.x3f")).unwrap());
    let engine = Engine::test_executor();
    let mut session = NativePhoto::new(source.clone());
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    let saved_edits = crate::sidecar::encode(&settings).unwrap();
    let full = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    let full_bytes = engine.readback_image(&full).unwrap();
    settings.working_scale = Scale::new(2).unwrap();
    let mut profile = Profile::new(Mode::Wall);
    let fast = engine.render(&source, &settings, &mut profile).unwrap();
    let fast_bytes = engine.readback_image(&fast).unwrap();
    for stage in [
        "submit reference noise base",
        "submit reference column conditioning",
        "submit reference luminance ratio",
        "submit texture restoration",
        "submit tiled ColorDQ",
        "submit color tile projection",
    ] {
        assert!(profile.host.iter().any(|s| s.stage == stage), "{stage}");
    }
    assert!(profile
        .host
        .iter()
        .any(|s| s.stage == "submit row banding correction"));
    assert!(
        fast.settings().row_correction,
        "preview policy must retain the authored edit"
    );
    assert_eq!(
        crate::sidecar::encode(fast.settings()).unwrap(),
        saved_edits
    );
    assert_eq!(fast.description().extent, {
        let [left, top, right, bottom] = full.view().crop;
        [right - left, bottom - top].map(|v| v.div_ceil(2))
    });
    assert_eq!(
        fast.view().crop,
        [
            0,
            0,
            fast.description().extent[0],
            fast.description().extent[1]
        ]
    );
    assert_eq!(fast.view().extent, full.view().extent.map(|v| v / 2));
    assert!(bytemuck::cast_slice::<u8, f32>(&fast_bytes)
        .iter()
        .all(|v| v.is_finite()));
    let mut bypassed = settings.clone();
    bypassed.reconstruction = None;
    let unchecked = engine
        .render(&source, &bypassed, &mut Profile::default())
        .unwrap();
    assert_ne!(engine.readback_image(&unchecked).unwrap(), fast_bytes);
    let mut stronger = settings.clone();
    stronger.reconstruction.as_mut().unwrap().luminance = 4;
    let stronger = engine
        .render(&source, &stronger, &mut Profile::default())
        .unwrap();
    assert_ne!(engine.readback_image(&stronger).unwrap(), fast_bytes);
    for step in 0..4 {
        match step {
            1 => {
                settings.exposure = 0.5;
                settings.saturation = 0.7;
                settings.reconstruction.as_mut().unwrap().texture.fine = 0.9;
            }
            2 => settings.working_scale = Scale::NATIVE,
            3 => settings.working_scale = Scale::new(2).unwrap(),
            _ => {}
        }
        let mut artifacts = engine
            .render_native_photo(
                &mut session,
                &settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        let Content::Image(cached) = artifacts.remove(0).content else {
            panic!("RGB")
        };
        let fresh = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(
            engine.readback_image(&cached).unwrap(),
            engine.readback_image(&fresh).unwrap(),
            "quality/edit step {step}"
        );
    }
    assert_eq!(engine.readback_image(&full).unwrap(), full_bytes);
    assert_eq!(engine.readback_image(&fast).unwrap(), fast_bytes);
    let mut artifacts = engine
        .render_artifacts(
            &source,
            &settings,
            &["calibration.layers"],
            &mut Profile::default(),
        )
        .unwrap();
    let Content::Image(inspection) = artifacts.remove(0).content else {
        panic!("layers")
    };
    let native_extent = [source.metadata.width as u32, source.metadata.height as u32];
    assert_eq!(inspection.description().extent, native_extent);
    settings.reconstruction.as_mut().unwrap().texture.coarse = 0.25;
    let mut profile = Profile::new(Mode::Wall);
    let dng = engine
        .render_dng_base(&source, &settings, &mut profile)
        .unwrap();
    assert_eq!(dng.description().extent, native_extent);
    assert!(profile
        .host
        .iter()
        .any(|s| s.stage == "submit texture restoration"));
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn row_correction_follows_sensor_pooling_for_color_and_monochrome() {
    use crate::{
        graph::Operation,
        interactive::Execution,
        profile::Profile,
        render::{Engine, Rendering, Scale, Settings},
        source::Source,
    };
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = Source::open(root.join("blue-parking-sign.x3f")).unwrap();
    let engine = Engine::test_executor();
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        for divisor in [1, 2, 4, 8] {
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.reconstruction = Some(Default::default());
            settings.rendering = rendering;
            settings.despeckle = false;
            settings.working_scale = Scale::new(divisor).unwrap();
            let recipe = crate::render::recipes::merrill::technical::build(
                &engine,
                crate::render::recipes::RecipeInput::Original(&source),
                &settings,
                &["final"],
                Some("Standard"),
                &mut Profile::default(),
                &mut Execution::default(),
            )
            .unwrap();
            let recipe = match recipe {
                crate::render::recipes::merrill::technical::Prepared::Requested(recipe)
                | crate::render::recipes::merrill::technical::Prepared::Linear(recipe, _, _) => {
                    recipe
                }
            };
            let nodes: Vec<_> = recipe
                .graph
                .nodes
                .iter()
                .filter(|node| matches!(node.operation, Operation::Banding(_)))
                .collect();
            assert_eq!(nodes.len(), 1);
            let producer = recipe.graph.outputs[nodes[0].inputs[0]].producer;
            let Operation::Highlights(highlights) = &recipe.graph.nodes[producer].operation else {
                panic!("row input must include highlight recovery")
            };
            assert_eq!(highlights.grid.map_or(1, |g| g.scale.divisor()), divisor);
            let input = recipe.graph.nodes[producer].inputs[0];
            let before = recipe.graph.outputs[input].producer;
            assert_eq!(
                matches!(
                    recipe.graph.nodes[before].operation,
                    Operation::ReduceLayers { .. }
                ),
                divisor > 1
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn speckle_inputs_share_the_requested_working_grid() {
    use crate::{
        graph::{DataType, Operation},
        interactive::Execution,
        profile::Profile,
        render::{Engine, MerrillSpeckleMethod, Rendering, Scale, Settings},
        source::Source,
    };
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = Source::open(root.join("blue-parking-sign.x3f")).unwrap();
    let engine = Engine::test_executor();
    for divisor in [1, 2, 4, 8, 64, 1024] {
        for (rendering, method) in [
            (Rendering::Color, MerrillSpeckleMethod::Reference),
            (Rendering::Color, MerrillSpeckleMethod::Compact),
            (Rendering::Monochrome, MerrillSpeckleMethod::Reference),
            (Rendering::Monochrome, MerrillSpeckleMethod::Compact),
        ] {
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.rendering = rendering;
            settings.working_scale = Scale::new(divisor).unwrap();
            settings.merrill_speckles.method = method;
            let recipe = super::super::technical::build(
                &engine,
                crate::render::recipes::RecipeInput::Original(&source),
                &settings,
                &["despeckle.layers"],
                Some("Standard"),
                &mut Profile::default(),
                &mut Execution::default(),
            )
            .unwrap();
            let super::super::technical::Prepared::Requested(recipe) = recipe else {
                panic!("speckle inspection must stop construction");
            };
            let node = recipe
                .graph
                .nodes
                .iter()
                .find(|node| matches!(node.operation, Operation::Despeckle(_)))
                .unwrap();
            let Operation::Despeckle(prepared) = &node.operation else {
                unreachable!()
            };
            assert_eq!(
                node.inputs.len(),
                if method == MerrillSpeckleMethod::Compact {
                    2
                } else {
                    1
                }
            );
            for (&input, data) in node
                .inputs
                .iter()
                .zip([DataType::CalibratedLayers, DataType::SensorLayers])
            {
                let output = &recipe.graph.outputs[input];
                assert_eq!(output.data, data);
                match &recipe.graph.nodes[output.producer].operation {
                    Operation::ReduceLayers {
                        prepared: reduced, ..
                    } => {
                        assert!(divisor > 1);
                        assert_eq!(reduced.working_scale(), Some(settings.working_scale));
                        assert_eq!(reduced.output_extent(), prepared.params.source[..2]);
                    }
                    Operation::Highlights(highlights) => {
                        assert_eq!(highlights.grid.map_or(1, |g| g.scale.divisor()), divisor);
                        let extent = highlights
                            .grid
                            .map_or(highlights.params.extent, |g| g.extent);
                        assert_eq!(extent, prepared.params.source[..2]);
                    }
                    _ => assert_eq!(divisor, 1, "native input reached reduced speckle node"),
                }
            }
        }
    }
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
        assert_eq!(region.rgb_bounds, [0, 0, 308, 308]);
        assert_eq!(region.encode.output[..2], [300, 300]);
        assert_eq!(region.encode.source[2], rotation);
    }
    assert!(super::super::region::Prepared::new(encoding(0), [0, 0, 0, 10]).is_err());
    assert!(super::super::region::Prepared::new(encoding(0), [0, 0, 99999, 10]).is_err());
    let edge = super::super::region::Prepared::new(encoding(0), [0, 0, 100, 100]).unwrap();
    assert_eq!(edge.encode.crop, [4, 4, 104, 104]);
    assert_eq!(edge.rgb_bounds, [4, 4, 108, 108]);
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn regional_detail_matches_full_and_continues_without_repeating_whole_sensor_inputs() {
    use crate::{
        inspection::Content,
        profile::{Mode, Profile},
        render::{Engine, NativePhoto, Scale, Settings},
        source::Source,
    };
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::test_executor();
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
            let mut session = NativePhoto::new(source.clone());
            settings.working_scale = Scale::new(2).unwrap();
            let overview = engine
                .render_native_photo(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap();
            settings.working_scale = Scale::NATIVE;
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
                    .render_native_photo(
                        &mut session,
                        &settings,
                        &["final"],
                        || false,
                        &mut profile,
                    )
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
                    .render_native_photo(
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
        render::{Engine, NativePhoto, Settings},
        source::Source,
    };
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = std::sync::Arc::new(Source::open(root.join("blue-parking-sign.x3f")).unwrap());
    let engine = Engine::test_executor();
    let mut settings = Settings::new("Sunlight", "Standard");
    // Isolate tile filtering. Speckle correction now follows the Noise bypass,
    // while a recipe without reconstruction can still select sensor corrections.
    settings.despeckle = false;
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
    let mut session = NativePhoto::new(source.clone());
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
            .render_native_photo(
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
    let mut neutral = settings
        .processing_settings(crate::source::ProcessingFamily::Merrill)
        .into_owned();
    neutral.tone_enabled = true;
    neutral.sharpening_enabled = true;
    let expected = engine
        .render(&source, &neutral, &mut Profile::default())
        .unwrap();
    assert!(engine.readback_image(&image).unwrap() == engine.readback_image(&expected).unwrap());
}

#[test]
fn draft_and_thumb_preserve_sensor_geometry_and_layer_types() {
    use crate::render::Scale;
    let mut native = encoding(90);
    // Thumb must retain an active edge at a half reduced pixel.
    native.crop[1] += 4;
    for quality in [Scale::new(4).unwrap(), Scale::new(8).unwrap()] {
        let grid = Grid::new(native, None, quality).unwrap();
        let divisor = quality.divisor();
        assert_eq!(grid.reduction.working_scale(), Some(quality));
        assert_eq!(grid.reduction.output_extent(), grid.grid.extent);
        assert_eq!(
            grid.encode.output[..2],
            native.output[..2]
                .iter()
                .map(|v| v / divisor)
                .collect::<Vec<_>>()
        );
        let mut graph = crate::graph::Graph::default();
        let source = graph.add(crate::graph::Operation::Source, &[]).unwrap();
        // Reducing original codes must not relabel them as calibrated layers.
        let layers = graph
            .add(
                crate::graph::Operation::ImageAlias {
                    dehaze_basis: None,
                    data: crate::graph::DataType::SensorLayers,
                    output: "test.source",
                },
                &[source],
            )
            .unwrap();
        let reduced = grid.append(&mut graph, layers).unwrap();
        assert_eq!(
            graph.outputs[reduced].data,
            crate::graph::DataType::SensorLayers
        );
        assert!(
            graph
                .add(
                    crate::graph::Operation::ImageAlias {
                        dehaze_basis: None,
                        data: crate::graph::DataType::CalibratedLayers,
                        output: "test.calibrated",
                    },
                    &[reduced]
                )
                .is_err(),
            "reduction does not replace calibration"
        );
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR"]
fn color_field_recovery_inputs_share_the_sensor_working_grid() {
    use crate::{
        graph::{DataType, Operation},
        interactive::Execution,
        profile::Profile,
        render::{ColorFieldApproach, Engine, HighlightMode, Rendering, Scale, Settings},
        source::Source,
    };
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = Source::open(root.join("yellow-flowers-and-stroller.x3f")).unwrap();
    let engine = Engine::test_executor();
    for rendering in [Rendering::Color, Rendering::Monochrome] {
        for approach in [
            ColorFieldApproach::Faithful,
            ColorFieldApproach::LocalGradient,
        ] {
            for divisor in [1, 2, 4, 8, 16, 64] {
                let mut settings = Settings::new("Sunlight", "Standard");
                settings.rendering = rendering;
                settings.working_scale = Scale::new(divisor).unwrap();
                settings.highlight_mode = HighlightMode::ColorField;
                settings.color_field.approach = approach;
                let result = crate::render::recipes::merrill::technical::build(
                    &engine,
                    crate::render::recipes::RecipeInput::Original(&source),
                    &settings,
                    &["highlight.layers"],
                    Some("Standard"),
                    &mut Profile::default(),
                    &mut Execution::default(),
                )
                .unwrap();
                let crate::render::recipes::merrill::technical::Prepared::Requested(recipe) =
                    result
                else {
                    panic!("highlight inspection must stop")
                };
                let node = recipe
                    .graph
                    .nodes
                    .iter()
                    .find(|n| matches!(n.operation, Operation::ColorField(_)))
                    .unwrap();
                let Operation::ColorField(p) = &node.operation else {
                    unreachable!()
                };
                assert_eq!(
                    recipe.graph.outputs[node.inputs[2]].data,
                    DataType::BlackLevels
                );
                assert!(recipe.graph.sensor_data_outputs.contains(&node.inputs[2]));
                for index in [0, 1, 3, 4] {
                    let output = &recipe.graph.outputs[node.inputs[index]];
                    match &recipe.graph.nodes[output.producer].operation {
                        Operation::ReduceLayers { prepared, .. } => {
                            assert!(divisor > 1);
                            assert_eq!(prepared.working_scale(), Some(settings.working_scale));
                            assert_eq!(prepared.output_extent(), p.params.size[..2]);
                            assert!(matches!(prepared.signal(), pool::Signal::Rgb));
                        }
                        _ => assert_eq!(divisor, 1, "native image reached reduced Color Field"),
                    }
                }
            }
        }
    }
}
