//! Checks shared frontend reservations and coordinated pressure across processing owners.

use super::*;
use crate::{
    inspection::Content,
    profile::Profile,
    render::{RenderSession, SessionKind, SessionUse},
    source::Source,
};
use std::sync::Arc;

#[test]
fn merging_shared_snapshots_preserves_unique_metadata_and_excludes_exclusive_pools() {
    let metadata = Arc::new(crate::source::Metadata {
        width: 1,
        height: 1,
        rotation: 0,
        processing_version: None,
        properties: Default::default(),
        matrices: Default::default(),
        lists: Default::default(),
    });
    let mut first = MemoryReservation::default();
    first.add_metadata(&metadata);
    first.exclusive_bytes = 123;
    let mut second = MemoryReservation::default();
    second.add_metadata(&metadata);
    second.exclusive_bytes = 456;
    first.extend_shared(&second);
    first.extend_shared(&second);
    assert_eq!(first.metadata.len(), 1);
    assert!(Arc::ptr_eq(&first.metadata[0], &metadata));
    assert_eq!(first.exclusive_bytes, 123);
}

#[test]
fn decoded_source_reservations_deduplicate_by_allocation_not_origin() {
    let metadata = Arc::new(crate::source::Metadata {
        width: 1,
        height: 1,
        rotation: 0,
        processing_version: None,
        properties: Default::default(),
        matrices: Default::default(),
        lists: Default::default(),
    });
    let make = || {
        Arc::new(Source {
            origin: Default::default(),
            metadata: metadata.clone(),
            pixels: vec![1., 2., 3.].into(),
            storage: crate::source::SourceStorage::InterleavedRgb,
            camf: vec![],
            timings: vec![],
        })
    };
    let first = make();
    let second = make();
    let mut reservation = MemoryReservation::default();
    reservation.add_source(&first);
    reservation.add_source(&first.clone());
    reservation.add_source(&second);
    assert_eq!(reservation.sources.len(), 2);
    assert_eq!(reservation.metadata.len(), 1);
    let mut retained = RetainedBytes::default();
    // Model a native owner without a GPU or a photographic fixture.
    let session = RenderSession::new(first.clone());
    session.account_memory(&mut retained);
    for source in &reservation.sources {
        retained.owner(source.clone(), source.retained_source_bytes() as usize);
    }
    assert_eq!(
        retained.bytes() as u64,
        first.retained_source_bytes() + second.retained_source_bytes() + metadata.retained_bytes()
    );
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn shared_reservations_and_processing_owners_are_counted_once() {
    let source = Arc::new(Source::open(std::env::var_os("SMALL_PREVIEW_X3F").unwrap()).unwrap());
    let engine = Engine::test_executor();
    let mut settings = super::super::Settings::new("Sunlight", "Standard");
    settings.working_scale = super::super::Scale::new(8).unwrap();
    let mut native = RenderSession::new(source.clone());
    let Content::Image(image) = engine
        .render_session(
            &mut native,
            &settings,
            &["linear.base"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap()
        .remove(0)
        .content
    else {
        panic!("image")
    };
    assert!(native
        .cache
        .buffers()
        .any(|buffer| buffer.shares_storage(&image.buffer)));
    let mut reduced = ProcessingWorkingSet::new();
    let empty = MemoryReservation::default();
    reduced
        .insert_native(1, native, &engine, SessionUse::Foreground)
        .unwrap();
    let baseline = engine.memory_resources(&reduced, &empty);
    let mut reservation = MemoryReservation::default();
    reservation.add_image(&image);
    reservation.add_image(&image.clone());
    reservation.add_metadata(&source.metadata);
    reservation.add_source(&source);
    let mut another = reservation.clone();
    another.exclusive_bytes = 9999;
    reservation.extend_shared(&another);
    reservation.exclusive_bytes = 1234;
    let shared = engine.memory_resources(&reduced, &reservation);
    assert_eq!(shared.cpu_bytes, baseline.cpu_bytes);
    assert_eq!(shared.gpu_bytes, baseline.gpu_bytes);
    assert_eq!(shared.total_bytes, baseline.total_bytes + 1234);

    let super::super::ReducedPreparation::Prepared(session) = engine
        .prepare_reduced_session(source.clone(), &settings, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("preparation")
    };
    let reduced_cpu = session.retained_cpu_bytes();
    reduced
        .insert_reduced(1, session, &engine, SessionUse::Foreground)
        .unwrap();
    let combined = engine.memory_resources(&reduced, &reservation);
    assert!(
        combined.cpu_bytes < shared.cpu_bytes + reduced_cpu,
        "Native and reduced owners share the metadata allocation"
    );
    let bytes = engine.readback_image(&image).unwrap();
    let trim = engine.trim_memory(&mut reduced, 0, &reservation);
    assert_eq!(
        trim.evicted_sessions,
        vec![SessionKey {
            photo: 1,
            kind: SessionKind::Native
        }]
    );
    assert_eq!(trim.over_budget_bytes, trim.resources.total_bytes);
    assert!(trim.over_budget_bytes >= source.retained_source_bytes() + 1234);
    assert_eq!(engine.readback_image(&image).unwrap(), bytes);
    assert!(
        matches!(
            reduced
                .render_reduced(
                    &engine,
                    1,
                    &settings,
                    SessionUse::Foreground,
                    || false,
                    &mut Profile::default()
                )
                .unwrap(),
            Some(super::super::SessionRender::Rendered(_))
        ),
        "active sensor inputs remain source-free"
    );
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn coordinated_budget_evicts_cold_history_before_actual_active_photos() {
    let engine = Engine::test_executor();
    let path = std::env::var_os("SMALL_PREVIEW_X3F").unwrap();
    let mut settings = super::super::Settings::new("Sunlight", "Standard");
    settings.working_scale = super::super::Scale::new(8).unwrap();
    let mut reduced = ProcessingWorkingSet::new();
    for id in [1, 2] {
        let super::super::ReducedPreparation::Prepared(session) = engine
            .prepare_reduced_session(
                Source::open(&path).unwrap(),
                &settings,
                || false,
                &mut Profile::default(),
            )
            .unwrap()
        else {
            panic!("preparation")
        };
        reduced
            .insert_reduced(id, session, &engine, SessionUse::Foreground)
            .unwrap();
    }
    let reservation = MemoryReservation::default();
    let before = engine.memory_resources(&reduced, &reservation);
    let limit = reduced.active_bytes() + 1024 * 1024;
    assert!(before.total_bytes > limit);
    let trim = engine.trim_memory(&mut reduced, limit, &reservation);
    assert_eq!(
        trim.evicted_sessions,
        vec![SessionKey {
            photo: 1,
            kind: SessionKind::Reduced
        }]
    );
    assert!(reduced.contains(SessionKey {
        photo: 2,
        kind: SessionKind::Reduced
    }));
    assert_eq!(trim.over_budget_bytes, 0);
    assert!(trim.resources.total_bytes < before.total_bytes);
    assert!(matches!(
        reduced
            .render_reduced(
                &engine,
                2,
                &settings,
                SessionUse::Foreground,
                || false,
                &mut Profile::default()
            )
            .unwrap(),
        Some(super::super::SessionRender::Rendered(_))
    ));
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR"]
fn native_fine_edits_keep_their_restart_frontier_under_memory_pressure() {
    let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let source = Arc::new(Source::open(path).unwrap());
    let engine = Engine::test_executor();
    let mut reduced = ProcessingWorkingSet::new();
    reduced
        .insert_native(
            1,
            RenderSession::new(source.clone()),
            &engine,
            SessionUse::Foreground,
        )
        .unwrap();
    let mut settings = super::super::Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    settings.sharpen_amount = Some(0.75);
    for (step, fine) in [1., 0.1, 0.15, 0.2].into_iter().enumerate() {
        settings.reconstruction.as_mut().unwrap().texture.fine = fine;
        let mut profile = Profile::new(crate::profile::Mode::Wall);
        let Content::Image(image) = reduced
            .with_native(1, |session| {
                engine.render_session(session, &settings, &["final"], || false, &mut profile)
            })
            .unwrap()
            .flatten()
            .unwrap()
            .remove(0)
            .content
        else {
            panic!("image")
        };
        if step >= 2 {
            assert!(
                profile.nodes.iter().all(|node| !node.executed
                    || !matches!(
                        node.operation,
                        "Source" | "NoiseBase" | "Column" | "Ratio" | "Pattern"
                    )),
                "warmed Fine edits must reuse their unchanged input cut: {:?}",
                profile.nodes
            );
        }
        let mut reservation = MemoryReservation::default();
        reservation.add_image(&image);
        // Even an undersized user budget must report essential overcommit
        // rather than repeatedly discarding the active control's inputs.
        let trim = engine.trim_memory(&mut reduced, 0, &reservation);
        assert!(trim.over_budget_bytes > 0);
        if step == 3 {
            let fresh = engine
                .render(&source, &settings, &mut Profile::default())
                .unwrap();
            assert_eq!(
                engine.readback_image(&image).unwrap(),
                engine.readback_image(&fresh).unwrap()
            );
        }
    }
}
