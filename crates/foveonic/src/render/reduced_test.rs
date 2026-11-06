//! Checks source-free recipe cuts, full-option outputs and bounded ownership.

use super::*;

fn rendered(outcome: ReducedRender) -> crate::image::RenderedImage {
    let ReducedRender::Rendered(mut artifacts) = outcome else {
        panic!("expected source-free image");
    };
    let crate::inspection::Content::Image(image) = artifacts.pop().unwrap().content else {
        panic!("expected image");
    };
    *image
}

fn full_option_cases(
    base: &Settings,
    caps: &crate::render::ControlCapabilities,
) -> Vec<(&'static str, Settings)> {
    let mut cases = vec![("reset", base.clone())];
    let mut s = base.clone();
    s.photographic.base_response = super::super::BaseResponse::Natural;
    s.photographic.tone_method = super::super::PhotographicMethod::Universal;
    assert!(s.adaptive_tone_active());
    cases.push(("adaptive", s));
    let mut s = base.clone();
    s.dehaze.amount = 0.5;
    s.dehaze.shadow = None;
    s.dehaze_estimate = true;
    cases.push(("auto-haze", s));
    let mut s = base.clone();
    s.dehaze.amount = 0.5;
    s.dehaze.shadow = Some([0.35; 3]);
    cases.push(("manual-haze", s));
    let mut s = base.clone();
    s.photographic.color_editor.master.hue = 0.3;
    s.photographic.color_editor.bands[0].lightness = 0.2;
    cases.push(("color-editor", s));
    let mut s = base.clone();
    s.photographic
        .color_editor
        .advanced
        .push(super::super::AdvancedColorRange {
            width: 360.,
            adjustment: crate::nodes::finish::color_editor::Adjustment {
                lightness: -0.3,
                ..Default::default()
            },
            ..Default::default()
        });
    cases.push(("advanced-color", s));
    let mut s = base.clone();
    s.tonal_ranges.shadows = 0.3;
    s.tonal_ranges.highlights = -0.4;
    s.photographic.local_contrast = 0.2;
    s.photographic.sharpening.method = super::super::PhotographicMethod::Universal;
    s.photographic.sharpening.amount = 0.6;
    cases.push(("spatial-finishing", s));
    let mut s = base.clone();
    s.reconstruction.get_or_insert_with(Default::default);
    s.merrill_banding.enabled = true;
    s.merrill_banding.method = super::super::MerrillBandingMethod::Directional;
    assert!(s.directional_banding_selected());
    cases.push(("directional", s));
    let mut s = base.clone();
    s.reconstruction.get_or_insert_with(Default::default);
    s.merrill_noise_method = super::super::MerrillNoiseMethod::MultiscaleRowsAndColumns;
    assert!(s.multiscale_noise_selected());
    cases.push(("multiscale", s));
    let mut s = base.clone();
    s.rendering = super::super::Rendering::Monochrome;
    s.monochrome.texture.fine = 0.5;
    cases.push(("monochrome", s));
    let mut s = cases[1].1.clone();
    s.dehaze = cases[2].1.dehaze;
    s.dehaze_estimate = true;
    s.photographic.color_editor = cases[4].1.photographic.color_editor;
    s.photographic.color_editor.advanced = cases[5].1.photographic.color_editor.advanced;
    s.tonal_ranges = cases[6].1.tonal_ranges;
    s.photographic.local_contrast = 0.2;
    s.photographic.sharpening = cases[6].1.photographic.sharpening;
    cases.push(("combined", s.clone()));
    s.rendering = super::super::Rendering::Monochrome;
    s.monochrome.texture.fine = 0.5;
    cases.push(("mono-combined", s));
    let mut strong = cases[1].1.clone();
    strong.photographic.base_response = super::super::BaseResponse::Strong;
    assert!(strong.adaptive_tone_active());
    cases.push(("adaptive-strong", strong));
    let mut reconstruction = base.clone();
    reconstruction
        .reconstruction
        .get_or_insert_with(Default::default);
    cases.push(("reconstruction", reconstruction));
    let mut all = cases
        .iter()
        .find(|(name, _)| *name == "combined")
        .unwrap()
        .1
        .clone();
    all.photographic.base_response = super::super::BaseResponse::Strong;
    all.reconstruction.get_or_insert_with(Default::default);
    all.merrill_banding.enabled = true;
    all.merrill_banding.method = super::super::MerrillBandingMethod::Directional;
    all.merrill_noise_method = super::super::MerrillNoiseMethod::MultiscaleRows;
    assert!(
        all.adaptive_tone_active()
            && all.directional_banding_selected()
            && all.multiscale_noise_selected()
    );
    cases.push(("full-combined", all));
    let mut applicable = Vec::new();
    for (name, mut settings) in cases {
        caps.normalize(&mut settings);
        if !applicable.iter().any(|(_, previous)| *previous == settings) {
            applicable.push((name, settings));
        }
    }
    applicable
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn full_option_dependency_cuts_release_source_and_match_fresh() {
    let path = std::env::var_os("SMALL_PREVIEW_X3F").unwrap();
    let original = Source::open(&path).unwrap();
    let caps = super::super::ControlCapabilities::from_metadata(&original.metadata).unwrap();
    let mut base = caps
        .editing_defaults(&original.metadata, super::super::Rendering::Color)
        .unwrap();
    base.white_balance = caps.default_white_balance(&original.metadata).unwrap();
    base.color_mode = caps.default_color_mode(&original.metadata).unwrap();
    drop(original);
    let engine = Engine::new().unwrap();
    let scales = std::env::var("SMALL_PREVIEW_SCALES").unwrap_or_else(|_| "2,4,8,16,64".into());
    let selected = std::env::var("SMALL_PREVIEW_CASE").ok();
    let mut exercised = 0;
    for divisor in scales.split(',').map(|s| s.parse::<u32>().unwrap()) {
        base.working_scale = Scale::new(divisor).unwrap();
        for (name, mut settings) in full_option_cases(&base, &caps) {
            if selected.as_ref().is_some_and(|selected| selected != name) {
                continue;
            }
            exercised += 1;
            let original = Arc::new(Source::open(&path).unwrap());
            let weak = Arc::downgrade(&original);
            let ReducedPreparation::Prepared(mut session) = engine
                .prepare_reduced_session(original, &settings, || false, &mut Profile::default())
                .unwrap()
            else {
                panic!("preparation {name} at{divisor}");
            };
            assert!(
                weak.upgrade().is_none(),
                "source retained {name} at{divisor}"
            );
            // These fixtures fit the fixed-preparation allowance and repair
            // target. An oversized single catalogue may exceed the repair
            // target under its separately accounted current-bundle policy.
            assert!(
                engine.cached_preparation_bytes()
                    < 8 * 1024 * 1024 + super::super::preparation::REPAIR_BYTES
            );
            for edit in 0..4 {
                if edit == 1 {
                    settings.exposure += 0.35;
                }
                if edit == 2 {
                    settings.white_balance_bias.warmth = 0.2;
                }
                if edit == 3
                    && session.metadata().processing_family().unwrap()
                        == crate::source::ProcessingFamily::Merrill
                {
                    if let Some(controls) = &mut settings.reconstruction {
                        controls.chroma = 4;
                        controls.luminance = 4;
                        controls.texture.fine = 0.5;
                    }
                }
                let mut artifacts = match engine
                    .render_reduced_session(
                        &mut session,
                        &settings,
                        &["final"],
                        || false,
                        &mut Profile::default(),
                    )
                    .unwrap()
                {
                    ReducedRender::Rendered(artifacts) => artifacts,
                    _ => panic!("source-free render {name} scale{divisor} edit{edit}"),
                };
                let artifact = artifacts.pop().unwrap();
                let verifier = engine.executor();
                let mut expected = verifier
                    .render_artifacts(
                        &Source::open(&path).unwrap(),
                        &settings,
                        &["final"],
                        &mut Profile::default(),
                    )
                    .unwrap();
                let expected = expected.pop().unwrap();
                assert_eq!(
                    artifact.dehaze_estimate.map(|v| v.map(f32::to_bits)),
                    expected.dehaze_estimate.map(|v| v.map(f32::to_bits)),
                    "analysis {name} scale{divisor} edit{edit}"
                );
                let crate::inspection::Content::Image(image) = artifact.content else {
                    panic!("image");
                };
                let crate::inspection::Content::Image(fresh) = expected.content else {
                    panic!("fresh image");
                };
                assert_eq!(
                    image.settings.working_scale, settings.working_scale,
                    "actual scale {name} scale{divisor}"
                );
                assert_eq!(
                    image.description(),
                    fresh.description(),
                    "geometry {name} scale{divisor}"
                );
                assert_eq!(image.view(), fresh.view(), "view {name} scale{divisor}");
                assert_eq!(
                    engine.readback_image(&image).unwrap(),
                    verifier.readback_image(&fresh).unwrap(),
                    "pixels {name} scale{divisor} edit{edit}"
                );
            }
            if session.metadata().processing_family().unwrap()
                == crate::source::ProcessingFamily::Merrill
                && settings.highlight_restoration
            {
                let mut changed = settings.clone();
                changed.highlight_threshold_ev = Some(-1.);
                let before = engine.gpu_activity();
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
                        ReducedRender::NeedsOriginal(NeedsOriginal::UpstreamChanged)
                    ),
                    "real native dependency change {name} scale{divisor}"
                );
                assert_eq!(
                    engine.gpu_activity().dispatches,
                    before.dispatches,
                    "source preflight precedes numerical dispatch"
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
            eprintln!("exact source-free {name} scale{divisor}");
        }
    }
    assert!(exercised > 0, "no applicable option cases selected");
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn working_set_protects_active_edits_and_bounds_parked_history() {
    let path = std::env::var_os("SMALL_PREVIEW_X3F").unwrap();
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.working_scale = Scale::new(8).unwrap();
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
            panic!("preparation");
        };
        session
    };
    let first = prepare();
    let resources = first.resources();
    assert_eq!(
        resources.metadata_bytes
            + resources.owner_bytes
            + resources.cache_structure_bytes
            + resources.parameter_payload_bytes
            + resources.preparation_bytes,
        first.retained_cpu_bytes()
    );
    let limit = first.cached_bytes() + first.retained_cpu_bytes() + 1024;
    let mut working = ReducedWorkingSet::new(limit);
    assert!(working.insert(1, first, &engine).unwrap().is_empty());
    assert_eq!(working.historical_bytes(), 0);
    assert!(working.insert(2, prepare(), &engine).unwrap().is_empty());
    assert!(working.contains(1) && working.contains(2));
    let image = rendered(
        working
            .render(&engine, 2, &settings, || false, &mut Profile::default())
            .unwrap()
            .unwrap(),
    );
    assert!(
        working.contains(2),
        "active growth cannot evict its own restart inputs"
    );
    assert!(working.historical_bytes() <= limit);
    assert!(working.active_bytes() > 0);
    let evicted = working.park_active();
    assert!(
        evicted.contains(&1),
        "older photo leaves only after optional versions are trimmed"
    );
    assert!(working.historical_bytes() <= limit);
    assert!(
        !engine.readback_image(&image).unwrap().is_empty(),
        "eviction preserves delivered images"
    );
    let mut roomy = ReducedWorkingSet::new(u64::MAX);
    for id in [1, 2, 3] {
        roomy.insert(id, prepare(), &engine).unwrap();
    }
    roomy
        .render(&engine, 1, &settings, || false, &mut Profile::default())
        .unwrap()
        .unwrap();
    assert!(roomy.prefetch(4, prepare(), &engine).unwrap().is_empty());
    assert_eq!(
        roomy.active.as_ref().unwrap().id,
        1,
        "prefetch cannot replace the actual active photo"
    );
    let limit = roomy.historical_bytes() - 1;
    assert_eq!(
        roomy.set_limit(limit),
        vec![4],
        "speculative photo is colder than genuinely used history"
    );
    let limit = roomy.historical_bytes() - 1;
    assert_eq!(roomy.set_limit(limit), vec![2]);
    assert!(roomy.contains(1) && roomy.contains(3));
    roomy.limit = 0;
    roomy.park_active();
    assert!(roomy.is_empty());
    engine.clear_preparation();
    assert_eq!(roomy.retained_bytes(&engine), 0);

    let mut fast = settings;
    fast.working_scale = Scale::new(2).unwrap();
    let source = Arc::new(Source::open(&path).unwrap());
    let weak = Arc::downgrade(&source);
    let ReducedPreparation::Prepared(session) = engine
        .prepare_reduced_session(source, &fast, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("Fast preparation");
    };
    assert!(weak.upgrade().is_none());
    let cuts: Vec<_> = session.cache.sealed_buffers().cloned().collect();
    let mut tiny_history = ReducedWorkingSet::new(1);
    tiny_history.insert(7, session, &engine).unwrap();
    for exposure in [0., 0.2, 0.4] {
        fast.exposure = exposure;
        let image = rendered(
            tiny_history
                .render(&engine, 7, &fast, || false, &mut Profile::default())
                .unwrap()
                .unwrap(),
        );
        assert_eq!(image.settings.working_scale, fast.working_scale);
        assert!(tiny_history.contains(7));
        assert!(tiny_history.active_bytes() > 1);
        assert_eq!(tiny_history.historical_bytes(), 0);
        assert!(
            cuts.iter().all(|old| tiny_history
                .active
                .as_ref()
                .unwrap()
                .session
                .buffers()
                .any(|new| old.shares_storage(new))),
            "Fast edits preserve active sensor cuts"
        );
    }
    assert_eq!(tiny_history.park_active(), vec![7]);
    assert!(tiny_history.is_empty());
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn verified_reload_preserves_main_cuts_and_rejects_other_origins_before_mutation() {
    let path = std::env::var_os("SMALL_PREVIEW_X3F").unwrap();
    let original = Arc::new(Source::open(&path).unwrap());
    let weak = Arc::downgrade(&original);
    let engine = Engine::new().unwrap();
    let caps = super::super::ControlCapabilities::from_metadata(&original.metadata).unwrap();
    let mut settings = caps
        .editing_defaults(&original.metadata, super::super::Rendering::Color)
        .unwrap();
    settings.white_balance = caps.default_white_balance(&original.metadata).unwrap();
    settings.color_mode = caps.default_color_mode(&original.metadata).unwrap();
    settings.working_scale = Scale::new(8).unwrap();
    settings
        .reconstruction
        .get_or_insert_with(Default::default)
        .banding = 4;
    let ReducedPreparation::Prepared(mut session) = engine
        .prepare_reduced_session(original, &settings, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("preparation");
    };
    assert!(weak.upgrade().is_none());
    let _main = rendered(
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
    let main_buffers: Vec<_> = session.cache.sealed_buffers().cloned().collect();
    assert!(!main_buffers.is_empty());
    let old_metadata = session.metadata().clone();
    let old_bytes = session.cached_bytes();
    let mut unrelated = Source::open(&path).unwrap();
    unrelated.origin = SourceOrigin::default();
    assert!(engine
        .refresh_reduced_session(
            &mut session,
            &unrelated,
            &settings,
            || false,
            &mut Profile::default()
        )
        .is_err());
    assert_eq!(session.cached_bytes(), old_bytes);
    assert!(Arc::ptr_eq(session.metadata(), &old_metadata));
    drop(unrelated);
    settings.photographic.color_editor.master.hue = 0.3;
    let changed = engine
        .render_reduced_session(
            &mut session,
            &settings,
            &["final"],
            || false,
            &mut Profile::default(),
        )
        .unwrap();
    assert!(
        matches!(
            changed,
            ReducedRender::Rendered(_)
                | ReducedRender::NeedsOriginal(NeedsOriginal::UpstreamChanged)
        ),
        "a new guide uses the retained cut or requests its actually missing native dependencies"
    );
    let reload = Arc::new(Source::open(&path).unwrap());
    let reload_weak = Arc::downgrade(&reload);
    assert_eq!(&session.origin, reload.origin());
    assert!(!Arc::ptr_eq(session.metadata(), &reload.metadata));
    assert!(engine
        .refresh_reduced_session(
            &mut session,
            &reload,
            &settings,
            || false,
            &mut Profile::default()
        )
        .unwrap()
        .is_some());
    assert!(
        main_buffers
            .iter()
            .all(|old| session.buffers().any(|new| old.shares_storage(new))),
        "partial refresh preserves every existing valid main cut"
    );
    drop(reload);
    assert!(reload_weak.upgrade().is_none());
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
    let verifier = engine.executor();
    let fresh = verifier
        .render(
            &Source::open(&path).unwrap(),
            &settings,
            &mut Profile::default(),
        )
        .unwrap();
    assert_eq!(
        engine.readback_image(&image).unwrap(),
        verifier.readback_image(&fresh).unwrap()
    );
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn per_output_scale_and_reduced_roi_hint_match_whole_grid() {
    let path = std::env::var_os("SMALL_PREVIEW_X3F").unwrap();
    let source = Arc::new(Source::open(&path).unwrap());
    let weak = Arc::downgrade(&source);
    let caps = super::super::ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = caps
        .editing_defaults(&source.metadata, super::super::Rendering::Color)
        .unwrap();
    settings.white_balance = caps.default_white_balance(&source.metadata).unwrap();
    settings.color_mode = caps.default_color_mode(&source.metadata).unwrap();
    settings.working_scale = Scale::new(8).unwrap();
    settings.photographic.color_editor.master.lightness = 0.2;
    let mut region = settings.clone();
    region.preview_region = Some([100, 100, 300, 300]);
    let engine = Engine::new().unwrap();
    let native_names: &[&str] = match source.metadata.processing_family().unwrap() {
        crate::source::ProcessingFamily::Merrill | crate::source::ProcessingFamily::Classic => {
            &["sensor.layers", "calibration.layers"]
        }
        crate::source::ProcessingFamily::Quattro => &[
            "sensor.rgb",
            "sensor.blue",
            "extraction.rgb",
            "extraction.blue",
        ],
    };
    let mut names = native_names.to_vec();
    names.extend(["linear.base", "final"]);
    let mut artifacts = engine
        .render_artifacts(&source, &region, &names, &mut Profile::default())
        .unwrap();
    let artifact = artifacts.pop().unwrap();
    assert_eq!(artifact.settings.working_scale, settings.working_scale);
    assert!(
        artifact.settings.preview_region.is_none(),
        "reduced ROI hint reports whole-grid coverage"
    );
    let linear = artifacts.pop().unwrap();
    let expected_linear = engine
        .render_artifacts(
            &source,
            &settings,
            &["linear.base"],
            &mut Profile::default(),
        )
        .unwrap()
        .pop()
        .unwrap();
    let (
        crate::inspection::Content::Image(linear),
        crate::inspection::Content::Image(expected_linear),
    ) = (linear.content, expected_linear.content)
    else {
        panic!("linear images")
    };
    assert_eq!(linear.settings.working_scale, settings.working_scale);
    assert_eq!(
        linear.view(),
        expected_linear.view(),
        "working-grid framing is independent of later camera framing"
    );
    assert_eq!(
        engine.readback_image(&linear).unwrap(),
        engine.readback_image(&expected_linear).unwrap()
    );
    let mut native_settings = settings.clone();
    native_settings.working_scale = Scale::NATIVE;
    let native_expected = engine
        .render_artifacts(
            &source,
            &native_settings,
            native_names,
            &mut Profile::default(),
        )
        .unwrap();
    for (native, expected) in artifacts.into_iter().zip(native_expected) {
        assert_eq!(
            native.settings.working_scale,
            Scale::NATIVE,
            "{} reports its own grid",
            native.name
        );
        let (
            crate::inspection::Content::Image(native),
            crate::inspection::Content::Image(expected),
        ) = (native.content, expected.content)
        else {
            panic!("native images")
        };
        assert_eq!(native.description(), expected.description());
        assert_eq!(
            native.uncropped_view(),
            expected.uncropped_view(),
            "native output cannot inherit the small final frame"
        );
        assert_eq!(native.view(), expected.view());
        assert_eq!(
            engine.readback_image(&native).unwrap(),
            engine.readback_image(&expected).unwrap()
        );
    }
    let crate::inspection::Content::Image(image) = artifact.content else {
        panic!("image");
    };
    let expected = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    assert_eq!(
        engine.readback_image(&image).unwrap(),
        engine.readback_image(&expected).unwrap()
    );
    let mut cropped_native = native_settings;
    cropped_native.crop.bounds = [0.1, 0.1, 0.9, 0.9];
    let mut mixed_region = cropped_native.clone();
    mixed_region.preview_region = Some([100, 100, 300, 300]);
    let mixed = engine
        .render_artifacts(&source, &mixed_region, &names, &mut Profile::default())
        .unwrap()
        .pop()
        .unwrap();
    assert!(
        mixed.settings.preview_region.is_none(),
        "a whole mixed-output graph cannot claim regional coverage"
    );
    let crate::inspection::Content::Image(mixed) = mixed.content else {
        panic!("mixed photograph")
    };
    let whole = engine
        .render(&source, &cropped_native, &mut Profile::default())
        .unwrap();
    assert_eq!(
        mixed.view(),
        whole.view(),
        "ignored Native ROI must not suppress the saved crop"
    );
    assert_eq!(
        engine.readback_image(&mixed).unwrap(),
        engine.readback_image(&whole).unwrap()
    );
    let ReducedPreparation::Prepared(mut session) = engine
        .prepare_reduced_session(source, &region, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("ROI execution hint must not exclude source-free ownership");
    };
    assert!(weak.upgrade().is_none());
    let retained = rendered(
        engine
            .render_reduced_session(
                &mut session,
                &region,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap(),
    );
    assert_eq!(retained.settings.working_scale, settings.working_scale);
    assert!(retained.settings.preview_region.is_none());
    assert_eq!(
        engine.readback_image(&retained).unwrap(),
        engine.readback_image(&expected).unwrap()
    );
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn every_camera_white_balance_matches_source_free_and_fresh_at_native_and_reduced_scales() {
    let path = std::env::var_os("SMALL_PREVIEW_X3F").unwrap();
    let original = Source::open(&path).unwrap();
    let caps = super::super::ControlCapabilities::from_metadata(&original.metadata).unwrap();
    let mut settings = caps
        .editing_defaults(&original.metadata, super::super::Rendering::Color)
        .unwrap();
    settings.color_mode = caps.default_color_mode(&original.metadata).unwrap();
    let choices: Vec<_> = original
        .metadata
        .white_balance_presets()
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert!(!choices.is_empty());
    drop(original);
    let engine = Engine::new().unwrap();
    let scales = std::env::var("WHITE_BALANCE_SCALES").unwrap_or_else(|_| "1,2,8,64".into());
    for divisor in scales.split(',').map(|s| s.parse::<u32>().unwrap()) {
        settings.working_scale = Scale::new(divisor).unwrap();
        for white_balance in &choices {
            settings.white_balance = white_balance.clone();
            let source = Arc::new(Source::open(&path).unwrap());
            let weak = Arc::downgrade(&source);
            let ReducedPreparation::Prepared(mut session) = engine
                .prepare_reduced_session(source, &settings, || false, &mut Profile::default())
                .unwrap_or_else(|error| {
                    panic!("white balance {white_balance} scale{divisor}: {error}")
                })
            else {
                panic!("superseded");
            };
            assert!(weak.upgrade().is_none());
            engine.clear_preparation();
            let image = rendered(
                engine
                    .render_reduced_session(
                        &mut session,
                        &settings,
                        &["final"],
                        || false,
                        &mut Profile::default(),
                    )
                    .unwrap_or_else(|error| {
                        panic!("source-free white balance {white_balance} scale{divisor}: {error}")
                    }),
            );
            assert_eq!(image.settings.working_scale, settings.working_scale);
            let verifier = engine.executor();
            let fresh = verifier
                .render(
                    &Source::open(&path).unwrap(),
                    &settings,
                    &mut Profile::default(),
                )
                .unwrap_or_else(|error| {
                    panic!("native source white balance {white_balance} scale{divisor}: {error}")
                });
            assert_eq!(
                engine.readback_image(&image).unwrap(),
                verifier.readback_image(&fresh).unwrap(),
                "white balance {white_balance} scale{divisor}"
            );
            eprintln!("exact source-free white balance {white_balance} scale{divisor}");
        }
    }
}
