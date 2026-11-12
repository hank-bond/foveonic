//! Checks asynchronous rating persistence, replacement-folder replies and Import source protection.

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
