//! Checks exact decoded persistence, source identity, corruption recovery and LRU eviction.

use super::worker::{entry_state, FamilyCatalog};
use super::*;
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn speculative_executor_never_waits_for_foreground_on_its_own_thread() {
    let fixture = Fixture::new();
    let cache = Cache::at(fixture.root.join("speculation"));
    let foreground = cache.priority().demand(crate::priority::Priority::Viewport);
    assert!(cache
        .priority()
        .speculate(|_| panic!("foreground must win"))
        .is_none());
    drop(foreground);
    assert_eq!(
        cache.priority().speculate(|cancel| {
            assert!(!cancel());
            let foreground = cache.priority().demand(crate::priority::Priority::Viewport);
            assert!(cancel());
            drop(foreground);
            assert!(
                cancel(),
                "completed foreground work still interrupts the attempt"
            );
            7
        }),
        Some(7)
    );
    assert_eq!(cache.priority().speculate(|cancel| !cancel()), Some(true));
}

#[test]
fn preparation_snapshot_uses_only_verified_local_neighborhood_entries() {
    let fixture = Fixture::new();
    let (cache, _foreground, identity) = fixture.ready_cache();
    {
        let mut state = cache.control.lock().unwrap();
        state.nearby = vec![fixture.source.clone(), fixture.root.join("not-ready.x3f")];
    }
    fs::remove_file(&fixture.source).unwrap();
    let (generation, paths) = cache.preparation_window();
    assert_eq!(paths, vec![fixture.source.clone()]);
    assert!(cache.preparation_is_current(generation));
    assert_eq!(
        cache.source_origin(&fixture.source),
        Some(SourceOrigin::File(identity))
    );
    cache.configure(Settings {
        enabled: false,
        ..Default::default()
    });
    assert!(!cache.preparation_is_current(generation));
    assert!(cache.preparation_window().1.is_empty());
}

#[test]
fn diagnostic_namespace_is_confined_to_temporary_cache_storage() {
    let root = PathBuf::from("temporary-cache");
    assert_eq!(diagnostic_root(root.clone(), None).unwrap(), root);
    assert_eq!(
        diagnostic_root(root.clone(), Some(std::ffi::OsStr::new("scenario-123"))).unwrap(),
        root.join("diagnostics/scenario-123")
    );
    for invalid in [
        "",
        ".",
        "..",
        "../other",
        "/absolute",
        "folder/name",
        "with space",
    ] {
        assert!(diagnostic_root(root.clone(), Some(std::ffi::OsStr::new(invalid))).is_err());
    }
}

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
            origin: Default::default(),
            metadata: Arc::new(Metadata {
                width: 8,
                height: 6,
                rotation: 90,
                processing_version: Some([3, 1, 2, 0]),
                properties: [("CAMMODEL".into(), "SIGMA dp0 Quattro".into())].into(),
                matrices: Default::default(),
                lists: Default::default(),
            }),
            pixels: (0..72).map(|i| i as f32 - 2048.).collect::<Vec<_>>().into(),
            storage: SourceStorage::PackedRgbBluePhases,
            camf: vec![],
            timings: vec![],
        }
    }
    fn ready_cache(&self) -> (Cache, crate::priority::Demand, Identity) {
        let cache = Cache::at(self.root.join("cache"));
        let foreground = cache.priority().demand(crate::priority::Priority::Viewport);
        let identity = Identity::read(&self.source).unwrap();
        let settings = Settings::default();
        store(
            &cache.root,
            &self.source,
            identity.clone(),
            &self.image(),
            DEFAULT_BUDGET,
        )
        .unwrap();
        super::selection::prepare(
            &cache.root,
            &self.source,
            &identity,
            &self.image().metadata,
            &fs::read(&self.source).unwrap(),
            &settings,
            &[],
            &|| true,
        )
        .unwrap();
        {
            let mut control = cache.control.lock().unwrap();
            control.settings = settings;
            control.watched = vec![self.source.clone()];
            control
                .identities
                .insert(self.source.clone(), identity.clone());
        }
        {
            let mut status = cache.status.lock().unwrap();
            status.ready.insert(self.source.clone());
            Arc::make_mut(&mut status.files).insert(self.source.clone(), EntryState::Ready);
        }
        (cache, foreground, identity)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn obsolete_decode_thread_preference_does_not_limit_processing_workers() {
    let legacy: Settings =
        serde_json::from_str(r#"{"enabled":true,"max_bytes":10000000000,"decode_threads":1}"#)
            .unwrap();
    assert_eq!(legacy, Settings::default());
    assert!(serde_json::to_value(legacy)
        .unwrap()
        .get("decode_threads")
        .is_none());
}

#[test]
fn defaults_enable_only_merrill_and_quattro_with_ten_decimal_gigabytes() {
    let settings = Settings::default();
    assert_eq!(settings.max_bytes, 10_000_000_000);
    assert_eq!((settings.prefetch_behind, settings.prefetch_ahead), (3, 7));
    assert!(settings.permits(ProcessingFamily::Merrill));
    assert!(settings.permits(ProcessingFamily::Quattro));
    assert!(!settings.permits(ProcessingFamily::Classic));
}

#[test]
fn failed_payload_read_revokes_ready_and_header_refresh_cannot_restore_it() {
    let fixture = Fixture::new();
    let (cache, _foreground, identity) = fixture.ready_cache();
    let file = cache.root.join(identity.name());
    let valid = fs::read(&file).unwrap();
    let mut corrupt = valid.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    fs::write(&file, corrupt).unwrap();
    assert!(read_header(
        &mut File::open(&file).unwrap(),
        &identity,
        &Settings::default()
    )
    .is_ok());

    assert!(cache.open_cached(&fixture.source, |_| {}).is_none());
    let status = cache.status();
    assert!(!status.ready.contains(&fixture.source));
    assert_eq!(
        status.files.get(&fixture.source),
        Some(&EntryState::Uncached)
    );
    assert!(cache.source_origin(&fixture.source).is_none());
    assert!(cache.selection(&fixture.source).is_none());
    super::worker::refresh(
        &cache.root,
        &cache.control,
        &cache.status,
        &mut FamilyCatalog::new(),
    )
    .unwrap();
    assert!(!cache.status().ready.contains(&fixture.source));
    assert!(file.exists(), "cache-read failures do not delete bundles");

    fs::write(&file, valid).unwrap();
    assert!(cache.open_cached(&fixture.source, |_| {}).is_some());
    super::worker::refresh(
        &cache.root,
        &cache.control,
        &cache.status,
        &mut FamilyCatalog::new(),
    )
    .unwrap();
    assert!(cache.status().ready.contains(&fixture.source));
}

#[test]
fn missing_ready_decode_immediately_loses_its_trusted_identity() {
    let fixture = Fixture::new();
    let (cache, _foreground, identity) = fixture.ready_cache();
    fs::remove_file(cache.root.join(identity.name())).unwrap();
    assert!(cache.open_cached(&fixture.source, |_| {}).is_none());
    assert!(!cache.status().ready.contains(&fixture.source));
    assert_eq!(
        cache.status().files.get(&fixture.source),
        Some(&EntryState::Uncached)
    );
    assert!(cache.import_identity(&fixture.source).is_none());
}

#[test]
fn interrupted_cache_reads_and_policy_misses_do_not_reject_valid_payloads() {
    let fixture = Fixture::new();
    let (cache, _foreground, identity) = fixture.ready_cache();
    crate::priority::interruptible(cache.gate.clone(), || {
        let _interrupt = cache.priority().demand(crate::priority::Priority::Viewport);
        assert!(cache.open_cached(&fixture.source, |_| {}).is_none());
    });
    assert!(cache.status().ready.contains(&fixture.source));
    assert!(!cache.control.lock().unwrap().rejected.contains(&identity));
    cache.control.lock().unwrap().settings.quattro = false;
    assert!(cache.open_cached(&fixture.source, |_| {}).is_none());
    cache.control.lock().unwrap().settings = Settings {
        max_bytes: 1,
        ..Settings::default()
    };
    assert!(cache.open_cached(&fixture.source, |_| {}).is_none());
    assert!(!cache.control.lock().unwrap().rejected.contains(&identity));
    assert!(cache
        .control
        .lock()
        .unwrap()
        .identities
        .contains_key(&fixture.source));
    cache.control.lock().unwrap().settings = Settings::default();
    assert!(cache.open_cached(&fixture.source, |_| {}).is_some());
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR"]
fn complete_selection_uses_only_local_cache_when_the_original_and_sidecar_are_unavailable() {
    let fixture = Fixture::new();
    let original = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    fs::copy(original, &fixture.source).unwrap();
    let recipe = crate::render::Settings::new("Sunlight", "Standard");
    crate::sidecar::save(&fixture.source, &recipe).unwrap();
    let cache = Cache::at(fixture.root.join("cache"));
    cache.configure(Settings::default());
    cache.prefetch_window(vec![fixture.source.clone()], Some(fixture.source.clone()));
    let deadline = Instant::now() + Duration::from_secs(15);
    while !cache.status().ready.contains(&fixture.source) {
        assert!(
            Instant::now() < deadline,
            "complete cache preparation timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let _foreground = cache.priority().demand(crate::priority::Priority::Viewport);
    let identity = Identity::read(&fixture.source).unwrap();
    let selection = cache.selection(&fixture.source).unwrap();
    assert_eq!(selection.settings, Some(recipe.clone()));
    assert!(selection.thumbnail.is_some());
    let preview = cache.preview(&fixture.source).unwrap();
    fs::rename(&fixture.source, fixture.root.join("removed.x3f")).unwrap();
    assert_eq!(
        cache.source_bytes(&fixture.source),
        Some(
            fs::metadata(fixture.root.join("removed.x3f"))
                .unwrap()
                .len()
        )
    );
    fs::rename(
        crate::sidecar::path(&fixture.source),
        fixture.root.join("removed.json"),
    )
    .unwrap();
    assert_eq!(
        cache.capture_properties(&fixture.source).unwrap(),
        selection.properties
    );
    assert_eq!(
        cache.selection(&fixture.source).unwrap().settings,
        Some(recipe)
    );
    assert_eq!(cache.preview(&fixture.source).unwrap().rgb, preview.rgb);
    let expected_identity = cache.import_identity(&fixture.source).unwrap();
    let destination = fixture.root.join("imported.x3f");
    assert!(cache
        .copy_original(
            &fixture.source,
            &destination,
            &expected_identity,
            crate::importing::ConflictPolicy::Rename
        )
        .unwrap());
    assert_eq!(
        fs::read(&destination).unwrap(),
        fs::read(fixture.root.join("removed.x3f")).unwrap()
    );
    fs::write(&destination, b"previous imported original").unwrap();
    assert!(cache
        .copy_original(
            &fixture.source,
            &destination,
            &expected_identity,
            crate::importing::ConflictPolicy::Overwrite
        )
        .unwrap());
    assert_eq!(
        fs::read(&destination).unwrap(),
        fs::read(fixture.root.join("removed.x3f")).unwrap()
    );
    let source = cache.open(&fixture.source, |_| {}).unwrap();
    assert_eq!(source.metadata.properties, selection.properties);
    assert_eq!(cache.status().hits, 1);
    assert_eq!(
        fs::metadata(fixture.root.join("imported.x3f"))
            .unwrap()
            .modified()
            .unwrap(),
        fs::metadata(fixture.root.join("removed.x3f"))
            .unwrap()
            .modified()
            .unwrap()
    );
    let cached_original = cache.root.join(identity.name()).with_extension("original");
    let mut damaged = fs::read(&cached_original).unwrap();
    damaged[0] ^= 1;
    fs::write(&cached_original, &damaged).unwrap();
    let rejected = fixture.root.join("rejected.x3f");
    assert!(!cache
        .copy_original(
            &fixture.source,
            &rejected,
            &expected_identity,
            crate::importing::ConflictPolicy::Rename
        )
        .unwrap());
    assert!(
        !rejected.exists(),
        "a corrupt cached original must not be imported"
    );
    damaged[0] ^= 1;
    fs::write(cached_original, damaged).unwrap();
    let mut changed = crate::render::Settings::new("Shade", "Vivid");
    changed.exposure = 0.5;
    cache.saved_settings(&fixture.source, &changed);
    assert_eq!(
        cache.selection(&fixture.source).unwrap().settings,
        Some(changed)
    );
    cache.status.lock().unwrap().ready.remove(&fixture.source);
    drop(_foreground);
    cache.writer.send(Command::Maintain).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cache.status().ready.contains(&fixture.source) {
        assert!(
            Instant::now() < deadline,
            "cached selection must survive background refresh while the original is unavailable"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn compact_payload_preserves_signed_and_unsigned_codes_across_chunks() {
    let fixture = Fixture::new();
    let root = fixture.root.join("cache");
    let identity = Identity::read(&fixture.source).unwrap();
    for signed in [false, true] {
        let mut source = fixture.image();
        Arc::make_mut(&mut source.metadata).width = 1024;
        Arc::make_mut(&mut source.metadata).height = 344;
        *source.samples_mut() = (0..1024 * 172 * 3)
            .map(|i| {
                let code = i as u16;
                if signed {
                    f32::from(code as i16)
                } else {
                    f32::from(code)
                }
            })
            .collect();
        assert!(source.pixels.len() > SAMPLES_PER_CHUNK);
        store(
            &root,
            &fixture.source,
            identity.clone(),
            &source,
            DEFAULT_BUDGET,
        )
        .unwrap();
        let path = root.join(identity.name());
        let mut file = File::open(&path).unwrap();
        let header = read_header(&mut file, &identity, &Settings::default()).unwrap();
        let header_bytes = serde_json::to_vec(&header).unwrap().len() as u64;
        assert_eq!(
            file.metadata().unwrap().len(),
            PREFIX_BYTES + header_bytes + source.pixels.len() as u64 * 2 + source.camf.len() as u64
        );
        let restored = load(&path, &identity, &Settings::default(), &|_| {}).unwrap();
        assert_eq!(
            bytemuck::cast_slice::<f32, u8>(source.samples().as_ref()),
            bytemuck::cast_slice::<f32, u8>(restored.samples().as_ref())
        );
    }
}

#[test]
fn merrill_unsigned_cache_stays_compact_and_preserves_codes() {
    let fixture = Fixture::new();
    let root = fixture.root.join("cache");
    let identity = Identity::read(&fixture.source).unwrap();
    let mut source = fixture.image();
    let metadata = Arc::make_mut(&mut source.metadata);
    metadata.width = 2464;
    metadata.height = 1632;
    metadata
        .properties
        .insert("CAMMODEL".into(), "SIGMA DP2 Merrill".into());
    metadata
        .properties
        .insert("RESOLUTION".into(), "LOW".into());
    source.storage = SourceStorage::InterleavedRgb;
    let codes: Vec<u16> = (0..2464 * 1632 * 3).map(|i| (i % 4096) as u16).collect();
    source.pixels = crate::source::SensorSamples::Unsigned(codes);
    source.validate().unwrap();
    store(
        &root,
        &fixture.source,
        identity.clone(),
        &source,
        DEFAULT_BUDGET,
    )
    .unwrap();
    let restored = load(
        &root.join(identity.name()),
        &identity,
        &Settings::default(),
        &|_| {},
    )
    .unwrap();
    let crate::source::SensorSamples::Unsigned(actual) = &restored.pixels else {
        panic!("Merrill codes should remain compact");
    };
    let crate::source::SensorSamples::Unsigned(expected) = &source.pixels else {
        unreachable!()
    };
    assert_eq!(actual, expected);
    let mut planes: [Vec<u16>; 3] = std::array::from_fn(|_| Vec::with_capacity(expected.len() / 3));
    for pixel in expected.as_chunks::<3>().0 {
        for (plane, &sample) in planes.iter_mut().zip(pixel) {
            plane.push(sample);
        }
    }
    let planar_root = fixture.root.join("planar-cache");
    source.pixels = crate::source::SensorSamples::UnsignedPlanes(planes);
    source.validate().unwrap();
    store(
        &planar_root,
        &fixture.source,
        identity.clone(),
        &source,
        DEFAULT_BUDGET,
    )
    .unwrap();
    assert_eq!(
        fs::read(root.join(identity.name())).unwrap(),
        fs::read(planar_root.join(identity.name())).unwrap()
    );
    let crate::source::SensorSamples::UnsignedPlanes(planes) = &mut source.pixels else {
        unreachable!()
    };
    planes[1].pop();
    assert!(source.validate().is_err());
    let expected_bytes = std::mem::size_of::<Source>()
        + actual.capacity() * std::mem::size_of::<u16>()
        + restored.camf.capacity()
        + restored.timings.capacity() * std::mem::size_of::<crate::profile::Timing>()
        + restored.origin.allocated_bytes();
    assert_eq!(restored.retained_source_bytes(), expected_bytes as u64);

    use std::io::{Read, Seek, SeekFrom, Write};
    let path = root.join(identity.name());
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    read_header(&mut file, &identity, &Settings::default()).unwrap();
    let position = file.stream_position().unwrap() + 17;
    file.seek(SeekFrom::Start(position)).unwrap();
    let mut byte = [0];
    file.read_exact(&mut byte).unwrap();
    file.seek(SeekFrom::Start(position)).unwrap();
    file.write_all(&[byte[0] ^ 1]).unwrap();
    drop(file);
    assert!(load(&path, &identity, &Settings::default(), &|_| {}).is_err());
}

#[test]
fn unrepresentable_samples_leave_existing_entry_intact() {
    let fixture = Fixture::new();
    let root = fixture.root.join("cache");
    let identity = Identity::read(&fixture.source).unwrap();
    let mut source = fixture.image();
    store(
        &root,
        &fixture.source,
        identity.clone(),
        &source,
        DEFAULT_BUDGET,
    )
    .unwrap();
    let path = root.join(identity.name());
    let original = fs::read(&path).unwrap();
    source.samples_mut()[0] = 0.5;
    assert!(store(&root, &fixture.source, identity, &source, DEFAULT_BUDGET).is_err());
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn obsolete_float_cache_is_a_miss() {
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
    let path = root.join(identity.name());
    let mut bytes = fs::read(&path).unwrap();
    bytes[..8].copy_from_slice(b"FOVDEC01");
    fs::write(&path, bytes).unwrap();
    assert!(!ready(&root, &fixture.source, &Settings::default()));
    assert!(load(&path, &identity, &Settings::default(), &|_| {}).is_err());
    store(
        &root,
        &fixture.source,
        identity.clone(),
        &fixture.image(),
        DEFAULT_BUDGET,
    )
    .unwrap();
    assert!(load(&path, &identity, &Settings::default(), &|_| {}).is_ok());
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
        bytemuck::cast_slice::<f32, u8>(source.samples().as_ref()),
        bytemuck::cast_slice::<f32, u8>(restored.samples().as_ref())
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
fn prepared_bundle_defers_publication_and_rechecks_recipe_policy_and_budget() {
    let fixture = Fixture::new();
    let root = fixture.root.join("prepared");
    let source = fixture.image();
    let original = fs::read(&fixture.source).unwrap();
    let identity = Identity::read(&fixture.source).unwrap();
    let prepare_selection = || {
        super::selection::prepare_assets(&fixture.source, &identity, &source.metadata, &original)
            .unwrap()
    };
    let selection = prepare_selection();
    let decoded = crate::decode_pool::cpu(|| prepare_decoded(identity.clone(), &source)).unwrap();
    assert!(!root.exists(), "CPU preparation must not publish files");
    publish_decoded(&root, &fixture.source, &source, decoded, 0, &[], || true).unwrap();
    super::selection::publish(
        &root,
        &fixture.source,
        &original,
        selection,
        &Settings::default(),
        &[],
        &|| false,
    )
    .unwrap();
    assert!(
        !root.exists(),
        "cancelled or over-budget preparation must not publish"
    );

    let stale = prepare_selection();
    let recipe = crate::render::Settings {
        exposure: 0.5,
        ..Default::default()
    };
    crate::sidecar::save(&fixture.source, &recipe).unwrap();
    assert!(super::selection::publish(
        &root,
        &fixture.source,
        &original,
        stale,
        &Settings::default(),
        &[],
        &|| true
    )
    .is_err());
    assert!(
        !root.exists(),
        "a stale recipe must be rejected before original publication"
    );

    let too_small = Settings {
        max_bytes: original.len() as u64 - 1,
        ..Default::default()
    };
    assert!(super::selection::publish(
        &root,
        &fixture.source,
        &original,
        prepare_selection(),
        &too_small,
        &[],
        &|| true
    )
    .is_err());
    assert!(!root.exists());

    let decoded = crate::decode_pool::cpu(|| prepare_decoded(identity.clone(), &source)).unwrap();
    publish_decoded(
        &root,
        &fixture.source,
        &source,
        decoded,
        DEFAULT_BUDGET,
        &[],
        || true,
    )
    .unwrap();
    super::selection::publish(
        &root,
        &fixture.source,
        &original,
        prepare_selection(),
        &Settings::default(),
        &[],
        &|| true,
    )
    .unwrap();
    let selection = super::selection::read(&root, &identity, &Settings::default()).unwrap();
    assert_eq!(selection.settings, Some(recipe));
    assert_eq!(selection.original_checksum, crc32fast::hash(&original));
    assert_eq!(
        fs::read(root.join(identity.name()).with_extension("original")).unwrap(),
        original
    );
    let restored = load(
        &root.join(identity.name()),
        &identity,
        &Settings::default(),
        &|_| {},
    )
    .unwrap();
    assert_eq!(
        bytemuck::cast_slice::<f32, u8>(restored.samples().as_ref()),
        bytemuck::cast_slice::<f32, u8>(source.samples().as_ref())
    );
}

#[test]
fn sidecar_change_during_original_publication_rejects_prepared_selection() {
    let fixture = Fixture::new();
    let root = fixture.root.join("cache");
    fs::create_dir_all(&root).unwrap();
    let source = fixture.image();
    let original = fs::read(&fixture.source).unwrap();
    let identity = Identity::read(&fixture.source).unwrap();
    let prepared =
        super::selection::prepare_assets(&fixture.source, &identity, &source.metadata, &original)
            .unwrap();
    let changed = std::cell::Cell::new(false);
    let updated = crate::render::Settings {
        exposure: 0.75,
        ..Default::default()
    };
    let current = || {
        let original_is_written = fs::read_dir(&root).unwrap().flatten().any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "writing")
                && entry
                    .metadata()
                    .is_ok_and(|metadata| metadata.len() == original.len() as u64)
        });
        if original_is_written && !changed.replace(true) {
            crate::sidecar::save(&fixture.source, &updated).unwrap();
        }
        true
    };
    assert!(super::selection::publish(
        &root,
        &fixture.source,
        &original,
        prepared,
        &Settings::default(),
        &[],
        &current
    )
    .is_err());
    assert!(
        changed.get(),
        "the sidecar must change after the original payload was written"
    );
    assert_eq!(
        fs::read(root.join(identity.name()).with_extension("original")).unwrap(),
        original
    );
    assert!(
        !root
            .join(identity.name())
            .with_extension("metadata")
            .exists(),
        "stale selection metadata must not make the original bundle ready"
    );
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
fn read_only_decoded_entry_remains_usable() {
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
    let path = root.join(identity.name());
    let original = fs::read(&path).unwrap();
    let permissions = fs::metadata(&path).unwrap().permissions();
    let mut read_only = permissions.clone();
    read_only.set_readonly(true);
    fs::set_permissions(&path, read_only).unwrap();
    let loaded = load(&path, &identity, &Settings::default(), &|_| {});
    fs::set_permissions(&path, permissions).unwrap();
    assert!(loaded.is_ok());
    assert_eq!(fs::read(&path).unwrap(), original);
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
    File::options()
        .write(true)
        .open(&oldest)
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1))
        .unwrap();
    File::options()
        .write(true)
        .open(&other)
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(2))
        .unwrap();
    load(&oldest, &identity, &Settings::default(), &|_| {}).unwrap();
    let retained = prune(&root, size, &[]).unwrap();
    assert_eq!(retained.len(), 1);
    assert!(oldest.exists());
    assert!(!other.exists());
    assert!(prune(&root, 0, &[]).unwrap().is_empty());
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
            original: fs::read(&fixture.source).unwrap(),
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
    let root = fixture.root.join("cache");
    let settings = Settings {
        classic: true,
        ..Settings::default()
    };
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
        let original = Source::open(&path).unwrap();
        let identity = Identity::read(&path).unwrap();
        store(&root, &path, identity.clone(), &original, DEFAULT_BUDGET).unwrap();
        let restored = load(&root.join(identity.name()), &identity, &settings, &|_| {}).unwrap();
        assert!(
            bytemuck::cast_slice::<f32, u8>(original.samples().as_ref())
                == bytemuck::cast_slice::<f32, u8>(restored.samples().as_ref()),
            "cached samples changed: {name}"
        );
        assert_eq!(
            serde_json::to_value(&*original.metadata).unwrap(),
            serde_json::to_value(&*restored.metadata).unwrap()
        );
        assert_eq!(original.camf, restored.camf);
        assert_eq!(original.storage(), restored.storage());
    }
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
    super::selection::prepare(
        &cache.root,
        &fixture.source,
        &Identity::read(&fixture.source).unwrap(),
        &fixture.image().metadata,
        &fs::read(&fixture.source).unwrap(),
        &Settings::default(),
        &[],
        &|| true,
    )
    .unwrap();
    let foreground = cache.priority().demand(crate::priority::Priority::Viewport);
    cache.configure(Settings::default());
    cache.prefetch_window(vec![fixture.source.clone()], Some(fixture.source.clone()));
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
    assert!(cache.selection(&fixture.source).is_some());
    let external_recipe = crate::render::Settings::new("Shade", "Vivid");
    crate::sidecar::save(&fixture.source, &external_recipe).unwrap();
    cache.writer.send(Command::Maintain).unwrap();
    wait(&|| {
        cache
            .selection(&fixture.source)
            .is_some_and(|s| s.settings.as_ref() == Some(&external_recipe))
    });
    fs::write(&fixture.source, b"edited original at same location").unwrap();
    cache.writer.send(Command::Maintain).unwrap();
    wait(&|| !cache.status().ready.contains(&fixture.source));
    assert!(cache.selection(&fixture.source).is_none());
    assert!(cache.open_cached(&fixture.source, |_| {}).is_none());
    cache.configure(Settings {
        quattro: false,
        ..Settings::default()
    });
    assert!(cache.status().ready.is_empty());
    assert!(
        cache.root.read_dir().unwrap().next().is_some(),
        "disabled families retain disposable entries"
    );
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
    let foreground = cache.priority().demand(crate::priority::Priority::Viewport);
    cache.configure(Settings::default());
    cache.prefetch_window(
        vec![quattro.clone(), merrill.clone(), classic.clone()],
        Some(quattro.clone()),
    );
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(cache.status().entries, 0);
    assert!(!cache.root.exists());
    drop(foreground);
    let deadline = Instant::now() + Duration::from_secs(15);
    while {
        let status = cache.status();
        status.ready.len() < 2 || status.files.get(&classic) != Some(&EntryState::Disabled)
    } {
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
fn prefetch_from_first_photo_follows_browser_order_for_one_window() {
    let fixture = Fixture::new();
    let cache = Cache::at(fixture.root.join("cache"));
    let _foreground = cache.priority().demand(crate::priority::Priority::Viewport);
    let paths = [
        "card/a.x3f",
        "card/DCIM/100SIGMA/a.x3f",
        "card/DCIM/101SIGMA/a.x3f",
    ]
    .map(PathBuf::from)
    .to_vec();
    cache.prefetch_window(paths.clone(), paths.first().cloned());
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
        entry_state(
            &root,
            &fixture.source,
            &policy,
            true,
            &mut families,
            &HashSet::new()
        ),
        EntryState::Queued
    );
    assert_eq!(
        entry_state(
            &root,
            &fixture.source,
            &policy,
            false,
            &mut families,
            &HashSet::new()
        ),
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
    super::selection::prepare(
        &root,
        &fixture.source,
        &Identity::read(&fixture.source).unwrap(),
        &fixture.image().metadata,
        &fs::read(&fixture.source).unwrap(),
        &Settings::default(),
        &[],
        &|| true,
    )
    .unwrap();
    assert_eq!(
        entry_state(
            &root,
            &fixture.source,
            &policy,
            true,
            &mut families,
            &HashSet::new()
        ),
        EntryState::Ready
    );
    let original = root
        .join(Identity::read(&fixture.source).unwrap().name())
        .with_extension("original");
    fs::write(&original, b"truncated").unwrap();
    assert_eq!(
        entry_state(
            &root,
            &fixture.source,
            &policy,
            false,
            &mut families,
            &HashSet::new()
        ),
        EntryState::Uncached,
        "a truncated original must not advertise a complete local bundle"
    );
    fs::write(original, fs::read(&fixture.source).unwrap()).unwrap();
    let identity = Identity::read(&fixture.source).unwrap();
    let metadata = root.join(identity.name()).with_extension("metadata");
    let valid = fs::read(&metadata).unwrap();
    fs::write(&metadata, b"corrupt selection").unwrap();
    assert!(super::selection::read(&root, &identity, &policy).is_err());
    assert_eq!(
        entry_state(
            &root,
            &fixture.source,
            &policy,
            false,
            &mut families,
            &HashSet::new()
        ),
        EntryState::Uncached
    );
    fs::write(metadata, valid).unwrap();
    let disabled = Settings {
        quattro: false,
        ..Settings::default()
    };
    assert_eq!(
        entry_state(
            &root,
            &fixture.source,
            &disabled,
            false,
            &mut families,
            &HashSet::new()
        ),
        EntryState::Disabled
    );
    assert!(
        super::selection::update_recipe(
            &root,
            &Identity::read(&fixture.source).unwrap(),
            Some(crate::render::Settings::new("Sunlight", "Standard")),
            &Settings {
                max_bytes: 1,
                ..policy.clone()
            },
            &[],
            &|| true,
        )
        .is_err(),
        "denied recipe writes must not acknowledge persistence"
    );
    prune(&root, 0, &[]).unwrap();
    assert_eq!(
        entry_state(
            &root,
            &fixture.source,
            &policy,
            false,
            &mut families,
            &HashSet::new()
        ),
        EntryState::Uncached
    );
    fs::remove_file(&fixture.source).unwrap();
    assert_eq!(
        entry_state(
            &root,
            &fixture.source,
            &policy,
            false,
            &mut families,
            &HashSet::new()
        ),
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
    cache.prefetch_window(vec![first.clone(), second.clone()], Some(first.clone()));
    let deadline = Instant::now() + Duration::from_secs(20);
    while !cache.status().prefetching.contains(&first) {
        assert!(Instant::now() < deadline, "prefetch did not start");
        std::thread::sleep(Duration::from_millis(1));
    }
    let cancelling = Instant::now();
    let active = cache.priority().demand(crate::priority::Priority::Viewport);
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
    assert!(!cache.gate.ready(crate::priority::Priority::Speculative));
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
        bytemuck::cast_slice::<f32, u8>(selected.samples().as_ref()),
        bytemuck::cast_slice::<f32, u8>(resumed.samples().as_ref())
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
    let threads = crate::decode_pool::shared().unwrap().threads();
    let cache = Cache::at(fixture.root.join("cache"));
    let settings = Settings::default();
    cache.configure(settings.clone());
    let began = Instant::now();
    cache.prefetch_window(paths.clone(), paths.first().cloned());
    while cache.status().ready.len() != PHOTOS {
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "prefetch stalled: {:?}",
            cache.status()
        );
        assert!(cache.status().prefetching.len() <= threads.min(3));
        std::thread::sleep(Duration::from_millis(5));
    }
    eprintln!(
        "{PHOTOS} Quattro photos, {threads} shared processing workers, at most three admitted photos: {:.1} ms",
        began.elapsed().as_secs_f64() * 1000.
    );
    assert!(cache.status().bytes <= settings.max_bytes);
    let active = cache.priority().demand(crate::priority::Priority::Viewport);
    for path in &paths {
        let restored = cache.open_cached(path, |_| {}).unwrap();
        assert_eq!(
            bytemuck::cast_slice::<f32, u8>(expected.samples().as_ref()),
            bytemuck::cast_slice::<f32, u8>(restored.samples().as_ref())
        );
    }
    drop(active);
}

#[test]
fn selection_replaces_the_window_without_restarting_a_finished_or_cleared_pass() {
    let fixture = Fixture::new();
    let cache = Cache::at(fixture.root.join("cache"));
    let _foreground = cache.priority().demand(crate::priority::Priority::Viewport);
    cache.configure(Settings::default());
    let files: Vec<_> = (0..40).map(|i| PathBuf::from(format!("{i}.x3f"))).collect();
    cache.prefetch_window(files.clone(), Some(files[10].clone()));
    {
        let mut state = cache.control.lock().unwrap();
        assert_eq!(state.queue.len(), 11);
        assert_eq!(state.queue.front(), Some(&files[10]));
        assert_eq!(state.watched, files);
        state.queue.clear(); // The controller has completed this pass.
    }
    cache.prefetch_window(files.clone(), Some(files[10].clone()));
    assert!(cache.control.lock().unwrap().queue.is_empty());
    cache.prefetch_window(files.clone(), Some(files[20].clone()));
    {
        let state = cache.control.lock().unwrap();
        assert_eq!(state.queue.len(), 11);
        assert_eq!(state.queue.front(), Some(&files[20]));
        assert!(state.queue.iter().all(|p| files[17..=27].contains(p)));
    }
    cache.clear();
    cache.prefetch_window(files.clone(), Some(files[20].clone()));
    assert!(cache.control.lock().unwrap().queue.is_empty());
    cache.prefetch_window(files.clone(), None);
    assert!(cache.control.lock().unwrap().queue.is_empty());
    cache.prefetch_window(files.clone(), Some(files[20].clone()));
    cache.configure(Settings {
        prefetch_behind: 0,
        prefetch_ahead: 1,
        ..Settings::default()
    });
    assert_eq!(
        cache
            .control
            .lock()
            .unwrap()
            .queue
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        files[20..22]
    );
}

#[test]
#[ignore = "requires QUATTRO_X3F"]
fn navigation_keeps_nearest_decodes_when_the_window_exceeds_the_disk_budget() {
    let original = PathBuf::from(std::env::var_os("QUATTRO_X3F").unwrap());
    let expected = Source::open(&original).unwrap();
    let fixture = Fixture::new();
    let files: Vec<_> = (0..24)
        .map(|i| {
            let path = fixture.root.join(format!("{i:02}.x3f"));
            fs::hard_link(&original, &path).unwrap();
            path
        })
        .collect();
    let probe = fixture.root.join("probe");
    let identity = Identity::read(&files[0]).unwrap();
    store(
        &probe,
        &files[0],
        identity.clone(),
        &expected,
        DEFAULT_BUDGET,
    )
    .unwrap();
    super::selection::prepare(
        &probe,
        &files[0],
        &identity,
        &expected.metadata,
        &fs::read(&files[0]).unwrap(),
        &Settings::default(),
        &[],
        &|| true,
    )
    .unwrap();
    // Readiness now includes every local companion, not just the decoded planes.
    let entry_bytes = prune(&probe, DEFAULT_BUDGET, &[])
        .unwrap()
        .iter()
        .map(|entry| entry.1)
        .sum::<u64>();
    fs::remove_dir_all(probe).unwrap();
    let cache = Cache::at(fixture.root.join("cache"));
    let settings = Settings {
        max_bytes: entry_bytes * 3 + entry_bytes / 2,
        prefetch_behind: 1,
        prefetch_ahead: 2,
        ..Settings::default()
    };
    cache.configure(settings.clone());
    for index in [5, 18] {
        let began = Instant::now();
        cache.prefetch_window(files.clone(), Some(files[index].clone()));
        let nearest: HashSet<_> = [index, index + 1, index - 1]
            .map(|i| files[i].clone())
            .into();
        let mut observed = HashSet::new();
        loop {
            let status = cache.status();
            for path in status.ready.intersection(&nearest) {
                if observed.insert(path.clone()) {
                    eprintln!(
                        "selection {index}: {} dot ready at {:.1} ms",
                        path.file_name().unwrap().to_string_lossy(),
                        began.elapsed().as_secs_f64() * 1000.
                    );
                }
            }
            if status.ready == nearest
                && status.prefetching.is_empty()
                && cache.control.lock().unwrap().queue.is_empty()
            {
                break;
            }
            assert!(
                began.elapsed() < Duration::from_secs(60),
                "neighborhood stalled: {status:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        // A partial fourth decoded entry may remain. Complete readiness also
        // requires its companions, and all physical files share this budget.
        let stored_bytes: u64 = fs::read_dir(&cache.root)
            .unwrap()
            .map(|entry| entry.unwrap().metadata().unwrap())
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len())
            .sum();
        assert!(stored_bytes <= settings.max_bytes);
        assert!(cache.status().bytes <= settings.max_bytes);
        let _foreground = cache.priority().demand(crate::priority::Priority::Viewport);
        for path in &nearest {
            let identity = Identity::read(path).unwrap();
            let original_copy = cache.root.join(identity.name()).with_extension("original");
            assert!(
                fs::read(original_copy).unwrap() == fs::read(path).unwrap(),
                "complete neighbor original changed: {}",
                path.display()
            );
            assert!(cache.selection(path).is_some());
            let decoded = cache.open_cached(path, |_| {}).unwrap();
            assert!(
                bytemuck::cast_slice::<f32, u8>(decoded.samples().as_ref())
                    == bytemuck::cast_slice::<f32, u8>(expected.samples().as_ref()),
                "complete neighbor decoded samples changed: {}",
                path.display()
            );
        }
        eprintln!(
            "selection {index}: retained current/+1/-1 in a three-decode budget, {:.1} ms",
            began.elapsed().as_secs_f64() * 1000.
        );
    }
    std::thread::sleep(Duration::from_millis(250));
    assert!(cache.control.lock().unwrap().queue.is_empty());
    assert!(cache.status().prefetching.is_empty());
    assert!(cache.status().bytes <= settings.max_bytes);
}

#[test]
#[ignore = "requires QUATTRO_X3F"]
fn compare_active_and_speculative_decode_wall_time() {
    let path = PathBuf::from(std::env::var_os("QUATTRO_X3F").unwrap());
    let began = Instant::now();
    let source = Source::open(&path).unwrap();
    eprintln!(
        "active: {:.1} ms {:?}",
        began.elapsed().as_secs_f64() * 1000.,
        source.load_timings()
    );
    drop(source);
    for workers in [3, 5] {
        let gate = Arc::new(crate::priority::Gate::default());
        let pool = Arc::new(crate::decode_pool::Pool::new(workers).unwrap());
        let order = Arc::new(crate::decode_pool::Order::default());
        std::thread::scope(|scope| {
            for photo in 0..3 {
                let gate = gate.clone();
                let pool = pool.clone();
                let turn = order.issue();
                let path = &path;
                scope.spawn(move || {
                    crate::priority::interruptible(gate, || {
                        let began = Instant::now();
                        crate::priority::decoder(crate::decode_pool::Task { pool, turn }, || {
                            let source = Source::open(path).unwrap();
                            eprintln!(
                                "{workers} layer workers, photo {photo}: {:.1} ms {:?}",
                                began.elapsed().as_secs_f64() * 1000.,
                                source.load_timings()
                            );
                            Ok(())
                        })
                        .unwrap();
                    })
                });
            }
        });
    }
}

pub(crate) fn small_x3f_with_exif() -> Vec<u8> {
    let metadata = Metadata {
        width: 8,
        height: 6,
        rotation: 0,
        processing_version: None,
        properties: [
            ("CAMMODEL".into(), "SIGMA dp0 Quattro".into()),
            ("ISO".into(), "400".into()),
        ]
        .into(),
        matrices: Default::default(),
        lists: Default::default(),
    };
    let exif = crate::capture_metadata::Capture::new(&metadata)
        .payload(
            [8, 6],
            crate::capture_metadata::ExifOutput::Embedded(crate::color_space::TiffColorSpace::Srgb),
        )
        .unwrap();
    let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe1];
    jpeg.extend(((exif.len() + 8) as u16).to_be_bytes());
    jpeg.extend(b"Exif\0\0");
    jpeg.extend(exif);
    let mut image = vec![0; 28];
    image[..4].copy_from_slice(b"SECi");
    image[8..12].copy_from_slice(&2_u32.to_le_bytes());
    image[12..16].copy_from_slice(&18_u32.to_le_bytes());
    image[16..20].copy_from_slice(&8_u32.to_le_bytes());
    image[20..24].copy_from_slice(&6_u32.to_le_bytes());
    image.extend(jpeg);
    let units: Vec<u16> = "CAMMODEL\0SIGMA dp0 Quattro\0".encode_utf16().collect();
    let mut prop = vec![0; 32];
    prop[..4].copy_from_slice(b"SECp");
    prop[4..8].copy_from_slice(&0x20000_u32.to_le_bytes());
    prop[8..12].copy_from_slice(&1_u32.to_le_bytes());
    prop[20..24].copy_from_slice(&(units.len() as u32).to_le_bytes());
    prop[28..32].copy_from_slice(&9_u32.to_le_bytes());
    prop.extend(units.into_iter().flat_map(u16::to_le_bytes));
    let mut file = vec![0_u8; 52];
    file[..4].copy_from_slice(b"FOVb");
    file[4..8].copy_from_slice(&0x20003_u32.to_le_bytes());
    let image_offset = file.len();
    file.extend(&image);
    let prop_offset = file.len();
    file.extend(&prop);
    let directory = file.len();
    file.extend(b"SECd");
    file.extend(0x20000_u32.to_le_bytes());
    file.extend(2_u32.to_le_bytes());
    for (offset, size, kind) in [
        (image_offset, image.len(), *b"IMAG"),
        (prop_offset, prop.len(), *b"PROP"),
    ] {
        file.extend((offset as u32).to_le_bytes());
        file.extend((size as u32).to_le_bytes());
        file.extend(kind);
    }
    file.extend((directory as u32).to_le_bytes());
    file
}

fn make_cached_decode_legacy(cache: &Cache, identity: &Identity) {
    let path = cache.root.join(identity.name());
    let mut bytes = fs::read(&path).unwrap();
    let length = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    let json = &mut bytes[20..20 + length];
    let old = b"\"version\":3";
    let offset = json.windows(old.len()).position(|w| w == old).unwrap();
    json[offset + old.len() - 1] = b'2';
    let checksum = crc32fast::hash(json);
    bytes[16..20].copy_from_slice(&checksum.to_le_bytes());
    fs::write(path, bytes).unwrap();
}

#[test]
fn legacy_decodes_reuse_browser_capture_metadata_without_redecoding() {
    let fixture = Fixture::new();
    fs::write(&fixture.source, small_x3f_with_exif()).unwrap();
    let (cache, _foreground, identity) = fixture.ready_cache();
    make_cached_decode_legacy(&cache, &identity);
    let properties = cache.capture_properties(&fixture.source).unwrap();
    assert!(properties.contains_key("EXIF_CAPTURE_TAGS"));
    assert!(properties.contains_key("EXIF_CAPTURE_CHECKED"));
    fs::remove_file(&fixture.source).unwrap();
    for _ in 0..2 {
        let source = cache
            .open_cached(&fixture.source, |_| {})
            .expect("cached pixels remain usable");
        assert_eq!(source.samples(), fixture.image().samples());
        assert_eq!(
            source.metadata.properties["EXIF_CAPTURE_TAGS"],
            properties["EXIF_CAPTURE_TAGS"]
        );
    }
}

#[test]
fn legacy_decode_survives_missing_or_changed_original() {
    for changed in [false, true] {
        let fixture = Fixture::new();
        fs::write(&fixture.source, small_x3f_with_exif()).unwrap();
        let (cache, _foreground, identity) = fixture.ready_cache();
        make_cached_decode_legacy(&cache, &identity);
        if changed {
            fs::write(&fixture.source, b"new and different original").unwrap();
        } else {
            fs::remove_file(&fixture.source).unwrap();
        }
        let source = cache
            .open_cached(&fixture.source, |_| {})
            .expect("cached pixels remain usable");
        assert_eq!(source.samples(), fixture.image().samples());
        assert!(!source.metadata.properties.contains_key("EXIF_CAPTURE_TAGS"));
    }
}

#[test]
fn a_malformed_recipe_is_never_recorded_in_a_cached_selection() {
    let fixture = Fixture::new();
    let recorded = || super::selection::recorded_recipe(&fixture.source);
    assert_eq!(
        recorded().unwrap(),
        None,
        "a missing recipe records defaults"
    );
    fs::create_dir_all(crate::sidecar::path(&fixture.source).parent().unwrap()).unwrap();
    fs::write(crate::sidecar::path(&fixture.source), b"malformed").unwrap();
    assert!(recorded().is_err(), "readers must find and report it");
}

#[test]
fn an_entry_without_a_recorded_orientation_takes_it_from_the_cached_decode_while_offline() {
    // The fixture's cached properties predate the recorded orientation.
    let fixture = Fixture::new();
    let (cache, _foreground, _identity) = fixture.ready_cache();
    fs::remove_file(&fixture.source).unwrap();
    let properties = cache.capture_properties(&fixture.source).unwrap();
    assert_eq!(properties["X3F_ROTATION"], "90");
}
