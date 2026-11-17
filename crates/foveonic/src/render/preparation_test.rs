//! Checks bounded preparation ownership, exact invalidation and sharing across repeated edits.

use super::*;
use crate::source::{Matrix, MatrixEncoding};

fn blue(stage: ordered_blue_repair::Stage) -> ordered_blue_repair::Prepared {
    ordered_blue_repair::Prepared::new(
        [128, 128],
        (0..1000)
            .map(|i| [12 + i / 100, 12 + i % 100, 0x03ff])
            .collect(),
        0,
        1.,
        stage,
    )
    .unwrap()
}

#[test]
fn repair_preparation_is_lazy_shared_and_separate_for_metadata_and_stage() {
    let metadata = metadata();
    let mut cache = Cache::default();
    let mut builds = 0;
    for stage in [
        ordered_blue_repair::Stage::Directional,
        ordered_blue_repair::Stage::DarkOffsets,
        ordered_blue_repair::Stage::General,
    ] {
        let first = cache
            .blue_repair(&metadata, stage, || {
                builds += 1;
                Ok(blue(stage))
            })
            .unwrap();
        let second = cache
            .blue_repair(&metadata, stage, || panic!("warm repair was rebuilt"))
            .unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let mut retained = RetainedBytes::default();
        cache.account(&mut retained);
        let prepared_bytes = retained.bytes();
        first.key().account(&mut retained);
        second.key().account(&mut retained);
        assert!(
            retained.bytes() - prepared_bytes < 1024,
            "keys must share catalogue bytes"
        );
    }
    assert_eq!(builds, 3);
    assert_eq!(Arc::strong_count(&metadata), 1);
    let distinct = Arc::new((*metadata).clone());
    cache
        .blue_repair(&distinct, ordered_blue_repair::Stage::Directional, || {
            builds += 1;
            Ok(blue(ordered_blue_repair::Stage::Directional))
        })
        .unwrap();
    assert_eq!(builds, 4);
    let chroma = || {
        chroma_repair::Prepared::new(
            [64, 64],
            vec![],
            0,
            [1.; 9],
            [1., 0., 0., 0., 1., 0., 0., 0., 1.],
            false,
            false,
        )
    };
    let first = cache.chroma_repair(&metadata, chroma).unwrap();
    let second = cache
        .chroma_repair(&metadata, || panic!("warm chroma was rebuilt"))
        .unwrap();
    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn repair_history_evicts_old_sources_and_retains_a_large_current_bundle() {
    let mut cache = Cache::default();
    let sources: Vec<_> = (0..=REPAIR_SOURCES).map(|_| metadata()).collect();
    let stage = ordered_blue_repair::Stage::Directional;
    let mut oldest = None;
    for source in &sources {
        let prepared = cache
            .blue_repair(source, stage, || Ok(blue(stage)))
            .unwrap();
        oldest.get_or_insert_with(|| Arc::downgrade(&prepared));
    }
    assert_eq!(cache.repairs.len(), REPAIR_SOURCES);
    assert!(oldest.unwrap().upgrade().is_none());
    let current = sources.last().unwrap();
    cache.repairs.last_mut().unwrap().bytes = REPAIR_BYTES + 1;
    let second = cache
        .blue_repair(current, ordered_blue_repair::Stage::General, || {
            Ok(blue(ordered_blue_repair::Stage::General))
        })
        .unwrap();
    assert_eq!(
        cache.repairs.len(),
        2,
        "large current stages must not evict one another"
    );
    cache
        .blue_repair(current, stage, || panic!("large current stage was rebuilt"))
        .unwrap();
    let weak = Arc::downgrade(&second);
    drop(second);
    drop(sources);
    let other = metadata();
    cache
        .blue_repair(&other, stage, || Ok(blue(stage)))
        .unwrap();
    assert_eq!(
        cache.repairs.len(),
        1,
        "expired metadata preparations must release their payloads"
    );
    assert!(weak.upgrade().is_none());
}

fn metadata() -> Arc<Metadata> {
    let mut value = Metadata {
        width: 64,
        height: 128,
        rotation: 0,
        processing_version: None,
        properties: Default::default(),
        matrices: Default::default(),
        lists: Default::default(),
    };
    for (name, values) in [
        ("ActiveImageArea", vec![8., 34., 48., 64.]),
        ("DarkShieldColRange", vec![0., 2., 56., 63.]),
        ("SatMapR", vec![8192.]),
        ("SatMapG", vec![8192.]),
        ("SatMapB", vec![8192.]),
    ] {
        value.matrices.insert(
            name.into(),
            Matrix {
                encoding: MatrixEncoding::Uint32,
                representation: "u32",
                dimensions: vec![values.len()],
                values,
            },
        );
    }
    Arc::new(value)
}
fn mode() -> color_mode::Settings {
    color_mode::Settings {
        tone_index: 0,
        contrast: 0.,
        compensation: None,
    }
}
#[test]
fn tone_preparation_reuses_exact_inputs_and_bounds_changed_responses() {
    let metadata = metadata();
    let mut cache = Cache::default();
    let original = cache
        .tone(&metadata, &mode(), Default::default(), false)
        .unwrap();
    let repeat = cache
        .tone(&metadata, &mode(), Default::default(), false)
        .unwrap();
    assert!(original.shares_storage(&repeat));
    assert_eq!(original.key(), repeat.key());
    assert_eq!(
        &*original,
        tone::prepare(&metadata, &mode(), Default::default()).unwrap()
    );
    assert_eq!(Arc::strong_count(&metadata), 1);
    for controls in [
        tone::Controls {
            contrast: 0.2,
            ..Default::default()
        },
        tone::Controls {
            shadows: 0.2,
            ..Default::default()
        },
        tone::Controls {
            highlights: 0.2,
            ..Default::default()
        },
    ] {
        let changed = cache.tone(&metadata, &mode(), controls, false).unwrap();
        assert!(!original.shares_storage(&changed));
        assert_eq!(
            &*changed,
            tone::prepare(&metadata, &mode(), controls).unwrap()
        );
        assert!(cache.bytes() <= TONES_PER_SOURCE * (65536 * 4 + 1024));
    }
    let mut selected = mode();
    selected.contrast = 0.1;
    let changed_mode = cache
        .tone(&metadata, &selected, Default::default(), false)
        .unwrap();
    assert_eq!(
        &*changed_mode,
        tone::prepare(&metadata, &selected, Default::default()).unwrap()
    );
    selected.compensation = Some([1.; 9]);
    assert!(changed_mode.shares_storage(
        &cache
            .tone(&metadata, &selected, Default::default(), false)
            .unwrap()
    ));
    let monochrome = cache
        .tone(&metadata, &selected, Default::default(), true)
        .unwrap();
    assert_eq!(
        &*monochrome,
        tone::prepare_monochrome(&metadata, Default::default()).unwrap()
    );
}
#[test]
fn immutable_metadata_identity_separates_sources_without_retaining_them() {
    let first = metadata();
    let second = Arc::new((*first).clone());
    let mut cache = Cache::default();
    let a = cache
        .tone(&first, &mode(), Default::default(), false)
        .unwrap();
    let b = cache
        .tone(&second, &mode(), Default::default(), false)
        .unwrap();
    assert!(!a.shares_storage(&b));
    assert_eq!(a, b);
    assert_eq!(cache.tones.len(), 2);
    assert_eq!(Arc::strong_count(&first), 1);
    assert_eq!(Arc::strong_count(&second), 1);
}
#[test]
fn highlight_thresholds_share_clipping_evidence_and_remain_exact() {
    let metadata = metadata();
    let mut cache = Cache::default();
    let original = cache.highlights(&metadata, None).unwrap();
    let repeated = cache.highlights(&metadata, None).unwrap();
    assert!(original
        .flags
        .as_ref()
        .unwrap()
        .shares_storage(repeated.flags.as_ref().unwrap()));
    let changed = cache.highlights(&metadata, Some(-1.)).unwrap();
    assert!(!Arc::ptr_eq(&original, &changed));
    assert!(original
        .flags
        .as_ref()
        .unwrap()
        .shares_storage(changed.flags.as_ref().unwrap()));
    assert_ne!(original.key(), changed.key());
    assert_eq!(
        changed.key(),
        highlight::prepare(&metadata, Some(-1.)).unwrap().key()
    );
    assert_eq!(cache.bytes(), metadata.width * metadata.height * 4);
    assert!(cache.highlights(&metadata, Some(f32::NAN)).is_err());
    assert!(changed.flags.as_ref().unwrap().shares_storage(
        cache
            .highlights(&metadata, Some(-1.))
            .unwrap()
            .flags
            .as_ref()
            .unwrap()
    ));
    let other = Arc::new((*metadata).clone());
    let changed_source = cache.highlights(&other, Some(-1.)).unwrap();
    assert!(!changed_source
        .flags
        .as_ref()
        .unwrap()
        .shares_storage(changed.flags.as_ref().unwrap()));
}

#[test]
fn preparation_inventory_deduplicates_payloads_retained_by_graph_keys() {
    let metadata = metadata();
    let mut cache = Cache::default();
    let curve = cache
        .tone(&metadata, &mode(), Default::default(), false)
        .unwrap();
    let highlights = cache.highlights(&metadata, None).unwrap();
    let curve_key = curve.key();
    let highlight_key = highlights.key();
    let mut retained = RetainedBytes::default();
    cache.account(&mut retained);
    let before = retained.bytes();
    curve.account(&mut retained);
    highlights.flags.as_ref().unwrap().account(&mut retained);
    assert_eq!(retained.bytes(), before);
    curve_key.account(&mut retained);
    highlight_key.account(&mut retained);
    // Additional graph-owned key vectors and scalar parameters remain small.
    assert!(retained.bytes() - before < 1024);
}

#[test]
fn alternating_photos_reuse_both_curves_with_bounded_weak_ownership() {
    let photos: Vec<_> = (0..8).map(|_| metadata()).collect();
    let mut cache = Cache::default();
    let adjusted = tone::Controls {
        contrast: 0.25,
        ..Default::default()
    };
    let mut curves = Vec::new();
    for photo in &photos {
        curves.push([
            cache
                .tone(photo, &mode(), Default::default(), false)
                .unwrap(),
            cache.tone(photo, &mode(), adjusted, false).unwrap(),
        ]);
    }
    assert_eq!(cache.tones.len(), TONE_CAPACITY);
    for (photo, previous) in photos.iter().zip(curves) {
        assert!(previous[0].shares_storage(
            &cache
                .tone(photo, &mode(), Default::default(), false)
                .unwrap()
        ));
        assert!(previous[1].shares_storage(&cache.tone(photo, &mode(), adjusted, false).unwrap()));
        assert_eq!(Arc::strong_count(photo), 1);
    }
    let extra = metadata();
    cache
        .tone(&extra, &mode(), Default::default(), false)
        .unwrap();
    assert_eq!(cache.tones.len(), TONE_CAPACITY);
    assert!(cache.bytes() <= TONE_CAPACITY * (65536 * 4 + 1024));
    drop(photos);
    cache.tone(&extra, &mode(), adjusted, false).unwrap();
    assert_eq!(cache.tones.len(), TONES_PER_SOURCE);
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with original Merrill files"]
fn gpu_preparation_invalidation_and_native_release_match_fresh_pixels() {
    use crate::{
        image::RenderedImage,
        inspection::Content,
        profile::Profile,
        render::{BaseResponse, PhotographicMethod, RenderSession, Scale, Settings},
        source::Source,
    };

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
            panic!("expected final image");
        };
        *image
    }
    fn compare(
        engine: &Engine,
        session: &mut RenderSession,
        settings: &Settings,
        label: &str,
    ) -> RenderedImage {
        let image = cached(engine, session, settings);
        let verifier = engine.executor();
        let fresh = verifier
            .render(session.source(), settings, &mut Profile::default())
            .unwrap();
        assert_eq!(image.description(), fresh.description(), "{label}: extent");
        assert_eq!(image.view(), fresh.view(), "{label}: view");
        assert!(
            engine.readback_image(&image).unwrap() == verifier.readback_image(&fresh).unwrap(),
            "{label}: exact pixels"
        );
        image
    }

    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let first = Arc::new(Source::open(root.join("blue-parking-sign.x3f")).unwrap());
    let second = Arc::new(Source::open(root.join("merrill-hazy-coast.x3f")).unwrap());
    let engine = Engine::test_executor();
    let mut session = RenderSession::new(first.clone());
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.working_scale = Scale::new(8).unwrap();
    settings.reconstruction = Some(Default::default());
    let retained = compare(&engine, &mut session, &settings, "initial");
    let retained_bytes = engine.readback_image(&retained).unwrap();
    settings.exposure = 0.25;
    compare(&engine, &mut session, &settings, "exposure");
    settings.white_balance = first.metadata.lists["WhiteBalanceGains"]
        .names()
        .find(|name| *name != "Sunlight")
        .unwrap()
        .into();
    compare(&engine, &mut session, &settings, "white balance");
    settings.highlight_threshold_ev = Some(-1.);
    compare(&engine, &mut session, &settings, "highlight threshold");
    settings.highlight_restoration = false;
    compare(&engine, &mut session, &settings, "highlights disabled");
    settings.highlight_restoration = true;
    settings.reconstruction.as_mut().unwrap().chroma = 2;
    compare(&engine, &mut session, &settings, "noise");
    settings.tone.contrast = 0.25;
    settings.tone.shadows = 0.2;
    settings.tone.highlights = -0.2;
    compare(&engine, &mut session, &settings, "tone controls");
    settings.color_mode = first.metadata.lists["ColorModeContrastCompensations"]
        .names()
        .find(|name| *name != "Standard")
        .unwrap()
        .into();
    compare(&engine, &mut session, &settings, "color mode");
    settings.photographic.tone_method = PhotographicMethod::Universal;
    settings.photographic.base_response = BaseResponse::Natural;
    compare(&engine, &mut session, &settings, "shared adaptive curve");
    settings.tone.contrast = -0.25;
    compare(
        &engine,
        &mut session,
        &settings,
        "adaptive and changed tone curves",
    );
    // Reduced rendering need not materialize native clipping. Request it
    // explicitly so this release check owns a real native payload.
    let native = engine.clipping_flags(&first.metadata, None).unwrap();
    let native_bytes = native.len() * std::mem::size_of::<u32>();
    assert_eq!(native.len(), first.metadata.width * first.metadata.height);
    let before = engine.cached_preparation_bytes();
    assert!(before >= native_bytes);
    engine.release_native_preparation(&second.metadata);
    assert_eq!(
        engine.cached_preparation_bytes(),
        before,
        "other source release"
    );
    engine.release_native_preparation(&first.metadata);
    assert_eq!(engine.cached_preparation_bytes(), before - native_bytes);
    compare(
        &engine,
        &mut session,
        &settings,
        "recreated native preparation",
    );
    let selected = color_mode::prepare(&first.metadata, &settings.color_mode).unwrap();
    let previous_curve = engine
        .prepare_tone(&first.metadata, &selected, settings.tone, false)
        .unwrap();
    let mut other = RenderSession::new(second);
    compare(
        &engine,
        &mut other,
        &Settings {
            working_scale: Scale::new(8).unwrap(),
            ..Settings::new("Sunlight", "Standard")
        },
        "second source",
    );
    compare(&engine, &mut session, &settings, "return to first source");
    assert!(
        previous_curve.shares_storage(
            &engine
                .prepare_tone(&first.metadata, &selected, settings.tone, false)
                .unwrap()
        ),
        "alternating Engine reuses first photo's curve"
    );
    engine.clear_preparation();
    assert_eq!(engine.cached_preparation_bytes(), 0);
    compare(
        &engine,
        &mut session,
        &settings,
        "released engine preparation",
    );
    assert!(
        engine.readback_image(&retained).unwrap() == retained_bytes,
        "retained image"
    );
}

#[test]
fn deferred_highlights_keep_small_keys_and_materialize_only_when_needed() {
    let metadata = metadata();
    let description = highlight::deferred(&metadata, Some(-1.)).unwrap();
    assert!(description.flags.is_none());
    let mut retained = RetainedBytes::default();
    description.key().account(&mut retained);
    assert!(retained.bytes() < 1024);
    let mut cache = Cache::default();
    let first = cache
        .materialize_highlights(&metadata, &description)
        .unwrap();
    assert_eq!(
        bytemuck::bytes_of(&first.params),
        bytemuck::bytes_of(&description.params)
    );
    assert!(first.flags.is_some());
    assert!(first.flags.as_ref().unwrap().shares_storage(
        cache
            .materialize_highlights(&metadata, &description)
            .unwrap()
            .flags
            .as_ref()
            .unwrap()
    ));
    let warm = highlight::deferred(&metadata, Some(-1.)).unwrap();
    assert_eq!(description.key(), warm.key());
    assert!(warm.flags.is_none());
    let changed = highlight::deferred(&metadata, Some(-0.5)).unwrap();
    assert_ne!(description.key(), changed.key());
    let next = cache.materialize_highlights(&metadata, &changed).unwrap();
    assert!(first
        .flags
        .as_ref()
        .unwrap()
        .shares_storage(next.flags.as_ref().unwrap()));
    assert_eq!(
        bytemuck::bytes_of(&next.params),
        bytemuck::bytes_of(&highlight::prepare(&metadata, Some(-0.5)).unwrap().params)
    );
}

#[test]
fn clipping_cache_shares_consumers_and_separates_native_and_working_grids() {
    use crate::render::{Scale, WorkingGrid};
    let metadata = metadata();
    let mut cache = Cache::default();
    let native = cache.clipping_flags(&metadata, None).unwrap();
    let grid = WorkingGrid::new(
        [metadata.width as u32, metadata.height as u32],
        Scale::new(4).unwrap(),
        8,
    )
    .unwrap()
    .with_halo(4)
    .unwrap();
    let reduced = cache.clipping_flags(&metadata, Some(grid)).unwrap();
    assert!(!native.shares_storage(&reduced));
    assert_eq!(reduced.len(), (grid.extent[0] * grid.extent[1]) as usize);
    let description = highlight::deferred(&metadata, Some(-1.))
        .unwrap()
        .on_grid(Some(grid));
    let restoration = cache
        .materialize_highlights(&metadata, &description)
        .unwrap();
    assert!(reduced.shares_storage(restoration.flags.as_ref().unwrap()));
    assert_eq!(cache.bytes(), reduced.len() * 4);
    let native_again = cache.clipping_flags(&metadata, None).unwrap();
    assert_eq!(native, native_again);
    assert!(!native.shares_storage(&native_again));
}
