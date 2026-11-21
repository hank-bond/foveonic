//! Checks signed sensor pooling, fractional camera framing and reduced Classic session consistency.

use super::*;

fn native(rotation: u32) -> EncodeParams {
    EncodeParams {
        mapping: [[0.; 4]; 2],
        source: [256, 192, rotation, 0],
        crop: [7, 5, 249, 187],
        output: if matches!(rotation, 90 | 270) {
            [182, 242, 0, 0]
        } else {
            [242, 182, 0, 0]
        },
    }
}

#[test]
fn grids_cover_all_scales_and_preserve_fractional_camera_framing() {
    for rotation in [0, 90, 180, 270] {
        for power in 1..32 {
            let scale = Scale::new(1 << power).unwrap();
            let native = native(rotation);
            let grid = Grid::new(native, scale, None, 8).unwrap();
            let d = scale.divisor() as f32;
            assert_eq!(grid.grid.logical, scale.extent([256, 192]));
            assert!(grid
                .extent()
                .into_iter()
                .zip(grid.grid.logical)
                .all(|(extent, logical)| extent >= logical + 16));
            let mut graph = Graph::default();
            let layers = graph
                .add(
                    Operation::RenderedInput {
                        data: crate::graph::DataType::CalibratedLayers,
                    },
                    &[],
                )
                .unwrap();
            let pooled = grid.append(&mut graph, layers).unwrap();
            assert_eq!(
                graph.outputs[pooled].data,
                crate::graph::DataType::CalibratedLayers
            );
            assert_eq!(
                graph
                    .nodes
                    .iter()
                    .filter(|node| matches!(node.operation, Operation::ReduceLayers { .. }))
                    .count(),
                1
            );
            let rgb = graph
                .add(
                    Operation::RenderedInput {
                        data: crate::graph::DataType::LinearRgb(ColorSpace::ProPhotoRgb),
                    },
                    &[],
                )
                .unwrap();
            let mut recipe = BuiltRecipe {
                graph,
                final_id: rgb,
                params: grid.encode,
                rgb_to_native: [[1., 0., 0.], [0., 1., 0.]],
                rgb_extent: grid.extent(),
                rgb_bounds: grid.encode.crop,
            };
            grid.frame(&mut recipe).unwrap();
            assert_eq!(recipe.params.source[2], rotation);
            let operation =
                &recipe.graph.nodes[recipe.graph.outputs[recipe.final_id].producer].operation;
            let Operation::ResampleRgb { params, .. } = operation else {
                panic!("missing photographic frame")
            };
            let expected = [
                [242. / d, 0., 7. / d + 7.5, 1.],
                [0., 182. / d, 5. / d + 7.5, 1.],
            ];
            for (actual, expected) in params
                .mapping
                .into_iter()
                .flatten()
                .zip(expected.into_iter().flatten())
            {
                assert!((actual - expected).abs() < 0.000001);
            }
            // Rotation is retained for presentation; this camera frame is unrotated.
            assert_eq!(params.output[..2], scale.extent([242, 182]));
        }
    }
    let mut odd = native(0);
    odd.source[0] += 1;
    let grid = Grid::new(odd, Scale::new(8).unwrap(), Some([19, 13]), 8).unwrap();
    assert_eq!(grid.grid.logical, [33, 24]);
    assert_eq!(grid.encode.output[..2], [19, 13]);
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR originals and a GPU"]
fn classic_small_preview_originals_keep_controls_and_match_fresh_sessions() {
    use crate::{
        inspection::Content,
        profile::Profile,
        render::{ControlCapabilities, Engine, NativePhoto, Rendering},
        source::{ProcessingFamily, Source},
    };
    let directory = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::test_executor();
    let mut supported = 0;
    for file in std::fs::read_dir(directory).unwrap() {
        let path = file.unwrap().path();
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("x3f"))
        {
            continue;
        }
        let source = std::sync::Arc::new(Source::open(&path).unwrap());
        if source.metadata.processing_family().unwrap() != ProcessingFamily::Classic {
            continue;
        }
        let capabilities = ControlCapabilities::from_metadata(&source.metadata).unwrap();
        let mut settings = capabilities
            .editing_defaults(&source.metadata, Rendering::Color)
            .unwrap();
        settings.white_balance = capabilities
            .default_white_balance(&source.metadata)
            .unwrap();
        settings.color_mode = capabilities.default_color_mode(&source.metadata).unwrap();
        settings.reconstruction = Some(Default::default());
        let requested = settings.reconstruction;
        let mut session = NativePhoto::new(source.clone());
        for divisor in [2, 4, 8, 16] {
            let scale = Scale::new(divisor).unwrap();
            settings.working_scale = scale;
            supported += 1;
            eprintln!("Classic {scale:?} original: {}", path.display());
            let full = super::super::prepare::geometry(&source.metadata).unwrap();
            let recipe = super::super::build(
                &engine,
                crate::render::recipes::RecipeInput::Original(&source),
                &settings,
                &["final"],
                &mut Profile::default(),
                &mut crate::interactive::Execution::default(),
            )
            .unwrap();
            let first_pool = recipe
                .graph
                .nodes
                .iter()
                .position(|node| matches!(node.operation, Operation::ReduceLayers { .. }))
                .unwrap();
            let calibration = recipe
                .graph
                .nodes
                .iter()
                .position(|node| matches!(node.operation, Operation::FixedPointCalibration(_)))
                .unwrap();
            let color = recipe.graph.nodes.iter().position(|node| {
                matches!(
                    node.operation,
                    Operation::FixedPointColor(_) | Operation::RangeReconstruct(_)
                )
            });
            assert!(calibration < first_pool);
            if let Some(color) = color {
                assert!(first_pool < color);
            }
            for (position, node) in recipe.graph.nodes.iter().enumerate() {
                if matches!(
                    node.operation,
                    Operation::SignedRepair(_)
                        | Operation::SignedDespeckle(_)
                        | Operation::RedDetail(_)
                        | Operation::HighlightStream(_)
                ) {
                    assert!(
                        position < first_pool,
                        "native correction moved after pooling"
                    );
                }
                if let Operation::RootDetail(root) = &node.operation {
                    assert!(root.params.shape[..2]
                        .iter()
                        .zip(
                            scale.extent([
                                source.metadata.width as u32,
                                source.metadata.height as u32,
                            ])
                        )
                        .all(|(actual, logical)| *actual >= logical));
                }
            }

            for edit in 0..4 {
                settings.exposure = if edit == 1 { 0.25 } else { 0. };
                settings.reconstruction = if edit == 2 { None } else { requested };
                let cached = engine
                    .render_native_photo(
                        &mut session,
                        &settings,
                        &["final"],
                        || false,
                        &mut Profile::default(),
                    )
                    .unwrap()
                    .unwrap();
                let fresh = engine
                    .render_artifacts(&source, &settings, &["final"], &mut Profile::default())
                    .unwrap();
                let (Content::Image(cached), Content::Image(fresh)) =
                    (&cached[0].content, &fresh[0].content)
                else {
                    panic!("missing photograph")
                };
                assert_eq!(cached.settings.working_scale, scale, "{}", path.display());
                assert_eq!(cached.settings.reconstruction, settings.reconstruction);
                assert_eq!(
                    cached.view().extent,
                    [
                        full.output[0].div_ceil(scale.divisor()),
                        full.output[1].div_ceil(scale.divisor())
                    ]
                );
                let bytes = engine.readback_image(cached).unwrap();
                assert_eq!(
                    bytes,
                    engine.readback_image(fresh).unwrap(),
                    "{} {scale:?} edit {edit}",
                    path.display()
                );
                assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
                    .iter()
                    .all(|v| v.is_finite()));
            }
        }
    }
    assert!(
        supported >= 2,
        "expected Classic originals at reduced scales"
    );
}
