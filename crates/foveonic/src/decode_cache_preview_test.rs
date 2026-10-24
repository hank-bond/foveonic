//! Checks preview recipe invalidation, stale handoffs, bounded persistence, and shared eviction.

use super::super::prune;
use super::*;
use std::time::{Duration, Instant, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    source: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "foveonic-preview-cache-test-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("photo.x3f");
        fs::write(&source, b"immutable source identity").unwrap();
        Self { root, source }
    }
    fn write(&self, rendered: bool, color: [u8; 3]) -> PendingPreview {
        let preview = Preview {
            rgb: color.repeat(16 * 8),
            extent: [16, 8],
            native_extent: [6000, 3000],
            rotation: 90,
        };
        PendingPreview {
            path: self.source.clone(),
            header: PreviewHeader {
                key: Key::read(&self.source).unwrap(),
                family: ProcessingFamily::Quattro,
                extent: preview.extent,
                native_extent: preview.native_extent,
                rotation: preview.rotation,
                rendered,
                checksum: 0,
            },
            preview,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn preview_reuses_only_the_same_source_and_recipe_and_obeys_policy() {
    let fixture = Fixture::new();
    let cache = Cache::at(fixture.root.join("cache"));
    cache.control.lock().unwrap().settings = Settings::default();
    let key = Key::read(&fixture.source).unwrap();
    write_checked(
        cache.directory(),
        fixture.write(true, [20, 80, 140]),
        &Settings::default(),
        &[],
        &|| true,
    )
    .unwrap();
    let preview = cache.preview(&fixture.source).unwrap();
    assert_eq!(preview.extent, [16, 8]);
    assert_eq!(preview.native_extent, [6000, 3000]);
    assert_eq!(preview.rotation, 90);
    assert!(preview
        .rgb
        .as_chunks::<3>()
        .0
        .iter()
        .all(|p| p.iter().zip([20, 80, 140]).all(|(a, b)| a.abs_diff(b) <= 3)));
    cache.control.lock().unwrap().settings.quattro = false;
    assert!(cache.preview(&fixture.source).is_none());
    cache.control.lock().unwrap().settings.quattro = true;
    let other = fixture.root.join("copy.x3f");
    fs::copy(&fixture.source, &other).unwrap();
    assert!(cache.preview(&other).is_none());
    sidecar::save(
        &fixture.source,
        &crate::render::Settings::new("Sunlight", "Standard"),
    )
    .unwrap();
    assert!(cache.preview(&fixture.source).is_none());
    fs::remove_file(sidecar::path(&fixture.source)).unwrap();
    assert!(cache.preview(&fixture.source).is_some());
    let destination = key.location(cache.directory());
    let mut bytes = fs::read(&destination).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(&destination, bytes).unwrap();
    assert!(cache.preview(&fixture.source).is_none());
    write_checked(
        cache.directory(),
        fixture.write(true, [30, 90, 150]),
        &Settings::default(),
        &[],
        &|| true,
    )
    .unwrap();
    fs::write(&fixture.source, b"changed source").unwrap();
    assert!(cache.preview(&fixture.source).is_none());
}

#[test]
fn late_embedded_and_stale_recipe_jobs_cannot_replace_rendered_previews() {
    let fixture = Fixture::new();
    let root = fixture.root.join("cache");
    let key = Key::read(&fixture.source).unwrap();
    write_checked(
        &root,
        fixture.write(true, [100, 150, 200]),
        &Settings::default(),
        &[],
        &|| true,
    )
    .unwrap();
    let bytes = fs::read(key.location(&root)).unwrap();
    write_checked(
        &root,
        fixture.write(false, [10, 20, 30]),
        &Settings::default(),
        &[],
        &|| true,
    )
    .unwrap();
    assert_eq!(fs::read(key.location(&root)).unwrap(), bytes);
    let stale = fixture.write(true, [200, 50, 10]);
    sidecar::save(
        &fixture.source,
        &crate::render::Settings::new("Shade", "Vivid"),
    )
    .unwrap();
    write_checked(&root, stale, &Settings::default(), &[], &|| true).unwrap();
    assert_eq!(fs::read(key.location(&root)).unwrap(), bytes);
}

#[test]
fn previews_share_the_decoded_entry_budget_and_clear_policy() {
    let fixture = Fixture::new();
    let root = fixture.root.join("cache");
    write_checked(
        &root,
        fixture.write(true, [100; 3]),
        &Settings::default(),
        &[],
        &|| true,
    )
    .unwrap();
    let key = Key::read(&fixture.source).unwrap();
    let bytes = fs::metadata(key.location(&root)).unwrap().len();
    let decoded = root.join("old.decode");
    fs::write(&decoded, vec![0; 1000]).unwrap();
    File::open(&decoded)
        .unwrap()
        .set_modified(UNIX_EPOCH)
        .unwrap();
    let settings = Settings {
        max_bytes: bytes + 100,
        ..Settings::default()
    };
    write_checked(
        &root,
        fixture.write(true, [120; 3]),
        &settings,
        &[],
        &|| true,
    )
    .unwrap();
    assert!(!decoded.exists());
    assert!(key.location(&root).exists());
    assert!(
        prune(&root, settings.max_bytes, &[])
            .unwrap()
            .iter()
            .map(|e| e.1)
            .sum::<u64>()
            <= settings.max_bytes
    );
    assert!(prune(&root, 0, &[]).unwrap().is_empty());
    assert!(!key.location(&root).exists());
}

#[test]
fn rendered_handoff_rejects_settings_that_are_newer_or_older_than_the_recipe() {
    let fixture = Fixture::new();
    let cache = Cache::at(fixture.root.join("cache"));
    cache.control.lock().unwrap().settings = Settings::default();
    let activity = cache.foreground();
    let settings = crate::render::Settings::new("Sunlight", "Standard");
    sidecar::save(&fixture.source, &settings).unwrap();
    let metadata = Metadata {
        width: 8,
        height: 6,
        rotation: 90,
        processing_version: Some([3, 1, 2, 0]),
        properties: [("CAMMODEL".into(), "SIGMA dp0 Quattro".into())].into(),
        matrices: Default::default(),
        lists: Default::default(),
    };
    let mut edited = settings.clone();
    edited.exposure = 1.;
    cache
        .save_preview(
            &fixture.source,
            &edited,
            &metadata,
            fixture.write(true, [100; 3]).preview,
        )
        .unwrap();
    assert!(cache.control.lock().unwrap().preview.is_none());
    cache
        .save_preview(
            &fixture.source,
            &settings,
            &metadata,
            fixture.write(true, [100; 3]).preview,
        )
        .unwrap();
    assert!(cache.control.lock().unwrap().preview.take().is_some());
    drop(activity);
    cache
        .save_preview(
            &fixture.source,
            &settings,
            &metadata,
            fixture.write(true, [120; 3]).preview,
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while cache.preview(&fixture.source).is_none() {
        assert!(
            Instant::now() < deadline,
            "asynchronous preview insertion did not complete"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let destination = Key::read(&fixture.source)
        .unwrap()
        .location(cache.directory());
    cache.clear();
    while destination.exists() {
        assert!(
            Instant::now() < deadline,
            "clear retained the preview companion"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR and MERRILL_X3F"]
fn original_large_previews_keep_orientation_and_skip_cropped_camera_jpegs() {
    let fixture = Fixture::new();
    let cache = Cache::at(fixture.root.join("cache"));
    cache.control.lock().unwrap().settings = Settings::default();
    let root = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let merrill = PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap());
    for (path, family, available) in [
        (
            root.join("dp0-quattro.x3f"),
            ProcessingFamily::Quattro,
            false,
        ),
        (
            root.join("dp2-quattro.x3f"),
            ProcessingFamily::Quattro,
            true,
        ),
        (merrill, ProcessingFamily::Merrill, true),
    ] {
        let thumbnail = crate::preview::read(&path).unwrap();
        embedded(
            cache.directory(),
            &path,
            family,
            &Settings::default(),
            &[],
            &|| true,
        )
        .unwrap();
        let preview = cache.preview(&path);
        assert_eq!(preview.is_some(), available, "{}", path.display());
        if let Some(preview) = preview {
            assert_eq!(preview.rotation, thumbnail.rotation);
            assert!(preview.extent.iter().all(|n| *n <= MAX_SIDE));
            assert_eq!(preview.extent.into_iter().max(), Some(MAX_SIDE));
            assert_eq!(preview.native_extent, thumbnail.native_extent);
            assert!(read(
                &Key::read(&path).unwrap().location(cache.directory()),
                &Key::read(&path).unwrap(),
                &Settings::default()
            )
            .is_ok_and(|(h, _)| !h.rendered));
        }
    }
}
