//! Checks that restored and unsaved edits survive photo switches, failures, and shutdown.

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
        fs::create_dir(path.join("foveonic")).unwrap();
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
fn import_edits_survive_photo_switches_without_writes_and_are_discarded_on_exit() {
    let directory = Directory::new();
    let a = directory.input("a.x3f");
    let b = directory.input("b.x3f");
    let saved = Settings::new("Shade", "Vivid");
    sidecar::save(&a, &saved).unwrap();
    let before = fs::read(sidecar::path(&a)).unwrap();
    let mut session = Session::new(
        egui::Context::default(),
        true,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
    session.set_memory_only(true);
    session.select(a.clone());
    let mut edited = receive(&mut session);
    edited.exposure = 2.;
    session.edited(&edited);
    session.select(b.clone());
    receive(&mut session);
    session.select(a.clone());
    assert_eq!(receive(&mut session), edited);
    assert_eq!(session.review_settings(&a), Some(&edited));
    session.set_memory_only(false);
    session.select(a.clone());
    assert_eq!(receive(&mut session), saved);
    drop(session);
    assert_eq!(fs::read(sidecar::path(&a)).unwrap(), before);
    assert!(!sidecar::path(&b).exists());
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
    let mut session = Session::new(
        egui::Context::default(),
        true,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
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
    let mut session = Session::new(
        egui::Context::default(),
        true,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
    session.select(a.clone());
    let mut a_settings = receive(&mut session);
    for step in 0..100 {
        a_settings.exposure = step as f32 * 0.01;
        session.edited(&a_settings);
    }
    // An edit can happen before metadata arrives. Saving a completed frame
    // instead of the requested controls would lose that newer edit.
    a_settings.white_balance = "Sunlight".into();
    a_settings.color_mode = "Standard".into();
    session.refresh(&a_settings);
    session.select(b.clone());
    let mut b_settings = receive(&mut session);
    b_settings.saturation = 1.7;
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
    let mut session = Session::new(
        egui::Context::default(),
        true,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
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
    let mut session = Session::new(
        egui::Context::default(),
        true,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
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
    let mut session = Session::new(
        egui::Context::default(),
        true,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
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
    let mut session = Session::new(
        egui::Context::default(),
        false,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
    session.select(input.clone());
    assert_eq!(receive(&mut session), Settings::new("", ""));
    assert!(session.warning().is_none());
    session.edited(&Settings::new("Sunlight", "Standard"));
    drop(session);
    assert_eq!(fs::read(sidecar::path(&input)).unwrap(), b"fixture recipe");
}

#[test]
fn fresh_merrill_capture_defaults_enable_noise_before_rendering() {
    for model in [
        "SIGMA DP1 Merrill",
        "SIGMA DP2 Merrill",
        "SIGMA DP3 Merrill",
        "SIGMA SD1 Merrill",
    ] {
        let fresh = capture_defaults([("CAMMODEL".into(), model.into())].into()).unwrap();
        assert_eq!(fresh.reconstruction, Some(Default::default()), "{model}");
        let selected = selection_settings(fresh.clone(), &["Shade".into(), "Vivid".into()]);
        assert_eq!(selected.reconstruction, fresh.reconstruction);
    }
}

#[test]
#[ignore = "requires MERRILL_X3F"]
fn merrill_preview_quality_does_not_save_defaults_or_reenable_saved_off() {
    let directory = Directory::new();
    let path = directory.input("merrill.x3f");
    fs::hard_link(std::env::var_os("MERRILL_X3F").unwrap(), &path).unwrap();
    let mut session = Session::new(
        egui::Context::default(),
        true,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
    session.select(path.clone());
    let mut settings = receive(&mut session);
    assert!(settings.reconstruction.is_some());
    for quality in [
        foveonic::render::PreviewQuality::Fast,
        foveonic::render::PreviewQuality::Full,
    ] {
        settings.preview_quality = quality;
        session.refresh(&settings);
    }
    drop(session);
    assert!(
        !sidecar::path(&path).exists(),
        "viewing must not create a recipe"
    );

    let mut session = Session::new(
        egui::Context::default(),
        true,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
    session.select(path.clone());
    settings = receive(&mut session);
    settings.reconstruction = None;
    session.edited(&settings);
    drop(session);
    let saved = fs::read(sidecar::path(&path)).unwrap();
    assert_eq!(sidecar::decode(&saved).unwrap().reconstruction, None);

    let mut session = Session::new(
        egui::Context::default(),
        true,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
    session.select(path.clone());
    let mut restored = receive(&mut session);
    assert_eq!(restored.reconstruction, None);
    for quality in [
        foveonic::render::PreviewQuality::Fast,
        foveonic::render::PreviewQuality::Full,
    ] {
        restored.preview_quality = quality;
        session.refresh(&restored);
    }
    drop(session);
    assert_eq!(fs::read(sidecar::path(&path)).unwrap(), saved);
}

#[test]
fn fresh_capture_defaults_precede_launch_choices_and_early_edits() {
    let properties = [
        ("CAMMODEL".into(), "SIGMA dp2 Quattro".into()),
        ("X3F_ADJUSTMENT_EXPOSURE".into(), "0.75".into()),
        ("X3F_ADJUSTMENT_CONTRAST".into(), "-0.5".into()),
        ("X3F_ADJUSTMENT_SATURATION".into(), "1".into()),
        ("X3F_ADJUSTMENT_SHARPNESS".into(), "-1".into()),
    ]
    .into();
    let fresh = capture_defaults(properties).unwrap();
    let mut selected = selection_settings(fresh, &["Shade".into(), "Vivid".into()]);
    assert_eq!(selected.white_balance, "Shade");
    assert_eq!(selected.color_mode, "Vivid");
    assert_eq!(selected.exposure, 0.75);
    assert_eq!(selected.tone.contrast, -0.5);
    assert_eq!(selected.saturation, 1.5);
    assert_eq!(selected.sharpen_amount, None);
    selected.exposure = 0.;
    // Selection does not interpret explicit neutral edits as missing defaults.
    let restored = selection_settings(selected.clone(), &[String::new(), String::new()]);
    assert_eq!(restored, selected);
}

#[test]
fn neutral_saved_recipe_is_not_reinitialized_from_capture() {
    let directory = Directory::new();
    let path = directory.input("neutral.x3f");
    let saved = Settings::default();
    sidecar::save(&path, &saved).unwrap();
    let mut session = Session::new(
        egui::Context::default(),
        true,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
    session.select(path);
    assert_eq!(receive(&mut session), saved);
}

#[test]
fn fresh_monochrome_marks_select_monochrome_saved_tones() {
    let settings = capture_defaults(
        [
            ("CAMMODEL".into(), "SIGMA sd Quattro".into()),
            ("X3F_MARK_BITS".into(), "2".into()),
            ("X3F_ADJUSTMENT_EXPOSURE".into(), "0.75".into()),
            ("X3F_ADJUSTMENT_MONO_EXPOSURE".into(), "-0.5".into()),
        ]
        .into(),
    )
    .unwrap();
    assert!(settings.is_monochrome());
    assert_eq!(settings.exposure, -0.5);
}

#[test]
fn saved_mixer_edits_override_fresh_capture_mixture() {
    let mut fresh = capture_defaults(
        [
            ("CAMMODEL".into(), "SIGMA sd Quattro".into()),
            ("X3F_ADJUSTMENT_MONO_MIX_RED".into(), "10813".into()),
            ("X3F_ADJUSTMENT_MONO_MIX_GREEN".into(), "10813".into()),
            ("X3F_ADJUSTMENT_MONO_MIX_BLUE".into(), "11142".into()),
        ]
        .into(),
    )
    .unwrap();
    assert_eq!(
        fresh.monochrome.mix,
        [10813. / 32768., 10813. / 32768., 11142. / 32768.]
    );
    fresh.monochrome.mix = [1., 0., 0.];
    assert_eq!(
        selection_settings(fresh, &[String::new(), String::new()])
            .monochrome
            .mix,
        [1., 0., 0.]
    );
}
