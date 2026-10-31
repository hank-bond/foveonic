//! Checks multiscale method switching, bypasses, native preview order, and shared export preparation.

use super::super::BuiltRecipe;
use crate::{
    color_space::ColorSpace,
    graph::Operation,
    image::RenderedImage,
    inspection::Content,
    interactive::Execution,
    nodes::sensor::calibration,
    profile::Profile,
    render::{Engine, MerrillNoiseMethod, PreviewQuality, RenderSession, Rendering, Settings},
    source::Source,
};
use std::{path::PathBuf, sync::Arc};

const METHODS: [MerrillNoiseMethod; 2] = [
    MerrillNoiseMethod::MultiscaleRows,
    MerrillNoiseMethod::MultiscaleRowsAndColumns,
];

fn fixture() -> Arc<Source> {
    let directory = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    Arc::new(Source::open(directory.join("blue-parking-sign.x3f")).unwrap())
}

fn settings() -> Settings {
    Settings {
        reconstruction: Some(crate::noise::Controls {
            chroma: 3,
            ..Default::default()
        }),
        ..Settings::new("Sunlight", "Standard")
    }
}

fn cached(engine: &Engine, session: &mut RenderSession, settings: &Settings) -> RenderedImage {
    let mut artifacts = engine
        .render_session(
            session,
            settings,
            &["final"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    let Content::Image(image) = artifacts.remove(0).content else {
        panic!("expected final image")
    };
    *image
}

fn pixels(engine: &Engine, source: &Source, settings: &Settings) -> Vec<u8> {
    let image = engine
        .render(source, settings, &mut Profile::default())
        .unwrap();
    engine.readback_image(&image).unwrap()
}

fn assert_same(label: &str, actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len(), "{label}: sample count");
    if actual == expected {
        return;
    }
    let mut differing = 0;
    let mut maximum = 0_f32;
    let mut first = None;
    for (index, (actual, expected)) in actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_chunks::<4>().0)
        .enumerate()
    {
        if actual != expected {
            let actual = f32::from_le_bytes(*actual);
            let expected = f32::from_le_bytes(*expected);
            first.get_or_insert((index, actual, expected));
            maximum = maximum.max((actual - expected).abs());
            differing += 1;
        }
    }
    panic!("{label}: {differing} differing float components, maximum={maximum}, first={first:?}");
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with original Merrill files"]
fn multiscale_cached_switches_and_inactive_bypasses_match_fresh() {
    let source = fixture();
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source.clone());
    let mut settings = settings();
    settings.output_size = Some([240, 160]);
    let retained = cached(&engine, &mut session, &settings);
    let retained_pixels = engine.readback_image(&retained).unwrap();
    for method in [
        METHODS[0],
        METHODS[1],
        METHODS[0],
        MerrillNoiseMethod::Reference,
    ] {
        settings.merrill_noise_method = method;
        let image = cached(&engine, &mut session, &settings);
        assert_same(
            &format!("{method:?} cached/fresh"),
            &engine.readback_image(&image).unwrap(),
            &pixels(&engine, &source, &settings),
        );
    }
    assert_same(
        "retained image",
        &engine.readback_image(&retained).unwrap(),
        &retained_pixels,
    );

    let controls = settings.reconstruction.as_mut().unwrap();
    controls.noise_enabled = false;
    controls.texture.fine = 1.2;
    let reference = pixels(&engine, &source, &settings);
    for method in METHODS {
        settings.merrill_noise_method = method;
        let image = cached(&engine, &mut session, &settings);
        assert_same(
            "Noise off retains independent Detail",
            &engine.readback_image(&image).unwrap(),
            &reference,
        );
        assert_eq!(settings.merrill_noise_method, method);
    }
    settings.rendering = Rendering::Monochrome;
    settings.reconstruction.as_mut().unwrap().noise_enabled = true;
    settings.merrill_noise_method = MerrillNoiseMethod::Reference;
    let reference = pixels(&engine, &source, &settings);
    for method in METHODS {
        settings.merrill_noise_method = method;
        assert_same(
            "monochrome bypass",
            &pixels(&engine, &source, &settings),
            &reference,
        );
        assert_eq!(settings.merrill_noise_method, method);
    }
}

fn photographic(engine: &Engine, source: &Source, settings: &Settings) -> BuiltRecipe {
    super::photographic::build(
        engine,
        source,
        settings,
        &["final"],
        &mut Profile::default(),
        &mut Execution::default(),
    )
    .unwrap()
}

fn position(recipe: &BuiltRecipe, predicate: impl Fn(&Operation) -> bool) -> usize {
    recipe
        .graph
        .nodes
        .iter()
        .position(|node| predicate(&node.operation))
        .unwrap()
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with original Merrill files"]
fn multiscale_preview_order_and_export_preparation_share_native_inputs() {
    let source = fixture();
    let engine = Engine::new().unwrap();
    let mut settings = settings();
    for method in METHODS {
        settings.merrill_noise_method = method;
        settings.preview_quality = PreviewQuality::Full;
        let full = photographic(&engine, &source, &settings);
        let shield = position(&full, |op| matches!(op, Operation::ShieldNoise(_)));
        let filter = position(&full, |op| matches!(op, Operation::Multiscale(_)));
        let calibration = position(&full, |op| matches!(op, Operation::Calibrate(_)));
        assert!(shield < filter && filter < calibration);
        let source_node = position(&full, |op| matches!(op, Operation::Source));
        let original = full.graph.nodes[source_node].outputs[0];
        let black = position(&full, |op| matches!(op, Operation::Black(_)));
        let black = full.graph.nodes[black].outputs[0];
        assert_eq!(full.graph.nodes[shield].inputs, [original, original, black]);
        assert_eq!(
            full.graph.nodes[filter].inputs,
            [full.graph.nodes[shield].outputs[0], original, black]
        );
        let repair = position(&full, |op| matches!(op, Operation::Repair(_)));
        assert!(full.graph.outputs[full.graph.nodes[repair].outputs[0]]
            .dependencies
            .contains(&original));

        let dng = super::dng_base::build(
            &engine,
            &source,
            &settings,
            &mut Profile::default(),
            &mut Execution::default(),
        )
        .unwrap();
        for (photo, other) in [
            (
                shield,
                position(&dng, |op| matches!(op, Operation::ShieldNoise(_))),
            ),
            (
                filter,
                position(&dng, |op| matches!(op, Operation::Multiscale(_))),
            ),
        ] {
            assert!(
                full.graph.nodes[photo].operation.key("native")
                    == dng.graph.nodes[other].operation.key("native")
            );
        }

        settings.preview_quality = PreviewQuality::Fast;
        let fast = photographic(&engine, &source, &settings);
        let filter = position(&fast, |op| matches!(op, Operation::Multiscale(_)));
        let reduction = position(&fast, |op| matches!(op, Operation::ReduceLayers(_)));
        assert!(
            filter < reduction,
            "Fast must filter the native sensor first"
        );
        settings.preview_quality = PreviewQuality::Draft;
        let draft = photographic(&engine, &source, &settings);
        assert!(draft.graph.nodes.iter().all(|node| !matches!(
            node.operation,
            Operation::ShieldNoise(_) | Operation::Multiscale(_)
        )));
        let _ = position(&draft, |op| matches!(op, Operation::ReduceLayers(_)));
        assert_eq!(settings.merrill_noise_method, method);
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with original Merrill files"]
fn multiscale_regional_views_match_whole_frame() {
    let source = fixture();
    let engine = Engine::new().unwrap();
    let mut settings = settings();
    let rect = [600, 800, 900, 1100];
    for method in METHODS {
        settings.merrill_noise_method = method;
        settings.preview_region = None;
        let full = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let full_bytes = engine.readback_image(&full).unwrap();
        settings.preview_region = Some(rect);
        let regional = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let local_bytes = engine.readback_image(&regional).unwrap();
        let encode = calibration::encode(
            &source.metadata,
            ColorSpace::from_metadata(&source.metadata).unwrap(),
        )
        .unwrap();
        let geometry = super::region::Prepared::new(encode, rect).unwrap();
        let local: &[f32] = bytemuck::cast_slice(&local_bytes);
        let expected: &[f32] = bytemuck::cast_slice(&full_bytes);
        let [x0, y0, x1, y1] = regional.view().crop;
        let mut maximum = 0_f32;
        for y in y0..y1 {
            for x in x0..x1 {
                let dst = ((y * regional.description().extent[0] + x) * 3) as usize;
                let src = (((y + geometry.physical[1]) * full.description().extent[0]
                    + x
                    + geometry.physical[0])
                    * 3) as usize;
                for c in 0..3 {
                    maximum = maximum.max((local[dst + c] - expected[src + c]).abs());
                }
            }
        }
        assert!(maximum < 0.00001, "regional multiscale mismatch: {maximum}");
    }
}
