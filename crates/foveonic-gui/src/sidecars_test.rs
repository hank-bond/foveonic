//! Checks asynchronous recipe restore, coalesced latest writes, shutdown flushing and visible protected failures.

use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "foveonic-gui-sidecar-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn input(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn receive(session: &mut Session) -> Settings {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(settings) = session.receive() {
            return settings;
        }
        assert!(Instant::now() < deadline, "recipe load did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn failure(session: &mut Session) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        session.receive();
        if session.warning().is_some() {
            return;
        }
        assert!(Instant::now() < deadline, "missing visible save failure");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn restore_per_photo_choices_and_do_not_write_just_by_viewing() {
    let directory = Directory::new();
    let a = directory.input("a.x3f");
    let b = directory.input("b.x3f");
    let mut saved = Settings::new("Shade", "Vivid");
    saved.exposure = 1.5;
    sidecar::save(&a, &saved).unwrap();
    let before = fs::read(sidecar::path(&a)).unwrap();
    let mut session = Session::new(egui::Context::default(), true);
    session.select(a.clone());
    assert_eq!(receive(&mut session), saved);
    session.refresh(&saved);
    session.select(b.clone());
    assert_eq!(receive(&mut session), Settings::new("", ""));
    session.refresh(&Settings::new("Sunlight", "Standard"));
    drop(session);
    assert_eq!(fs::read(sidecar::path(&a)).unwrap(), before);
    assert!(!sidecar::path(&b).exists());
}

#[test]
fn requested_latest_edits_survive_photo_switches_and_orderly_shutdown() {
    let directory = Directory::new();
    let a = directory.input("a.x3f");
    let b = directory.input("b.x3f");
    let mut session = Session::new(egui::Context::default(), true);
    session.select(a.clone());
    let mut a_settings = receive(&mut session);
    for step in 0..100 {
        a_settings.exposure = step as f32 * 0.01;
        session.edited(&a_settings);
    }
    // Metadata may resolve after editing; save the requested settings, not a completed frame.
    a_settings.white_balance = "Sunlight".into();
    a_settings.color_mode = "Standard".into();
    session.refresh(&a_settings);
    session.select(b.clone());
    let mut b_settings = receive(&mut session);
    b_settings.sensor_gain = 1.7;
    session.edited(&b_settings);
    session.select(a.clone());
    assert_eq!(receive(&mut session), a_settings);
    session.clear();
    drop(session);
    assert_eq!(sidecar::load(&a).unwrap(), Some(a_settings));
    assert_eq!(sidecar::load(&b).unwrap(), Some(b_settings));
}

#[test]
fn replaced_loads_cannot_initialize_a_new_selection() {
    let directory = Directory::new();
    let a = directory.input("a.x3f");
    let b = directory.input("b.x3f");
    sidecar::save(&a, &Settings::new("Shade", "Vivid")).unwrap();
    sidecar::save(&b, &Settings::new("Sunlight", "Standard")).unwrap();
    let mut session = Session::new(egui::Context::default(), true);
    session.select(a);
    session.select(b);
    assert_eq!(receive(&mut session), Settings::new("Sunlight", "Standard"));
    session.clear();
    assert!(!session.ready());
    assert!(session.receive().is_none());
}

#[test]
fn corrupt_or_future_recipes_stay_visible_and_cannot_be_overwritten() {
    let directory = Directory::new();
    let input = directory.input("a.x3f");
    let bytes = b"{\"format\":\"foveonic-editing\",\"version\":2}";
    fs::write(sidecar::path(&input), bytes).unwrap();
    let mut session = Session::new(egui::Context::default(), true);
    session.select(input.clone());
    assert_eq!(receive(&mut session), Settings::new("", ""));
    assert!(session
        .warning()
        .unwrap()
        .contains("automatic saving is disabled"));
    session.edited(&Settings::new("Shade", "Vivid"));
    assert!(!session.can_retry());
    drop(session);
    assert_eq!(fs::read(sidecar::path(&input)).unwrap(), bytes);
}

#[test]
fn failed_saves_remain_visible_after_selection_changes_and_can_retry_latest_edits() {
    let directory = Directory::new();
    let input = directory.input("a.x3f");
    let mut session = Session::new(egui::Context::default(), true);
    session.select(input.clone());
    let mut settings = receive(&mut session);
    fs::create_dir(sidecar::path(&input)).unwrap();
    settings.exposure = 1.;
    session.edited(&settings);
    failure(&mut session);
    assert!(session.can_retry());
    session.select(directory.input("b.x3f"));
    receive(&mut session);
    assert!(session.warning().is_some());
    fs::remove_dir(sidecar::path(&input)).unwrap();
    session.select(input.clone());
    assert_eq!(receive(&mut session), settings);
    assert!(
        session.warning().is_some(),
        "reselecting must not hide an unsaved recipe"
    );
    session.retry();
    drop(session);
    assert_eq!(sidecar::load(&input).unwrap(), Some(settings));
}

#[test]
fn smoke_sessions_neither_read_nor_write_fixture_recipes() {
    let directory = Directory::new();
    let input = directory.input("a.x3f");
    fs::write(sidecar::path(&input), b"fixture recipe").unwrap();
    let mut session = Session::new(egui::Context::default(), false);
    session.select(input.clone());
    assert_eq!(receive(&mut session), Settings::new("", ""));
    assert!(session.warning().is_none());
    session.edited(&Settings::new("Sunlight", "Standard"));
    drop(session);
    assert_eq!(fs::read(sidecar::path(&input)).unwrap(), b"fixture recipe");
}
