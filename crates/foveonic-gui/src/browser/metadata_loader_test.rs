//! Checks asynchronous metadata failures, rating persistence, folder replacement and Import source protection.

use super::*;
use std::time::{Duration, Instant};

#[test]
fn reopening_uses_saved_ratings_and_rejects_previous_folder_metadata() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for folder in [first.path(), second.path()] {
        std::fs::write(folder.join("a.x3f"), b"metadata fixture").unwrap();
    }
    let context = egui::Context::default();
    let mut browser = super::super::Browser::new(context.clone());
    browser.open(first.path()).unwrap();
    browser.rate(&[first.path().join("a.x3f")], Rating::try_from(4).unwrap());
    browser.open(second.path()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !browser.metadata_ready() {
        assert!(Instant::now() < deadline);
        browser.receive(&context);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(browser
        .records
        .keys()
        .all(|path| path.starts_with(second.path())));
    browser.open(first.path()).unwrap();
    while !browser.metadata_ready() {
        assert!(Instant::now() < deadline);
        browser.receive(&context);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(browser.rating(&first.path().join("a.x3f")).stars(), 4);
    assert!(!foveonic::sidecar::path(&first.path().join("a.x3f")).exists());
}

#[test]
fn import_ratings_remain_in_memory_and_do_not_create_source_files() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("a.x3f");
    std::fs::write(&path, b"original").unwrap();
    let mut browser = super::super::Browser::new(egui::Context::default());
    browser.import_mode = true;
    browser.open(folder.path()).unwrap();
    browser.rate(std::slice::from_ref(&path), Rating::try_from(5).unwrap());
    assert_eq!(browser.changed_rating(&path).unwrap().stars(), 5);
    drop(browser);
    assert!(!review::path(folder.path()).exists());
    assert!(!foveonic::sidecar::path(&path).exists());
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
}

#[test]
fn damaged_metadata_does_not_stop_the_remaining_folder_thumbnails() {
    let folder = tempfile::tempdir().unwrap();
    let bad = folder.path().join("a-damaged.x3f");
    let good = folder.path().join("b-healthy.x3f");
    let mut section = b"SECp".to_vec();
    for word in [0x20000_u32, 512, 0, 0, 65537] {
        section.extend(word.to_le_bytes());
    }
    for i in 0..512_u32 {
        section.extend(i.to_le_bytes());
        section.extend(65536_u32.to_le_bytes());
    }
    section.extend(std::iter::repeat_n(65_u16, 65536).flat_map(u16::to_le_bytes));
    section.extend(0_u16.to_le_bytes());
    let mut bytes = vec![0; 40];
    bytes[..4].copy_from_slice(b"FOVb");
    bytes[4..8].copy_from_slice(&0x20000_u32.to_le_bytes());
    bytes.extend(&section);
    let directory = bytes.len() as u32;
    bytes.extend(b"SECd");
    for word in [0x20000_u32, 1, 40, section.len() as u32] {
        bytes.extend(word.to_le_bytes());
    }
    bytes.extend(b"PROP");
    bytes.extend(directory.to_le_bytes());
    std::fs::write(&bad, &bytes).unwrap();
    bytes.truncate(40);
    bytes.extend(b"SECi");
    for word in [0x20000_u32, 2, 3, 2, 1, 6] {
        bytes.extend(word.to_le_bytes());
    }
    bytes.extend([128_u8; 6]);
    let directory = bytes.len() as u32;
    bytes.extend(b"SECd");
    for word in [0x20000_u32, 1, 40, 34] {
        bytes.extend(word.to_le_bytes());
    }
    bytes.extend(b"IMAG");
    bytes.extend(directory.to_le_bytes());
    std::fs::write(&good, &bytes).unwrap();
    let ctx = eframe::egui::Context::default();
    let mut browser = super::super::Browser::new(ctx.clone());
    browser.open(folder.path()).unwrap();
    assert_eq!(browser.files, vec![bad.clone(), good.clone()]);
    browser.schedule_thumbnails(browser.files.clone());
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        browser.receive(&ctx);
        if browser.cache.contains_key(&bad)
            && browser.cache.contains_key(&good)
            && browser.metadata_ready()
        {
            break;
        }
        assert!(Instant::now() < deadline, "folder loading did not settle");
        std::thread::yield_now();
    }
    assert!(browser.cache[&bad].is_none());
    assert!(browser.thumbnail(&good).is_some());
}

#[test]
fn needed_photos_load_while_rendering_demand_holds_back_background_reads() {
    let folder = tempfile::tempdir().unwrap();
    let paths: Vec<_> = ["a.x3f", "b.x3f"]
        .map(|name| folder.path().join(name))
        .into();
    for path in &paths {
        std::fs::write(path, b"metadata fixture").unwrap();
    }
    let cache = Arc::new(Cache::new());
    let loader = Loader::new(egui::Context::default(), Some(cache.clone()));
    // A selection holds viewport demand until its photo is planned, and
    // planning waits for this photo's metadata summary.
    let _selection = cache
        .priority()
        .demand(foveonic::priority::Priority::Viewport);
    loader.replace(&paths);
    loader.need(&paths[1..]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if loader
            .receive()
            .any(|reply| matches!(reply, Reply::Loaded { path, .. } if path == paths[1]))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "a needed photo waited for rendering demand to end"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_needed_photo_arrives_through_the_browser_and_is_asked_for_again_after_a_reload() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("a.x3f");
    std::fs::write(&path, b"metadata fixture").unwrap();
    let context = egui::Context::default();
    let mut browser = super::super::Browser::new(context.clone());
    let arrive = |browser: &mut super::super::Browser| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while browser.metadata_summary(&path).is_none() {
            browser.need_summaries(std::slice::from_ref(&path));
            browser.receive(&context);
            assert!(Instant::now() < deadline, "needed metadata did not arrive");
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    browser.open(folder.path()).unwrap();
    arrive(&mut browser);
    browser.open(folder.path()).unwrap();
    assert!(
        browser.metadata_summary(&path).is_none(),
        "a reload forgets the old listing's properties"
    );
    arrive(&mut browser);
}
