//! Checks reduced sensor invalidation, exact downstream reuse and bounded resource ownership.

use super::*;

#[test]
fn sensor_identity_tracks_upstream_controls_and_leaves_tail_edits_reusable() {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.preview_quality = PreviewQuality::Thumb;
    settings.reconstruction = Some(Default::default());
    let base = upstream(&settings).unwrap();
    for case in 0..7 {
        let mut changed = settings.clone();
        match case {
            0 => changed.white_balance = "Shade".into(),
            1 => changed.repair = None,
            2 => changed.highlight_threshold_ev = Some(-1.),
            3 => changed.highlight_restoration = false,
            4 => changed.despeckle = false,
            5 => {
                changed.merrill_noise_method =
                    super::super::MerrillNoiseMethod::MultiscaleRowsAndColumns
            }
            _ => changed.reconstruction.as_mut().unwrap().banding = 4,
        }
        assert_ne!(base, upstream(&changed).unwrap(), "upstream case {case}");
    }
    for case in 0..5 {
        let mut changed = settings.clone();
        match case {
            0 => changed.exposure = 0.5,
            1 => changed.reconstruction.as_mut().unwrap().chroma = 4,
            2 => changed.reconstruction.as_mut().unwrap().luminance = 4,
            3 => changed.reconstruction.as_mut().unwrap().texture.fine = 1.5,
            _ => changed.tone.contrast = 0.3,
        }
        assert_eq!(base, upstream(&changed).unwrap(), "tail case {case}");
    }
    settings.corrections_enabled = false;
    let bypassed = upstream(&settings).unwrap();
    settings.repair = None;
    settings.highlight_threshold_ev = Some(1.);
    // Threshold is still deliberately conservative in the small identity.
    assert_ne!(bypassed, upstream(&settings).unwrap());
}

#[test]
fn unsupported_native_branches_request_original_explicitly() {
    use crate::source::{Matrix, MatrixEncoding};
    let metadata = Metadata {
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
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.preview_quality = PreviewQuality::Thumb;
    settings.reconstruction = Some(Default::default());
    assert_eq!(requirements(&metadata, &settings, &["final"]), None);
    assert_eq!(
        requirements(&metadata, &settings, &["linear.base"]),
        Some(NeedsOriginal::NativeOutput)
    );
    let mut changed = settings.clone();
    changed.dehaze.amount = 0.5;
    assert_eq!(
        requirements(&metadata, &changed, &["final"]),
        Some(NeedsOriginal::NativeAuxiliaryInput)
    );
    changed = settings.clone();
    changed.merrill_banding.enabled = true;
    changed.merrill_banding.method = super::super::MerrillBandingMethod::Directional;
    assert_eq!(
        requirements(&metadata, &changed, &["final"]),
        Some(NeedsOriginal::NativeAuxiliaryInput)
    );
    changed = settings;
    changed.preview_quality = PreviewQuality::Full;
    assert_eq!(
        requirements(&metadata, &changed, &["final"]),
        Some(NeedsOriginal::NativeOutput)
    );
}

fn rendered(outcome: ReducedRender) -> RenderedImage {
    let ReducedRender::Rendered(mut artifacts) = outcome else {
        panic!("expected reduced image");
    };
    let crate::inspection::Content::Image(image) = artifacts.pop().unwrap().content else {
        panic!("expected image");
    };
    *image
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn reduced_session_releases_native_work_and_matches_fresh_tail_edits() {
    let path = std::env::var_os("SMALL_PREVIEW_X3F").unwrap();
    let source = Source::open(&path).unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    settings.preview_quality = PreviewQuality::Thumb;
    let engine = Engine::new().unwrap();
    let native_bytes = source.samples().len() as u64 * 4;
    let ReducedPreparation::Prepared(mut session) = engine
        .prepare_reduced_session(source, &settings, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("expected reduced preparation");
    };
    assert!(session.cached_bytes() < native_bytes / 16);
    assert_eq!(
        session.sensor.description.space,
        crate::image::Space::CalibratedLayers
    );
    assert_eq!(session.cache.bytes(), 0);
    assert!(engine.cached_preparation_bytes() < 1024 * 1024);
    for case in 0..6 {
        match case {
            0 => {}
            1 => settings.exposure = 0.5,
            2 => settings.reconstruction.as_mut().unwrap().chroma = 4,
            3 => settings.reconstruction.as_mut().unwrap().luminance = 4,
            4 => settings.reconstruction.as_mut().unwrap().texture.fine = 1.5,
            _ => settings.tone.contrast = 0.3,
        }
        let image = rendered(
            engine
                .render_reduced_session(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap(),
        );
        let bytes = engine.readback_image(&image).unwrap();
        let verifier = engine.executor();
        let original = Source::open(&path).unwrap();
        let fresh = verifier
            .render(&original, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(image.description(), fresh.description(), "case {case}");
        assert_eq!(image.view(), fresh.view(), "case {case}");
        assert_eq!(
            bytes,
            verifier.readback_image(&fresh).unwrap(),
            "case {case}"
        );
        assert!(session.cached_bytes() < native_bytes / 4);
    }
    for case in 0..6 {
        let mut changed = settings.clone();
        match case {
            0 => changed.white_balance = "Shade".into(),
            1 => changed.highlight_threshold_ev = Some(-1.),
            2 => changed.despeckle = false,
            3 => changed.reconstruction.as_mut().unwrap().banding = 4,
            4 => changed.preview_quality = PreviewQuality::Full,
            _ => changed.preview_quality = PreviewQuality::Draft,
        }
        assert!(
            matches!(
                engine
                    .render_reduced_session(
                        &mut session,
                        &changed,
                        &["final"],
                        || false,
                        &mut Profile::default()
                    )
                    .unwrap(),
                ReducedRender::NeedsOriginal(_)
            ),
            "case {case}"
        );
    }
    assert!(matches!(
        engine
            .render_reduced_session(
                &mut session,
                &settings,
                &["final"],
                || true,
                &mut Profile::default()
            )
            .unwrap(),
        ReducedRender::Superseded
    ));
    assert!(matches!(
        engine
            .executor()
            .render_reduced_session(
                &mut session,
                &settings,
                &["final"],
                || false,
                &mut Profile::default()
            )
            .unwrap(),
        ReducedRender::NeedsOriginal(NeedsOriginal::EngineChanged)
    ));
    for case in 0..8 {
        let mut changed = settings.clone();
        match case {
            0 => {
                changed.white_balance = session
                    .metadata()
                    .white_balance_presets()
                    .into_iter()
                    .find(|name| *name != settings.white_balance)
                    .unwrap()
                    .to_owned()
            }
            1 => changed.highlight_threshold_ev = Some(-1.),
            2 => changed.repair = None,
            3 => changed.despeckle = false,
            4 => {
                changed.merrill_noise_method =
                    super::super::MerrillNoiseMethod::MultiscaleRowsAndColumns
            }
            5 => changed.reconstruction.as_mut().unwrap().banding = 4,
            6 => changed.processing_sections.noise = false,
            _ => changed.corrections_enabled = false,
        }
        let ReducedPreparation::Prepared(mut rebuilt) = engine
            .prepare_reduced_session(
                Source::open(&path).unwrap(),
                &changed,
                || false,
                &mut Profile::default(),
            )
            .unwrap()
        else {
            panic!("expected upstream reseal {case}");
        };
        let image = rendered(
            engine
                .render_reduced_session(
                    &mut rebuilt,
                    &changed,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap(),
        );
        let verifier = engine.executor();
        let fresh = verifier
            .render(
                &Source::open(&path).unwrap(),
                &changed,
                &mut Profile::default(),
            )
            .unwrap();
        assert_eq!(
            engine.readback_image(&image).unwrap(),
            verifier.readback_image(&fresh).unwrap(),
            "upstream reseal {case}"
        );
    }
    let original = Source::open(&path).unwrap();
    let samples = original.samples().as_ptr();
    let native_len = original.samples().len();
    let mut native_request = settings;
    native_request.preview_quality = PreviewQuality::Full;
    let ReducedPreparation::NeedsOriginal { reason, source } = engine
        .prepare_reduced_session(original, &native_request, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("expected original fallback ownership");
    };
    assert_eq!(reason, NeedsOriginal::NativeOutput);
    assert_eq!(source.samples().as_ptr(), samples);
    assert_eq!(source.samples().len(), native_len);
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn working_set_evicts_after_insertion_and_cache_growth() {
    let path = std::env::var_os("SMALL_PREVIEW_X3F").unwrap();
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.preview_quality = PreviewQuality::Thumb;
    settings.reconstruction = Some(Default::default());
    let prepare = || {
        let ReducedPreparation::Prepared(session) = engine
            .prepare_reduced_session(
                Source::open(&path).unwrap(),
                &settings,
                || false,
                &mut Profile::default(),
            )
            .unwrap()
        else {
            panic!("expected reduced preparation");
        };
        session
    };
    let first = prepare();
    let duplicate = || {
        Box::new(ReducedSession {
            sensor: first.sensor.clone(),
            upstream: first.upstream.clone(),
            calibration: first.calibration.clone(),
            cache: Cache::default(),
            metadata_bytes: first.metadata_bytes,
        })
    };
    let mut aliases = ReducedWorkingSet::new(u64::MAX);
    aliases.insert(10, duplicate(), &engine).unwrap();
    aliases.insert(11, duplicate(), &engine).unwrap();
    assert_eq!(aliases.retained_gpu_bytes(), first.cached_bytes());
    assert_eq!(
        aliases.retained_cpu_bytes(),
        first.retained_cpu_bytes() * 2 - first.metadata_bytes
    );
    drop(aliases);
    let limit = first.cached_bytes() + first.retained_cpu_bytes() + 1024;
    let mut working = ReducedWorkingSet::new(limit);
    assert!(working.insert(1, first, &engine).unwrap().is_empty());
    assert_eq!(working.insert(2, prepare(), &engine).unwrap(), vec![1]);
    assert!(working.contains(2));
    assert!(working.retained_bytes(&engine) <= limit);
    let image = rendered(
        working
            .render(&engine, 2, &settings, || false, &mut Profile::default())
            .unwrap()
            .unwrap(),
    );
    assert!(
        !working.contains(2),
        "growing cache must enforce the same budget"
    );
    assert_eq!(
        working.retained_bytes(&engine),
        engine.cached_preparation_bytes() as u64
    );
    assert!(
        !engine.readback_image(&image).unwrap().is_empty(),
        "eviction must preserve delivered images"
    );
    let mut roomy = ReducedWorkingSet::new(u64::MAX);
    for id in [1, 2, 3] {
        roomy.insert(id, prepare(), &engine).unwrap();
    }
    roomy
        .render(&engine, 1, &settings, || false, &mut Profile::default())
        .unwrap()
        .unwrap();
    roomy.limit = roomy.retained_bytes(&engine) - 1;
    assert_eq!(
        roomy.enforce(&engine),
        vec![2],
        "rendering refreshes LRU recency"
    );
    assert!(roomy.contains(1) && roomy.contains(3));
    roomy.limit = 0;
    roomy.enforce(&engine);
    assert!(roomy.is_empty());
    assert_eq!(
        roomy.retained_bytes(&engine),
        0,
        "undersized empty budget releases engine preparation"
    );
}
