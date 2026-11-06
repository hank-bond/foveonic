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
fn small_grids_preserve_fractional_active_edges_and_orientation() {
    for rotation in [0, 90, 180, 270] {
        for quality in [PreviewQuality::Draft, PreviewQuality::Thumb] {
            let native = native(rotation);
            let grid = Grid::new(native, quality).unwrap();
            let d = quality.divisor();
            assert_eq!(grid.extent(), [256 / d, 192 / d]);
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
                rgb_extent: grid.extent(),
                rgb_bounds: grid.encode.crop,
            };
            grid.frame(&mut recipe).unwrap();
            assert_eq!(recipe.params.source[2], rotation);
            let operation =
                &recipe.graph.nodes[recipe.graph.outputs[recipe.final_id].producer].operation;
            let Operation::ResampleRgb { params, .. } = operation else {
                panic!("missing native frame")
            };
            assert_eq!(params.mapping[0], [242. / d as f32, 0., 7. / d as f32, 1.]);
            assert_eq!(params.mapping[1], [0., 182. / d as f32, 5. / d as f32, 1.]);
            assert_eq!(params.output[..2], [242 / d, 182 / d]);
        }
    }
    let mut odd = native(0);
    odd.source[0] += 1;
    assert_eq!(
        Grid::new(odd, PreviewQuality::Thumb).unwrap().extent(),
        [33, 24]
    );
    assert!(Grid::new(native(0), PreviewQuality::Fast).is_err());
}

#[test]
fn unvalidated_classic_routes_report_full_fallback() {
    use crate::source::{Matrix, MatrixEncoding};
    let matrix = |values: Vec<f64>| Matrix {
        encoding: MatrixEncoding::Float32,
        representation: "test",
        dimensions: vec![values.len()],
        values,
    };
    let mut metadata = Metadata {
        width: 256,
        height: 192,
        rotation: 0,
        processing_version: None,
        properties: [
            ("CAMMODEL".into(), "SIGMA SD9".into()),
            ("ISO".into(), "100".into()),
        ]
        .into(),
        matrices: [("ActiveImageArea".into(), matrix(vec![7., 5., 248., 186.]))].into(),
        lists: Default::default(),
    };
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    assert!(settings.small_preview_available(&metadata, PreviewQuality::Thumb));
    metadata.properties.insert("ISO".into(), "400".into());
    assert!(!settings.small_preview_available(&metadata, PreviewQuality::Thumb));
    metadata.properties.insert("ISO".into(), "100".into());
    metadata
        .matrices
        .insert("ChromaBlur".into(), matrix(vec![101.]));
    assert!(!settings.small_preview_available(&metadata, PreviewQuality::Thumb));
    settings.reconstruction = None;
    assert!(settings.small_preview_available(&metadata, PreviewQuality::Thumb));
    settings.rendering = crate::render::Rendering::Monochrome;
    assert!(!settings.small_preview_available(&metadata, PreviewQuality::Thumb));
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR originals and a GPU"]
fn classic_small_preview_originals_keep_controls_and_match_fresh_sessions() {
    use crate::{
        inspection::Content,
        profile::Profile,
        render::{ControlCapabilities, Engine, RenderSession, Rendering},
        source::{ProcessingFamily, Source},
    };
    let directory = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::new().unwrap();
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
        let mut session = RenderSession::new(source.clone());
        for quality in [PreviewQuality::Draft, PreviewQuality::Thumb] {
            settings.preview_quality = quality;
            if !available(&source.metadata, &settings, quality) {
                continue;
            }
            supported += 1;
            let full = super::super::prepare::geometry(&source.metadata).unwrap();
            let recipe = super::super::build(
                &engine,
                &source,
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
                .position(|node| matches!(node.operation, Operation::ReduceLayers(_)))
                .unwrap();
            let calibration = recipe
                .graph
                .nodes
                .iter()
                .position(|node| matches!(node.operation, Operation::FixedPointCalibration(_)))
                .unwrap();
            let color = recipe
                .graph
                .nodes
                .iter()
                .position(|node| matches!(node.operation, Operation::FixedPointColor(_)))
                .unwrap();
            assert!(calibration < first_pool && first_pool < color);
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
                    assert_eq!(
                        root.params.shape[..2],
                        [
                            (source.metadata.width as u32).div_ceil(quality.divisor()),
                            (source.metadata.height as u32).div_ceil(quality.divisor())
                        ]
                    );
                }
            }

            for edit in 0..4 {
                settings.exposure = if edit == 1 { 0.25 } else { 0. };
                settings.reconstruction = if edit == 2 { None } else { requested };
                let cached = engine
                    .render_session(
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
                assert_eq!(
                    cached.settings.preview_quality,
                    quality,
                    "{}",
                    path.display()
                );
                assert_eq!(cached.settings.reconstruction, settings.reconstruction);
                assert_eq!(
                    cached.view().extent,
                    [
                        full.output[0] / quality.divisor(),
                        full.output[1] / quality.divisor()
                    ]
                );
                let bytes = engine.readback_image(cached).unwrap();
                assert_eq!(
                    bytes,
                    engine.readback_image(fresh).unwrap(),
                    "{} {quality:?} edit {edit}",
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
        "expected validated Classic Draft and Thumb originals"
    );
}
