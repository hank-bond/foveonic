//! Checks shared frontend reservations and coordinated pressure across processing owners.

use super::*;
use crate::{
    inspection::Content,
    profile::Profile,
    render::{NativePhoto, Purpose, Resolution},
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
    let session = NativePhoto::new(first.clone());
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
    let mut native = NativePhoto::new(source.clone());
    let Content::Image(image) = engine
        .render_native_photo(
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
    let mut reduced = WorkingSet::new();
    let empty = MemoryReservation::default();
    reduced
        .insert_native(1, native, &engine, Purpose::Edit)
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
        .prepare_reduced_photo(source.clone(), &settings, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("preparation")
    };
    let reduced_cpu = session.retained_cpu_bytes();
    reduced
        .insert_reduced(1, session, &engine, Purpose::Edit)
        .unwrap();
    let combined = engine.memory_resources(&reduced, &reservation);
    assert!(
        combined.cpu_bytes < shared.cpu_bytes + reduced_cpu,
        "Native and reduced owners share the metadata allocation"
    );
    let bytes = engine.readback_image(&image).unwrap();
    // A guard keeps the reduced photo while the native one is evicted.
    let in_use = reduced
        .guard(PhotoKey {
            photo: 1,
            resolution: Resolution::Reduced,
        })
        .unwrap();
    let trim = engine.trim_memory(&mut reduced, 0, &reservation);
    drop(in_use);
    assert_eq!(
        trim.evicted_photos,
        vec![PhotoKey {
            photo: 1,
            resolution: Resolution::Native
        }]
    );
    assert_eq!(trim.over_target_bytes, trim.resources.total_bytes);
    assert!(trim.over_target_bytes >= source.retained_source_bytes() + 1234);
    assert_eq!(engine.readback_image(&image).unwrap(), bytes);
    assert!(
        matches!(
            reduced
                .render_reduced(
                    &engine,
                    1,
                    &settings,
                    Purpose::Edit,
                    || false,
                    &mut Profile::default()
                )
                .unwrap(),
            Some(super::super::PhotoRender::Rendered(_))
        ),
        "reduced sensor data needs no file"
    );
}

#[test]
#[ignore = "requires GPU and SMALL_PREVIEW_X3F"]
fn target_evicts_the_least_recent_photo_first() {
    let engine = Engine::test_executor();
    let path = std::env::var_os("SMALL_PREVIEW_X3F").unwrap();
    let mut settings = super::super::Settings::new("Sunlight", "Standard");
    settings.working_scale = super::super::Scale::new(8).unwrap();
    let mut reduced = WorkingSet::new();
    for id in [1, 2] {
        let super::super::ReducedPreparation::Prepared(session) = engine
            .prepare_reduced_photo(
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
            .insert_reduced(id, session, &engine, Purpose::Edit)
            .unwrap();
    }
    let reservation = MemoryReservation::default();
    let before = engine.memory_resources(&reduced, &reservation);
    let target = reduced.most_recent_bytes() + 1024 * 1024;
    assert!(before.total_bytes > target);
    let trim = engine.trim_memory(&mut reduced, target, &reservation);
    assert_eq!(
        trim.evicted_photos,
        vec![PhotoKey {
            photo: 1,
            resolution: Resolution::Reduced
        }]
    );
    assert!(reduced.contains(PhotoKey {
        photo: 2,
        resolution: Resolution::Reduced
    }));
    assert_eq!(trim.over_target_bytes, 0);
    assert!(trim.resources.total_bytes < before.total_bytes);
    assert!(matches!(
        reduced
            .render_reduced(
                &engine,
                2,
                &settings,
                Purpose::Edit,
                || false,
                &mut Profile::default()
            )
            .unwrap(),
        Some(super::super::PhotoRender::Rendered(_))
    ));
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR"]
fn native_fine_edits_keep_their_restart_frontier_under_memory_pressure() {
    let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let source = Arc::new(Source::open(path).unwrap());
    let engine = Engine::test_executor();
    let mut reduced = WorkingSet::new();
    reduced
        .insert_native(1, NativePhoto::new(source.clone()), &engine, Purpose::Edit)
        .unwrap();
    let key = PhotoKey {
        photo: 1,
        resolution: Resolution::Native,
    };
    reduced.set_visible([key]);
    let mut settings = super::super::Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    settings.sharpen_amount = Some(0.75);
    for (step, fine) in [1., 0.1, 0.15, 0.2].into_iter().enumerate() {
        settings.reconstruction.as_mut().unwrap().texture.fine = fine;
        let mut profile = Profile::new(crate::profile::Mode::Wall);
        let Some(super::super::PhotoRender::Rendered(mut artifacts)) =
            super::super::driver::render_working_photo(
                &engine,
                &mut reduced,
                key,
                &settings,
                &["final"],
                Purpose::Edit,
                &|| false,
                &mut profile,
                None,
            )
            .unwrap()
        else {
            panic!("render")
        };
        let Content::Image(image) = artifacts.remove(0).content else {
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
        // A target too small for the visible photo reports the overage rather
        // than dropping the latest edit's inputs after every render.
        let trim = engine.trim_memory(&mut reduced, 0, &reservation);
        assert!(trim.over_target_bytes > 0);
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
