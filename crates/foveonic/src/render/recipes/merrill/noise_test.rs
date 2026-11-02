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
        let image = cached(&engine, &mut session, &settings);
        assert_same(
            "monochrome native method cached/fresh",
            &engine.readback_image(&image).unwrap(),
            &pixels(&engine, &source, &settings),
        );
        assert_eq!(settings.merrill_noise_method, method);
    }
    settings.merrill_noise_method = MerrillNoiseMethod::Reference;
    assert_same(
        "monochrome Reference restored",
        &pixels(&engine, &source, &settings),
        &reference,
    );
    settings.processing_sections.noise = false;
    let bypass = pixels(&engine, &source, &settings);
    for method in METHODS {
        settings.merrill_noise_method = method;
        assert_same(
            "monochrome Noise section bypass",
            &pixels(&engine, &source, &settings),
            &bypass,
        );
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with original Merrill files"]
fn monochrome_native_methods_keep_layer_order_and_shared_export_preparation() {
    use crate::nodes::monochrome::Stage;
    use crate::render::MerrillBandingMethod;
    let source = fixture();
    let engine = Engine::new().unwrap();
    let mut settings = settings();
    settings.rendering = Rendering::Monochrome;
    for method in [MerrillNoiseMethod::Reference, METHODS[0], METHODS[1]] {
        settings.merrill_noise_method = method;
        for banding in [
            None,
            Some(MerrillBandingMethod::Reference),
            Some(MerrillBandingMethod::Directional),
        ] {
            settings.merrill_banding.enabled = banding.is_some();
            settings.merrill_banding.method = banding.unwrap_or(MerrillBandingMethod::Reference);
            let photo = photographic(&engine, &source, &settings);
            let dng = super::dng_base::build(
                &engine,
                &source,
                &settings,
                &mut Profile::default(),
                &mut Execution::default(),
            )
            .unwrap();
            let mix = position(&photo, |op| {
                matches!(
                    op,
                    Operation::Monochrome {
                        stage: Stage::Mix,
                        ..
                    }
                )
            });
            let histogram = position(&photo, |op| {
                matches!(
                    op,
                    Operation::Monochrome {
                        stage: Stage::Histogram,
                        ..
                    }
                )
            });
            assert!(
                mix < histogram,
                "native filtering retains the scalar luminance baseline"
            );
            let native = method != MerrillNoiseMethod::Reference;
            let directional = banding == Some(MerrillBandingMethod::Directional);
            assert_eq!(
                photo
                    .graph
                    .nodes
                    .iter()
                    .any(|n| matches!(n.operation, Operation::Multiscale(_))),
                native
            );
            assert_eq!(
                photo
                    .graph
                    .nodes
                    .iter()
                    .any(|n| matches!(n.operation, Operation::RowGuard(_))),
                directional
            );
            assert_eq!(
                photo
                    .graph
                    .nodes
                    .iter()
                    .any(|n| matches!(n.operation, Operation::DirectionalBanding { .. })),
                directional
            );
            assert_eq!(
                photo.graph.nodes.iter().any(|n| matches!(
                    n.operation,
                    Operation::Monochrome {
                        stage: Stage::Column,
                        ..
                    }
                )),
                banding == Some(MerrillBandingMethod::Reference)
            );
            assert_eq!(
                photo
                    .graph
                    .nodes
                    .iter()
                    .any(|n| matches!(n.operation, Operation::Banding(_))),
                settings.reference_rows_selected()
            );
            if native {
                let filter = position(&photo, |op| matches!(op, Operation::Multiscale(_)));
                let calibrate = position(&photo, |op| matches!(op, Operation::Calibrate(_)));
                assert!(filter < calibrate && calibrate < mix);
            }
            if directional {
                let local = position(&photo, final_directional_banding);
                assert!(local < mix);
                assert_eq!(
                    photo.graph.nodes[mix].inputs,
                    [photo.graph.nodes[local].outputs[1]]
                );
                assert_eq!(
                    photo.graph.outputs[photo.graph.nodes[local].outputs[1]].data,
                    crate::graph::DataType::CalibratedLayers
                );
            }
            for name in [
                "shield-noise.layers",
                "row-guard.layers",
                "multiscale.layers",
                "column.layers",
                "mono.mix",
                "mono.filtered",
                "monochrome.rgb",
            ] {
                if !photo.graph.outputs.iter().any(|output| output.name == name) {
                    continue;
                }
                let key = |recipe: &BuiltRecipe| {
                    let output = recipe
                        .graph
                        .outputs
                        .iter()
                        .find(|o| o.name == name)
                        .unwrap();
                    recipe.graph.nodes[output.producer].operation.key(name)
                };
                assert!(key(&photo) == key(&dng), "photo/DNG differ at {name}");
            }

            let mut lower = settings.clone();
            lower.monochrome.banding = 0;
            let mut upper = settings.clone();
            upper.monochrome.banding = 4;
            let lower = photographic(&engine, &source, &lower);
            let upper = photographic(&engine, &source, &upper);
            let guide = |recipe: &BuiltRecipe| {
                let index = position(recipe, |op| {
                    matches!(
                        op,
                        Operation::Monochrome {
                            stage: Stage::Directional,
                            ..
                        }
                    )
                });
                let Operation::Monochrome { prepared, .. } = &recipe.graph.nodes[index].operation
                else {
                    unreachable!()
                };
                (
                    prepared.params.source[2],
                    recipe.graph.nodes[index].operation.key("guide"),
                )
            };
            let low_guide = guide(&lower);
            let high_guide = guide(&upper);
            if banding == Some(MerrillBandingMethod::Reference) {
                assert_eq!([low_guide.0, high_guide.0], [15, 39]);
                assert!(
                    low_guide.1 != high_guide.1,
                    "selected Reference amount must change its guide"
                );
            } else {
                assert_eq!([low_guide.0, high_guide.0], [27, 27]);
                assert!(
                    low_guide.1 == high_guide.1,
                    "inactive Reference amount changed its guide"
                );
                assert_eq!(lower.graph.nodes.len(), upper.graph.nodes.len());
                for (low, high) in lower.graph.nodes.iter().zip(&upper.graph.nodes) {
                    assert!(
                        low.operation.key("inactive") == high.operation.key("inactive"),
                        "inactive Reference amount changed {}",
                        low.operation.label()
                    );
                    assert_eq!(low.inputs, high.inputs);
                }
            }
        }
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

fn final_directional_banding(operation: &Operation) -> bool {
    use crate::nodes::color::directional_banding::{Axis, Band, Stage};
    matches!(
        operation,
        Operation::DirectionalBanding {
            stage: Stage::Correct(Band::Coarse, Axis::Rows),
            ..
        }
    )
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
fn directional_banding_controls_share_native_exports_and_cached_results() {
    let source = fixture();
    let engine = Engine::new().unwrap();
    let mut settings = settings();
    settings.merrill_noise_method = MerrillNoiseMethod::MultiscaleRows;
    settings.merrill_banding.enabled = true;
    settings.merrill_banding.method = crate::render::MerrillBandingMethod::Directional;
    let full = photographic(&engine, &source, &settings);
    let shield = position(&full, |op| matches!(op, Operation::ShieldNoise(_)));
    let guard = position(&full, |op| matches!(op, Operation::RowGuard(_)));
    let filter = position(&full, |op| matches!(op, Operation::Multiscale(_)));
    let local = position(&full, final_directional_banding);
    assert!(shield < guard && guard < filter && filter < local);
    let dng = super::dng_base::build(
        &engine,
        &source,
        &settings,
        &mut Profile::default(),
        &mut Execution::default(),
    )
    .unwrap();
    for index in [guard, local] {
        let is_guard = index == guard;
        let other = position(&dng, |op| {
            if is_guard {
                matches!(op, Operation::RowGuard(_))
            } else {
                final_directional_banding(op)
            }
        });
        assert!(
            full.graph.nodes[index].operation.key("shared")
                == dng.graph.nodes[other].operation.key("shared")
        );
    }
    settings.preview_quality = PreviewQuality::Fast;
    let fast = photographic(&engine, &source, &settings);
    assert!(
        position(&fast, |op| matches!(op, Operation::RowGuard(_)))
            < position(&fast, |op| matches!(op, Operation::ReduceLayers(_)))
    );
    let fast_bands = position(&fast, final_directional_banding);
    assert!(
        full.graph.nodes[local].operation.key("native bands")
            == fast.graph.nodes[fast_bands].operation.key("native bands")
    );
    settings.preview_quality = PreviewQuality::Draft;
    let draft = photographic(&engine, &source, &settings);
    assert!(draft.graph.nodes.iter().all(|node| !matches!(
        node.operation,
        Operation::RowGuard(_) | Operation::DirectionalBanding { .. }
    )));

    settings.preview_quality = PreviewQuality::Full;
    settings.output_size = Some([240, 160]);
    let mut session = RenderSession::new(source.clone());
    let retained = cached(&engine, &mut session, &settings);
    let retained_bytes = engine.readback_image(&retained).unwrap();
    for (axis, strength, size) in [(0, 1., 63), (1, 0.5, 79), (0, 0., 63)] {
        let bands = if axis == 0 {
            &mut settings.merrill_banding.rows.bands
        } else {
            &mut settings.merrill_banding.columns.bands
        };
        bands[1].strength = strength;
        bands[1].size = size;
        let actual = engine
            .readback_image(&cached(&engine, &mut session, &settings))
            .unwrap();
        assert_same(
            "band-control cache switch",
            &actual,
            &pixels(&engine, &source, &settings),
        );
        assert_same(
            "retained banding image",
            &engine.readback_image(&retained).unwrap(),
            &retained_bytes,
        );
    }
    settings.merrill_banding.rows.bands[1].strength = 0.8;
    cached(&engine, &mut session, &settings);
    settings.merrill_banding.rows.bands[1].strength = 0.7;
    let mut profile = Profile::new(crate::profile::Mode::Wall);
    engine
        .render_session(&mut session, &settings, &["final"], || false, &mut profile)
        .unwrap()
        .unwrap();
    assert!(profile
        .host
        .iter()
        .any(|t| t.stage == "correct native coarse row bands"));
    assert!(!profile.host.iter().any(|t| matches!(
        t.stage,
        "prepare directional band tiles"
            | "measure native fine column bands"
            | "measure native fine row bands"
            | "measure native coarse column bands"
            | "measure native coarse row bands"
            | "correct native fine column bands"
            | "correct native fine row bands"
            | "correct native coarse column bands"
    )));

    // Once a strength edit establishes its frontier, further nonzero changes
    // must reuse measured offsets rather than rerun either bilateral filter.
    settings.merrill_banding.rows.bands[0].strength = 0.8;
    cached(&engine, &mut session, &settings);
    settings.merrill_banding.rows.bands[0].strength = 0.7;
    let mut profile = Profile::new(crate::profile::Mode::Wall);
    let edited = engine
        .render_session(&mut session, &settings, &["final"], || false, &mut profile)
        .unwrap()
        .unwrap();
    assert!(profile
        .host
        .iter()
        .any(|t| t.stage == "correct native fine row bands"));
    assert!(!profile.host.iter().any(|t| matches!(
        t.stage,
        "prepare directional band tiles"
            | "measure native fine row bands"
            | "measure native fine column bands"
            | "correct native fine column bands"
    )));
    let crate::inspection::Content::Image(edited) = &edited[0].content else {
        panic!("expected image")
    };
    assert_same(
        "cached band strength",
        &engine.readback_image(edited).unwrap(),
        &pixels(&engine, &source, &settings),
    );

    settings.merrill_banding.rows.bands[0].size = 25;
    let mut profile = Profile::new(crate::profile::Mode::Wall);
    let edited = engine
        .render_session(&mut session, &settings, &["final"], || false, &mut profile)
        .unwrap()
        .unwrap();
    assert!(profile
        .host
        .iter()
        .any(|t| t.stage == "measure native fine row bands"));
    assert!(!profile.host.iter().any(|t| matches!(
        t.stage,
        "prepare directional band tiles"
            | "measure native fine column bands"
            | "correct native fine column bands"
    )));
    let crate::inspection::Content::Image(edited) = &edited[0].content else {
        panic!("expected image")
    };
    assert_same(
        "cached band support",
        &engine.readback_image(edited).unwrap(),
        &pixels(&engine, &source, &settings),
    );
    settings.merrill_noise_method = MerrillNoiseMethod::Reference;
    let banding_only = photographic(&engine, &source, &settings);
    let _ = position(&banding_only, final_directional_banding);
    let _ = position(&banding_only, |op| matches!(op, Operation::RowGuard(_)));
    assert!(banding_only
        .graph
        .nodes
        .iter()
        .all(|node| !matches!(node.operation, Operation::Multiscale(_))));
    settings.merrill_banding.enabled = false;
    let reference = pixels(&engine, &source, &settings);
    settings.merrill_banding.rows.bands[1].strength = 2.;
    assert_same(
        "reference ignores inactive band controls",
        &reference,
        &pixels(&engine, &source, &settings),
    );
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with original Merrill files"]
fn multiscale_regional_views_match_whole_frame() {
    let source = fixture();
    let engine = Engine::new().unwrap();
    let mut settings = settings();
    let rect = [600, 800, 900, 1100];
    for (method, banding) in METHODS
        .into_iter()
        .map(|method| (method, false))
        .chain([(MerrillNoiseMethod::MultiscaleRows, true)])
    {
        settings.merrill_noise_method = method;
        settings.merrill_banding.enabled = true;
        settings.merrill_banding.method = if banding {
            crate::render::MerrillBandingMethod::Directional
        } else {
            crate::render::MerrillBandingMethod::Reference
        };
        settings.merrill_banding.columns.bands[1].strength = if banding { 1. } else { 0. };
        settings.merrill_banding.rows.bands[1].strength = if banding { 1. } else { 0. };
        settings.neutralize_shadows.strength = if banding { 0.8 } else { 0. };
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
        let geometry = super::region::Prepared::with_finish_support(
            encode,
            rect,
            super::super::linear_edits::support(&settings) + super::region::FINISH_SUPPORT,
            1,
        )
        .unwrap();
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

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with original Merrill files"]
fn speckle_method_threshold_and_size_cached_results_match_fresh() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let mut session = RenderSession::new(source.clone());
    let mut selected = settings();
    let original = pixels(&engine, &source, &selected);
    for (method, threshold, size) in [
        (crate::render::MerrillSpeckleMethod::Compact, 0.25, 5),
        (crate::render::MerrillSpeckleMethod::Compact, 0.4, 7),
        (crate::render::MerrillSpeckleMethod::Reference, 1., 3),
    ] {
        selected.merrill_speckles = crate::render::MerrillSpeckleControls {
            method,
            threshold,
            size,
        };
        let image = cached(&engine, &mut session, &selected);
        let data = engine.readback_image(&image).unwrap();
        assert_same(
            "speckle cached/fresh",
            &data,
            &pixels(&engine, &source, &selected),
        );
    }
    assert_same(
        "Reference speckles restored",
        &pixels(&engine, &source, &selected),
        &original,
    );
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with original Merrill files"]
fn neutralize_shadows_cached_switches_and_neutral_bypass_match_fresh() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let mut session = RenderSession::new(source.clone());
    let mut selected = settings();
    let baseline = pixels(&engine, &source, &selected);
    selected.neutralize_shadows.strength = 0.8;
    let photo = photographic(&engine, &source, &selected);
    let dng = super::dng_base::build(
        &engine,
        &source,
        &selected,
        &mut Profile::default(),
        &mut Execution::default(),
    )
    .unwrap();
    let index = position(&photo, |op| {
        matches!(op, Operation::ShadowNeutralize { .. })
    });
    let other = position(&dng, |op| matches!(op, Operation::ShadowNeutralize { .. }));
    assert!(
        photo.graph.nodes[index].operation.key("shared")
            == dng.graph.nodes[other].operation.key("shared")
    );

    for strength in [0.8, 0.4, 0.] {
        selected.neutralize_shadows.strength = strength;
        let image = cached(&engine, &mut session, &selected);
        assert_same(
            "shadow cached/fresh",
            &engine.readback_image(&image).unwrap(),
            &pixels(&engine, &source, &selected),
        );
    }
    selected.neutralize_shadows.strength = 0.8;
    cached(&engine, &mut session, &selected);
    selected.neutralize_shadows.color_tolerance = 0.4;
    let mut profile = Profile::new(crate::profile::Mode::Wall);
    engine
        .render_session(&mut session, &selected, &["final"], || false, &mut profile)
        .unwrap()
        .unwrap();
    assert!(profile
        .host
        .iter()
        .any(|t| t.stage == "neutralize green and magenta shadows"));
    assert!(!profile
        .host
        .iter()
        .any(|t| t.stage == "prepare regional shadow color"));
    selected.neutralize_shadows.strength = 0.;
    assert_same(
        "shadow zero bypass",
        &pixels(&engine, &source, &selected),
        &baseline,
    );
    selected.rendering = Rendering::Monochrome;
    let mono = pixels(&engine, &source, &selected);
    selected.neutralize_shadows.strength = 1.;
    assert_same(
        "monochrome ignores shadow chroma",
        &pixels(&engine, &source, &selected),
        &mono,
    );
}
