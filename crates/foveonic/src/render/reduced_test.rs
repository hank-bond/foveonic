//! Checks source-free recipe cuts, full-option outputs and bounded ownership.

use super::*;

fn rendered(outcome: PhotoRender) -> crate::image::RenderedImage {
    let PhotoRender::Rendered(mut artifacts) = outcome else {
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
    cases.push(("multiscale", s.clone()));
    s.rendering = super::super::Rendering::Monochrome;
    cases.push(("mono-multiscale", s));
    let mut s = base.clone();
    s.reconstruction.get_or_insert_with(Default::default);
    s.merrill_noise_method = super::super::MerrillNoiseMethod::MultiscaleRows;
    cases.push(("multiscale-rows", s));
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
    let mut compact = base.clone();
    compact.merrill_speckles = super::super::MerrillSpeckleControls {
        method: super::super::MerrillSpeckleMethod::Compact,
        threshold: 0.2,
        size: 7,
    };
    cases.push(("compact-speckles", compact));
    let mut mono_directional = base.clone();
    mono_directional.rendering = super::super::Rendering::Monochrome;
    mono_directional
        .reconstruction
        .get_or_insert_with(Default::default);
    mono_directional.merrill_banding.method = super::super::MerrillBandingMethod::Directional;
    cases.push(("mono-directional", mono_directional));
    for (name, approach) in [
        (
            "highlight-local",
            super::super::ColorFieldApproach::LocalGradient,
        ),
        (
            "highlight-global",
            super::super::ColorFieldApproach::Faithful,
        ),
    ] {
        let mut s = base.clone();
        s.highlight_mode = super::super::HighlightMode::ColorField;
        s.color_field.approach = approach;
        cases.push((name, s.clone()));
        let mut multiscale = s.clone();
        multiscale
            .reconstruction
            .get_or_insert_with(Default::default);
        multiscale.merrill_noise_method = super::super::MerrillNoiseMethod::MultiscaleRows;
        cases.push((
            match approach {
                super::super::ColorFieldApproach::LocalGradient => "multiscale-highlight-local",
                super::super::ColorFieldApproach::Faithful => "multiscale-highlight-global",
            },
            multiscale.clone(),
        ));
        multiscale.merrill_banding.enabled = true;
        multiscale.merrill_banding.method = super::super::MerrillBandingMethod::Directional;
        cases.push((
            match approach {
                super::super::ColorFieldApproach::LocalGradient => "directional-highlight-local",
                super::super::ColorFieldApproach::Faithful => "directional-highlight-global",
            },
            multiscale,
        ));
        s.merrill_speckles.method = super::super::MerrillSpeckleMethod::Compact;
        cases.push((
            match approach {
                super::super::ColorFieldApproach::LocalGradient => "highlight-local-compact",
                super::super::ColorFieldApproach::Faithful => "highlight-global-compact",
            },
            s,
        ));
    }
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
    let engine = Engine::test_executor();
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
                .prepare_reduced_photo(original, &settings, || false, &mut Profile::default())
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
            for edit in 0..5 {
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
                if edit == 4
                    && session.metadata().processing_family().unwrap()
                        == crate::source::ProcessingFamily::Merrill
                {
                    if settings.highlight_mode == super::super::HighlightMode::Classic {
                        settings.highlight_threshold_ev = Some(-1.);
                    } else {
                        settings.color_field.selectivity = 0.8;
                    }
                }
                let mut artifacts = match engine
                    .render_reduced_photo(
                        &mut session,
                        &settings,
                        &["final"],
                        || false,
                        &mut Profile::default(),
                    )
                    .unwrap()
                {
                    PhotoRender::Rendered(artifacts) => artifacts,
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
            {
                if settings.directional_banding_selected()
                    && settings.merrill_noise_method
                        == super::super::MerrillNoiseMethod::MultiscaleRows
                {
                    let mut changed = settings.clone();
                    changed.merrill_noise_method = super::super::MerrillNoiseMethod::Reference;
                    assert!(
                        matches!(
                            engine
                                .render_reduced_photo(
                                    &mut session,
                                    &changed,
                                    &["final"],
                                    || false,
                                    &mut Profile::default()
                                )
                                .unwrap(),
                            PhotoRender::Rendered(_)
                        ),
                        "resized-only filter change {name} scale{divisor}"
                    );
                }
                let mut changed = settings.clone();
                // Noise-method changes can now affect only resized filtering
                // when Directional already supplies the same shield inputs.
                // White balance still changes native calibration and shielding.
                changed.white_balance = if settings.white_balance == "Incandescent" {
                    "Sunlight"
                } else {
                    "Incandescent"
                }
                .into();
                let before = engine.gpu_activity();
                assert!(
                    matches!(
                        engine
                            .render_reduced_photo(
                                &mut session,
                                &changed,
                                &["final"],
                                || false,
                                &mut Profile::default()
                            )
                            .unwrap(),
                        PhotoRender::NeedsOriginal(NeedsOriginal::UpstreamChanged)
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
                    .render_reduced_photo(
                        &mut session,
                        &settings,
                        &["final"],
                        || true,
                        &mut Profile::default()
                    )
                    .unwrap(),
                PhotoRender::Superseded
            ));
            assert!(matches!(
                engine
                    .executor()
                    .render_reduced_photo(
                        &mut session,
                        &settings,
                        &["final"],
                        || false,
                        &mut Profile::default()
                    )
                    .unwrap(),
                PhotoRender::NeedsOriginal(NeedsOriginal::EngineChanged)
            ));
            eprintln!("exact source-free {name} scale{divisor}");
        }
    }
    assert!(exercised > 0, "no applicable option cases selected");
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn shared_working_set_orders_native_reduced_and_speculative_owners() {
    use crate::render::{NativePhoto, PhotoKey, Purpose, Resolution, WorkingSet};
    let path = std::env::var_os("SMALL_PREVIEW_X3F").unwrap();
    let source = Arc::new(Source::open(&path).unwrap());
    let engine = Engine::test_executor();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.working_scale = Scale::new(8).unwrap();
    let ReducedPreparation::Prepared(session) = engine
        .prepare_reduced_photo(source.clone(), &settings, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("preparation")
    };
    let mut working = WorkingSet::new();
    let reduced = PhotoKey {
        photo: 1,
        resolution: Resolution::Reduced,
    };
    let edited = PhotoKey {
        photo: 2,
        resolution: Resolution::Native,
    };
    let speculative = PhotoKey {
        photo: 3,
        resolution: Resolution::Native,
    };
    working
        .insert_reduced(1, session, &engine, Purpose::Edit)
        .unwrap();
    working
        .insert_native(2, NativePhoto::new(source.clone()), &engine, Purpose::Edit)
        .unwrap();
    working
        .insert_native(
            3,
            NativePhoto::new(source.clone()),
            &engine,
            Purpose::Speculative,
        )
        .unwrap();
    let first_native = working
        .with_native(2, |session| {
            engine.render_native_photo(
                session,
                &settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
        })
        .unwrap()
        .flatten()
        .unwrap()
        .remove(0);
    let crate::inspection::Content::Image(first_native) = first_native.content else {
        panic!("image")
    };
    assert!(working.record_use(reduced, Purpose::Edit).is_some());
    assert!(working.record_use(edited, Purpose::Edit).is_some());
    assert!(working.contains(reduced));
    let image = rendered(
        working
            .render_reduced(
                &engine,
                1,
                &settings,
                Purpose::Speculative,
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap(),
    );
    assert_eq!(working.evict_least_recent(), Some(speculative));
    assert_eq!(working.evict_least_recent(), Some(reduced));
    assert!(working.contains(edited));
    let replay = working
        .with_native(2, |session| {
            engine.render_native_photo(
                session,
                &settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
        })
        .unwrap()
        .flatten()
        .unwrap()
        .remove(0);
    let crate::inspection::Content::Image(replay) = replay.content else {
        panic!("image")
    };
    assert_eq!(
        engine.readback_image(&first_native).unwrap(),
        engine.readback_image(&replay).unwrap()
    );
    assert!(
        !engine.readback_image(&image).unwrap().is_empty(),
        "eviction preserves delivered pixels"
    );
    working.set_visible([edited]);
    assert_eq!(
        working.evict_least_recent(),
        None,
        "a visible photo is never evicted"
    );
    assert!(working.contains(edited));
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn visible_reduced_photo_keeps_its_sensor_data_under_pressure() {
    use crate::render::{MemoryReservation, PhotoKey, Purpose, Resolution, WorkingSet};
    let source = Arc::new(Source::open(std::env::var_os("SMALL_PREVIEW_X3F").unwrap()).unwrap());
    let weak = Arc::downgrade(&source);
    let engine = Engine::test_executor();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.working_scale = Scale::new(2).unwrap();
    let ReducedPreparation::Prepared(session) = engine
        .prepare_reduced_photo(source, &settings, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("preparation")
    };
    assert!(
        weak.upgrade().is_none(),
        "source-free preparation must release decoded input"
    );
    let resources = session.resources();
    assert_eq!(
        resources.metadata_bytes
            + resources.owner_bytes
            + resources.cache_structure_bytes
            + resources.parameter_payload_bytes
            + resources.preparation_bytes,
        session.retained_cpu_bytes()
    );
    let sensor_data: Vec<_> = session.cache.sensor_data_buffers().cloned().collect();
    let mut working = WorkingSet::new();
    working
        .insert_reduced(7, session, &engine, Purpose::Edit)
        .unwrap();
    working.set_visible([PhotoKey {
        photo: 7,
        resolution: Resolution::Reduced,
    }]);
    let key = PhotoKey {
        photo: 7,
        resolution: Resolution::Reduced,
    };
    for exposure in [0., 0.2, 0.4] {
        settings.exposure = exposure;
        let image = rendered(
            working
                .render_reduced(
                    &engine,
                    7,
                    &settings,
                    Purpose::Edit,
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap(),
        );
        assert_eq!(image.settings.working_scale, settings.working_scale);
        let trim = engine.trim_memory(&mut working, 1, &MemoryReservation::default());
        assert!(trim.over_target_bytes > 0);
        assert!(working.contains(key));
        assert!(
            sensor_data.iter().all(|old| working
                .reduced(7)
                .unwrap()
                .cache
                .sensor_data_buffers()
                .any(|new| old.shares_storage(new))),
            "a visible photo keeps its sensor data under pressure"
        );
    }
    // A visible photo stays even when nothing fits.
    let trim = engine.trim_memory(&mut working, 0, &MemoryReservation::default());
    assert!(trim.evicted_photos.is_empty());
    assert!(working.contains(key));
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn verified_reload_preserves_main_sensor_data_and_rejects_other_origins_before_mutation() {
    let path = std::env::var_os("SMALL_PREVIEW_X3F").unwrap();
    let original = Arc::new(Source::open(&path).unwrap());
    let weak = Arc::downgrade(&original);
    let engine = Engine::test_executor();
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
        .prepare_reduced_photo(original, &settings, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("preparation");
    };
    assert!(weak.upgrade().is_none());
    let _main = rendered(
        engine
            .render_reduced_photo(
                &mut session,
                &settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap(),
    );
    let main_buffers: Vec<_> = session.cache.sensor_data_buffers().cloned().collect();
    assert!(!main_buffers.is_empty());
    let old_metadata = session.metadata().clone();
    let old_bytes = session.cached_bytes();
    let mut unrelated = Source::open(&path).unwrap();
    unrelated.origin = SourceOrigin::default();
    assert!(engine
        .refresh_reduced_photo(
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
        .render_reduced_photo(
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
            PhotoRender::Rendered(_) | PhotoRender::NeedsOriginal(NeedsOriginal::UpstreamChanged)
        ),
        "a new guide uses the retained cut or requests its actually missing native dependencies"
    );
    let reload = Arc::new(Source::open(&path).unwrap());
    let reload_weak = Arc::downgrade(&reload);
    assert_eq!(&session.origin, reload.origin());
    assert!(!Arc::ptr_eq(session.metadata(), &reload.metadata));
    assert!(engine
        .refresh_reduced_photo(
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
            .render_reduced_photo(
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
    let engine = Engine::test_executor();
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
        .prepare_reduced_photo(source, &region, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("ROI execution hint must not exclude source-free ownership");
    };
    assert!(weak.upgrade().is_none());
    let retained = rendered(
        engine
            .render_reduced_photo(
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
    let engine = Engine::test_executor();
    let scales = std::env::var("WHITE_BALANCE_SCALES").unwrap_or_else(|_| "1,2,8,64".into());
    for divisor in scales.split(',').map(|s| s.parse::<u32>().unwrap()) {
        settings.working_scale = Scale::new(divisor).unwrap();
        for white_balance in &choices {
            settings.white_balance = white_balance.clone();
            let source = Arc::new(Source::open(&path).unwrap());
            let weak = Arc::downgrade(&source);
            let ReducedPreparation::Prepared(mut session) = engine
                .prepare_reduced_photo(source, &settings, || false, &mut Profile::default())
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
                    .render_reduced_photo(
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
