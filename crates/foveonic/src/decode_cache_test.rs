//! Checks exact decoded persistence, source identity, corruption recovery and LRU eviction.

use super::worker::{entry_state, FamilyCatalog};
use super::*;
use std::time::Duration;

struct Fixture {
    root: PathBuf,
    source: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "foveonic-cache-test-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("photo.x3f");
        fs::write(&source, b"identity fixture").unwrap();
        Self { root, source }
    }
    fn image(&self) -> Source {
        Source {
            metadata: Arc::new(Metadata {
                width: 8,
                height: 6,
                rotation: 90,
                processing_version: Some([3, 1, 2, 0]),
                properties: [("CAMMODEL".into(), "SIGMA dp0 Quattro".into())].into(),
                matrices: Default::default(),
                lists: Default::default(),
            }),
            pixels: (0..72)
                .map(|i| if i == 0 { -0. } else { i as f32 - 2048. })
                .collect(),
            storage: SourceStorage::PackedRgbBluePhases,
            camf: vec![],
            timings: vec![],
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn decode_thread_budget_restores_legacy_settings_and_clamps_to_available_cores() {
    let legacy: Settings =
        serde_json::from_str(r#"{"enabled":true,"max_bytes":10000000000}"#).unwrap();
    assert_eq!(legacy.decode_threads, available_decode_threads().min(3));
    for (requested, expected) in [(0, 1), (usize::MAX, available_decode_threads())] {
        let fixture = Fixture::new();
        let cache = Cache::at(fixture.root.join("cache"));
        cache.configure(Settings {
            decode_threads: requested,
            ..Settings::default()
        });
        assert_eq!(
            cache.control.lock().unwrap().settings.decode_threads,
            expected
        );
    }
}

#[test]
fn defaults_enable_only_merrill_and_quattro_with_ten_decimal_gigabytes() {
    let settings = Settings::default();
    assert_eq!(settings.max_bytes, 10_000_000_000);
    assert!(settings.permits(ProcessingFamily::Merrill));
    assert!(settings.permits(ProcessingFamily::Quattro));
    assert!(!settings.permits(ProcessingFamily::Classic));
}

#[test]
fn persistence_preserves_sample_bits_geometry_and_source_location() {
    let fixture = Fixture::new();
    let root = fixture.root.join("cache");
    let source = fixture.image();
    let identity = Identity::read(&fixture.source).unwrap();
    store(
        &root,
        &fixture.source,
        identity.clone(),
        &source,
        DEFAULT_BUDGET,
    )
    .unwrap();
    let file = root.join(identity.name());
    let restored = load(&file, &identity, &Settings::default(), &|_| {}).unwrap();
    assert_eq!(
        bytemuck::cast_slice::<f32, u8>(source.samples()),
        bytemuck::cast_slice::<f32, u8>(restored.samples())
    );
    assert_eq!(
        serde_json::to_value(&*source.metadata).unwrap(),
        serde_json::to_value(&*restored.metadata).unwrap()
    );
    assert_eq!(source.storage(), restored.storage());
    let other = fixture.root.join("moved.x3f");
    fs::copy(&fixture.source, &other).unwrap();
    assert!(load(
        &file,
        &Identity::read(&other).unwrap(),
        &Settings::default(),
        &|_| {}
    )
    .is_err());
    File::options()
        .write(true)
        .open(&fixture.source)
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(123))
        .unwrap();
    assert!(load(
        &file,
        &Identity::read(&fixture.source).unwrap(),
        &Settings::default(),
        &|_| {}
    )
    .is_err());
}

#[test]
fn corrupt_truncated_and_oversized_entries_are_rejected() {
    let fixture = Fixture::new();
    let root = fixture.root.join("cache");
    let identity = Identity::read(&fixture.source).unwrap();
    store(
        &root,
        &fixture.source,
        identity.clone(),
        &fixture.image(),
        DEFAULT_BUDGET,
    )
    .unwrap();
    let file = root.join(identity.name());
    let original = fs::read(&file).unwrap();
    let mut metadata_corruption = original.clone();
    metadata_corruption[PREFIX_BYTES as usize + 1] ^= 1;
    fs::write(&file, metadata_corruption).unwrap();
    assert!(load(&file, &identity, &Settings::default(), &|_| {}).is_err());
    let mut corrupt = original.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    fs::write(&file, corrupt).unwrap();
    assert!(load(&file, &identity, &Settings::default(), &|_| {}).is_err());
    fs::write(&file, &original[..original.len() - 1]).unwrap();
    assert!(load(&file, &identity, &Settings::default(), &|_| {}).is_err());
    fs::write(&file, &original).unwrap();
    let small = Settings {
        max_bytes: original.len() as u64 - 1,
        ..Settings::default()
    };
    assert!(load(&file, &identity, &small, &|_| {}).is_err());
    let disabled = Settings {
        quattro: false,
        ..Settings::default()
    };
    assert!(load(&file, &identity, &disabled, &|_| {}).is_err());
}

#[test]
fn successful_reuse_updates_lru_before_eviction() {
    let fixture = Fixture::new();
    let root = fixture.root.join("cache");
    let identity = Identity::read(&fixture.source).unwrap();
    store(
        &root,
        &fixture.source,
        identity.clone(),
        &fixture.image(),
        DEFAULT_BUDGET,
    )
    .unwrap();
    let oldest = root.join(identity.name());
    let other = root.join("other.decode");
    fs::copy(&oldest, &other).unwrap();
    let size = fs::metadata(&oldest).unwrap().len();
    File::open(&oldest)
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1))
        .unwrap();
    File::open(&other)
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(2))
        .unwrap();
    load(&oldest, &identity, &Settings::default(), &|_| {}).unwrap();
    let retained = prune(&root, size).unwrap();
    assert_eq!(retained.len(), 1);
    assert!(oldest.exists());
    assert!(!other.exists());
    assert!(prune(&root, 0).unwrap().is_empty());
}

#[test]
fn background_writes_obey_budget_changes_disable_and_clear() {
    let fixture = Fixture::new();
    let cache = Cache::at(fixture.root.join("cache"));
    cache.configure(Settings::default());
    let identity = Identity::read(&fixture.source).unwrap();
    let path = cache.root.join(identity.name());
    cache
        .writer
        .send(Command::Store {
            path: fixture.source.clone(),
            identity,
            source: Arc::new(fixture.image()),
        })
        .unwrap();
    let wait_for = |done: &dyn Fn() -> bool| {
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        while !done() {
            assert!(
                Instant::now() < deadline,
                "background cache work did not settle"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    wait_for(&|| path.exists());
    cache.configure(Settings {
        enabled: false,
        ..Settings::default()
    });
    assert!(
        cache.open(&fixture.source, |_| {}).is_err(),
        "disabled cache must attempt original decoding"
    );
    assert!(path.exists());
    cache.configure(Settings {
        max_bytes: 0,
        ..Settings::default()
    });
    wait_for(&|| !path.exists() && cache.status().bytes == 0);
    fs::create_dir_all(&cache.root).unwrap();
    fs::write(&path, b"disposable entry").unwrap();
    cache.clear();
    wait_for(&|| !path.exists());
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR with Quattro, Merrill and Classic originals"]
fn original_families_roundtrip_exact_samples_metadata_and_camera_order() {
    let fixtures = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let fixture = Fixture::new();
    let cache = Cache::at(fixture.root.join("cache"));
    cache.configure(Settings {
        classic: true,
        ..Settings::default()
    });
    for name in [
        "dp0-quattro.x3f",
        "sd-quattro-h.x3f",
        "merrill",
        "sd9.x3f",
        "sd15.x3f",
    ] {
        let path = if name == "merrill" {
            PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap())
        } else {
            fixtures.join(name)
        };
        let original = cache.open(&path, |_| {}).unwrap();
        let file = cache.root.join(Identity::read(&path).unwrap().name());
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        while !file.exists() {
            assert!(
                Instant::now() < deadline,
                "cache insertion did not finish: {name}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let restored = cache.open(&path, |_| {}).unwrap();
        assert!(
            bytemuck::cast_slice::<f32, u8>(original.samples())
                == bytemuck::cast_slice::<f32, u8>(restored.samples()),
            "cached samples changed: {name}"
        );
        assert_eq!(
            serde_json::to_value(&*original.metadata).unwrap(),
            serde_json::to_value(&*restored.metadata).unwrap()
        );
        assert_eq!(original.camf, restored.camf);
        assert_eq!(original.storage(), restored.storage());
    }
    assert_eq!(cache.status().hits, 5);
}

#[test]
fn folder_readiness_obeys_foreground_priority_identity_and_policy() {
    let fixture = Fixture::new();
    let cache = Cache::at(fixture.root.join("cache"));
    let identity = Identity::read(&fixture.source).unwrap();
    store(
        &cache.root,
        &fixture.source,
        identity,
        &fixture.image(),
        DEFAULT_BUDGET,
    )
    .unwrap();
    let foreground = cache.foreground();
    cache.configure(Settings::default());
    cache.prefetch_folder(vec![fixture.source.clone()]);
    std::thread::sleep(Duration::from_millis(30));
    assert!(
        cache.status().ready.is_empty(),
        "background scan must yield to foreground work"
    );
    drop(foreground);
    let wait = |done: &dyn Fn() -> bool| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait(&|| cache.status().ready.contains(&fixture.source));
    cache.configure(Settings {
        quattro: false,
        ..Settings::default()
    });
    assert!(cache.status().ready.is_empty());
    assert!(
        cache.root.read_dir().unwrap().next().is_some(),
        "disabled families retain disposable entries"
    );
    fs::write(&fixture.source, b"edited original at same location").unwrap();
    assert!(!ready(&cache.root, &fixture.source, &Settings::default()));
    cache.clear();
    wait(&|| cache.status().entries == 0);
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR and MERRILL_X3F"]
fn original_folder_prefetch_waits_for_foreground_and_skips_disabled_families() {
    let root = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let fixture = Fixture::new();
    let cache = Cache::at(fixture.root.join("cache"));
    let quattro = root.join("dp0-quattro.x3f");
    let merrill = PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap());
    let classic = root.join("sd15.x3f");
    let foreground = cache.foreground();
    cache.configure(Settings::default());
    cache.prefetch_folder(vec![quattro.clone(), merrill.clone(), classic.clone()]);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(cache.status().entries, 0);
    assert!(!cache.root.exists());
    drop(foreground);
    let deadline = Instant::now() + Duration::from_secs(15);
    while cache.status().ready.len() < 2 {
        assert!(
            Instant::now() < deadline,
            "folder prefetch did not finish: {:?}",
            cache.status()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let status = cache.status();
    assert_eq!(status.files.get(&quattro), Some(&EntryState::Ready));
    assert_eq!(status.files.get(&merrill), Some(&EntryState::Ready));
    assert_eq!(status.files.get(&classic), Some(&EntryState::Disabled));
    assert!(status.ready.contains(&quattro));
    assert!(status.ready.contains(&merrill));
    assert!(!status.ready.contains(&classic));
    cache.open(&quattro, |_| {}).unwrap();
    cache.open(&merrill, |_| {}).unwrap();
    assert_eq!(cache.status().hits, 2);
    assert!(cache.status().bytes <= DEFAULT_BUDGET);
}

#[test]
fn folder_prefetch_keeps_browser_order_for_one_pass() {
    let fixture = Fixture::new();
    let cache = Cache::at(fixture.root.join("cache"));
    let _foreground = cache.foreground();
    let paths = [
        "card/a.x3f",
        "card/DCIM/100SIGMA/a.x3f",
        "card/DCIM/101SIGMA/a.x3f",
    ]
    .map(PathBuf::from)
    .to_vec();
    cache.prefetch_folder(paths.clone());
    let mut state = cache.control.lock().unwrap();
    assert_eq!(
        state.queue.iter().collect::<Vec<_>>(),
        paths.iter().collect::<Vec<_>>()
    );
    assert_eq!(state.queue.pop_front(), Some(paths[0].clone()));
    assert_eq!(
        state.queue.iter().collect::<Vec<_>>(),
        paths[1..].iter().collect::<Vec<_>>()
    );
    assert_eq!(state.watched, paths);
}

#[test]
fn file_status_tracks_queue_ready_eviction_and_architecture_policy() {
    let fixture = Fixture::new();
    let root = fixture.root.join("cache");
    let identity = Identity::read(&fixture.source).unwrap();
    let mut families = FamilyCatalog::from([(
        fixture.source.clone(),
        (identity.clone(), Some(ProcessingFamily::Quattro)),
    )]);
    let policy = Settings::default();
    assert_eq!(
        entry_state(&root, &fixture.source, &policy, true, &mut families),
        EntryState::Queued
    );
    assert_eq!(
        entry_state(&root, &fixture.source, &policy, false, &mut families),
        EntryState::Uncached
    );
    store(
        &root,
        &fixture.source,
        identity,
        &fixture.image(),
        DEFAULT_BUDGET,
    )
    .unwrap();
    assert_eq!(
        entry_state(&root, &fixture.source, &policy, true, &mut families),
        EntryState::Ready
    );
    let disabled = Settings {
        quattro: false,
        ..Settings::default()
    };
    assert_eq!(
        entry_state(&root, &fixture.source, &disabled, false, &mut families),
        EntryState::Disabled
    );
    prune(&root, 0).unwrap();
    assert_eq!(
        entry_state(&root, &fixture.source, &policy, false, &mut families),
        EntryState::Uncached
    );
    fs::remove_file(&fixture.source).unwrap();
    assert_eq!(
        entry_state(&root, &fixture.source, &policy, false, &mut families),
        EntryState::Unavailable
    );
}

#[test]
#[ignore = "requires QUATTRO_X3F"]
fn foreground_open_interrupts_inflight_prefetch_then_resumes_browser_order() {
    let original = PathBuf::from(std::env::var_os("QUATTRO_X3F").unwrap());
    let fixture = Fixture::new();
    let first = fixture.root.join("a.x3f");
    let second = fixture.root.join("b.x3f");
    fs::hard_link(&original, &first).unwrap();
    fs::hard_link(&original, &second).unwrap();
    let cache = Cache::at(fixture.root.join("cache"));
    cache.configure(Settings::default());
    cache.prefetch_folder(vec![first.clone(), second.clone()]);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !cache.status().prefetching.contains(&first) {
        assert!(Instant::now() < deadline, "prefetch did not start");
        std::thread::sleep(Duration::from_millis(1));
    }
    let cancelling = Instant::now();
    let active = cache.foreground();
    while !cache.status().prefetching.is_empty() {
        assert!(
            Instant::now() < deadline,
            "prefetch did not unwind during foreground work"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    eprintln!(
        "prefetch cancellation acknowledged: {:.1} ms",
        cancelling.elapsed().as_secs_f64() * 1000.
    );
    {
        let state = cache.control.lock().unwrap();
        assert_eq!(
            state.queue.iter().cloned().collect::<Vec<_>>(),
            [first.clone(), second.clone()]
        );
    }
    let began = Instant::now();
    let selected = cache.open(&second, |_| {}).unwrap();
    eprintln!(
        "foreground source open during interrupted prefetch: {:.1} ms",
        began.elapsed().as_secs_f64() * 1000.
    );
    assert!(!cache.gate.idle());
    assert!(!ready(&cache.root, &first, &Settings::default()));
    drop(active);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !cache.status().ready.contains(&first) {
        assert!(
            Instant::now() < deadline,
            "prefetch did not resume: {:?}",
            cache.status()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let resumed = cache.open(&first, |_| {}).unwrap();
    assert_eq!(
        bytemuck::cast_slice::<f32, u8>(selected.samples()),
        bytemuck::cast_slice::<f32, u8>(resumed.samples())
    );
}

#[test]
#[ignore = "requires QUATTRO_X3F"]
fn parallel_prefetch_throughput_preserves_exact_samples_and_disk_budget() {
    let original = PathBuf::from(std::env::var_os("QUATTRO_X3F").unwrap());
    let fixture = Fixture::new();
    const PHOTOS: usize = 6;
    let paths: Vec<_> = (0..PHOTOS)
        .map(|i| {
            let path = fixture.root.join(format!("{i}.x3f"));
            fs::hard_link(&original, &path).unwrap();
            path
        })
        .collect();
    let expected = Source::open(&original).unwrap();
    let mut budgets = vec![1, available_decode_threads().min(3)];
    budgets.dedup();
    for threads in budgets {
        let cache = Cache::at(fixture.root.join(format!("cache-{threads}")));
        let settings = Settings {
            decode_threads: threads,
            ..Settings::default()
        };
        cache.configure(settings.clone());
        let began = Instant::now();
        cache.prefetch_folder(paths.clone());
        while cache.status().ready.len() != PHOTOS {
            assert!(
                began.elapsed() < Duration::from_secs(60),
                "prefetch stalled: {:?}",
                cache.status()
            );
            assert!(cache.status().prefetching.len() <= threads);
            std::thread::sleep(Duration::from_millis(5));
        }
        eprintln!(
            "{PHOTOS} Quattro photos, {threads} background decode threads: {:.1} ms",
            began.elapsed().as_secs_f64() * 1000.
        );
        assert!(cache.status().bytes <= settings.max_bytes);
        let active = cache.foreground();
        for path in &paths {
            let restored = cache.open_cached(path, |_| {}).unwrap();
            assert_eq!(
                bytemuck::cast_slice::<f32, u8>(expected.samples()),
                bytemuck::cast_slice::<f32, u8>(restored.samples())
            );
        }
        drop(active);
    }
}
