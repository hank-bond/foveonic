//! Checks that restored and unsaved edits survive reloading, failures, and shutdown.

use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(1);

#[test]
fn idle_recipe_snapshot_uses_requested_then_saved_then_camera_defaults_without_io() {
    let path = PathBuf::from("unavailable-card/photo.x3f");
    let saved = Settings {
        exposure: 1.,
        ..Default::default()
    };
    let requested = Settings {
        exposure: -1.,
        ..Default::default()
    };
    let selection = || {
        serde_json::from_value(serde_json::json!({
            "properties":{}, "settings":saved, "thumbnail":null, "original_checksum":0,
        }))
        .unwrap()
    };
    let mut recipes = CachedRecipes {
        defaults: Default::default(),
        enabled: true,
        recent: Default::default(),
    };
    assert_eq!(recipes.resolve(&path, selection()).unwrap(), saved);
    recipes.recent.insert(path.clone(), requested.clone());
    assert_eq!(recipes.resolve(&path, selection()).unwrap(), requested);
    recipes.recent.clear();
    recipes.enabled = false;
    assert_eq!(
        recipes.resolve(&path, selection()).unwrap(),
        Settings::default()
    );
}
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
/// A photo's settings as the editing runtime resolves them.
fn load(session: &mut Session, path: &std::path::Path) -> Settings {
    session.resolve_settings(&[path.to_path_buf()]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        session.receive();
        if let Some(reply) = session.take_resolved().pop() {
            assert_eq!(reply.path, path);
            return reply.settings;
        }
        assert!(
            Instant::now() < deadline,
            "recipe resolution did not complete"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn failure(session: &mut Session) {
    wait(session, "missing visible save failure", |session| {
        session
            .warning()
            .is_some_and(|warning| warning.contains("Edits are not saved"))
    });
}
fn settle_saves(session: &mut Session) {
    wait(session, "saves did not finish", |session| {
        session.unsaved.is_empty()
    });
}
fn wait(session: &mut Session, message: &str, done: impl Fn(&Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        session.receive();
        if done(session) {
            return;
        }
        assert!(Instant::now() < deadline, "{message}");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn session(enabled: bool) -> Session {
    Session::new(
        egui::Context::default(),
        enabled,
        Arc::new(foveonic::decode_cache::Cache::new()),
    )
}

#[test]
fn pasted_settings_reach_every_photo_and_freeze_independently() {
    let directory = Directory::new();
    let paths: Vec<_> = ["a.x3f", "b.x3f", "c.x3f"]
        .map(|name| directory.input(name))
        .into();
    let mut session = session(true);
    let mut copied = Settings::new("Shade", "Vivid");
    copied.exposure = 0.7;
    for path in &paths {
        session.apply(path.clone(), copied.clone());
    }
    let frozen = session.requested_settings(&paths[1]).unwrap().clone();
    copied.exposure = 1.4;
    session.apply(paths[0].clone(), copied.clone());
    assert_eq!(load(&mut session, &paths[1]), frozen);
    drop(session);
    assert_eq!(sidecar::load(&paths[0]).unwrap().settings(), Some(copied));
    for path in &paths[1..] {
        assert_eq!(
            sidecar::load(path).unwrap().settings(),
            Some(frozen.clone())
        );
    }
}

#[test]
fn pasted_import_settings_stay_in_memory_without_source_writes() {
    let directory = Directory::new();
    let a = directory.input("a.x3f");
    let b = directory.input("b.x3f");
    let mut session = session(true);
    session.set_memory_only(true);
    let mut copied = Settings::new("Shade", "Vivid");
    copied.exposure = 0.7;
    session.apply(a.clone(), copied.clone());
    session.apply(b.clone(), copied.clone());
    assert_eq!(load(&mut session, &b), copied);
    assert_eq!(session.requested_settings(&a), Some(&copied));
    drop(session);
    assert!(!sidecar::path(&a).exists());
    assert!(!sidecar::path(&b).exists());
}

#[test]
fn import_edits_stay_in_memory_and_are_discarded_on_exit() {
    let directory = Directory::new();
    let a = directory.input("a.x3f");
    let saved = Settings::new("Shade", "Vivid");
    sidecar::save(&a, &saved).unwrap();
    let before = fs::read(sidecar::path(&a)).unwrap();
    let mut session = session(true);
    session.set_memory_only(true);
    let mut edited = load(&mut session, &a);
    edited.exposure = 2.;
    session.apply(a.clone(), edited.clone());
    assert_eq!(load(&mut session, &a), edited);
    session.set_memory_only(false);
    assert_eq!(load(&mut session, &a), saved);
    drop(session);
    assert_eq!(fs::read(sidecar::path(&a)).unwrap(), before);
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
    let mut session = session(true);
    assert_eq!(load(&mut session, &a), saved);
    assert_eq!(load(&mut session, &b), Settings::new("", ""));
    drop(session);
    assert_eq!(fs::read(sidecar::path(&a)).unwrap(), before);
    assert!(!sidecar::path(&b).exists());
}

#[test]
fn latest_edits_survive_reloading_and_orderly_shutdown() {
    let directory = Directory::new();
    let a = directory.input("a.x3f");
    let b = directory.input("b.x3f");
    let mut session = session(true);
    let mut a_settings = load(&mut session, &a);
    for step in 0..100 {
        a_settings.exposure = step as f32 * 0.01;
        session.apply(a.clone(), a_settings.clone());
    }
    let mut b_settings = load(&mut session, &b);
    b_settings.saturation = 1.7;
    session.apply(b.clone(), b_settings.clone());
    // A save may still be in flight. The reload must see the latest edit.
    assert_eq!(load(&mut session, &a), a_settings);
    drop(session);
    assert_eq!(sidecar::load(&a).unwrap().settings(), Some(a_settings));
    assert_eq!(sidecar::load(&b).unwrap().settings(), Some(b_settings));
}

#[test]
fn batch_recipes_retain_latest_browse_edits_after_save_and_freeze_independently() {
    let directory = Directory::new();
    let path = directory.input("photo.x3f");
    let mut session = session(true);
    let mut settings = load(&mut session, &path);
    settings.exposure = 0.7;
    session.apply(path.clone(), settings.clone());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !session.unsaved.is_empty() {
        session.receive();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(session.requested_settings(&path), Some(&settings));
    let frozen = session.requested_settings(&path).unwrap().clone();
    settings.exposure = 1.4;
    session.apply(path.clone(), settings.clone());
    assert_eq!(frozen.exposure, 0.7);
    assert_eq!(session.requested_settings(&path), Some(&settings));
    session.set_memory_only(false);
    assert!(session.requested.is_empty());
}

#[test]
fn failed_saves_remain_visible_and_can_retry_latest_edits() {
    let directory = Directory::new();
    let input = directory.input("a.x3f");
    let mut session = session(true);
    let mut settings = load(&mut session, &input);
    fs::create_dir(sidecar::path(&input)).unwrap();
    settings.exposure = 1.;
    session.apply(input.clone(), settings.clone());
    failure(&mut session);
    assert!(session.can_retry());
    load(&mut session, &directory.input("b.x3f"));
    assert!(session.warning().is_some());
    fs::remove_dir(sidecar::path(&input)).unwrap();
    assert_eq!(load(&mut session, &input), settings);
    assert!(
        session.warning().is_some(),
        "reloading must not hide an unsaved recipe"
    );
    session.retry();
    drop(session);
    assert_eq!(sidecar::load(&input).unwrap().settings(), Some(settings));
}

#[test]
fn smoke_sessions_neither_read_nor_write_fixture_recipes() {
    let directory = Directory::new();
    let input = directory.input("a.x3f");
    fs::write(sidecar::path(&input), b"fixture recipe").unwrap();
    let mut session = session(false);
    assert_eq!(load(&mut session, &input), Settings::new("", ""));
    assert!(session.warning().is_none());
    session.apply(input.clone(), Settings::new("Sunlight", "Standard"));
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
        let fresh = capture_defaults(MetadataSummary::from_properties(
            [("CAMMODEL".into(), model.into())].into(),
        ))
        .unwrap();
        assert_eq!(fresh.reconstruction, Some(Default::default()), "{model}");
    }
}

#[test]
#[ignore = "requires MERRILL_X3F"]
fn merrill_capture_defaults_are_not_saved_and_saved_off_stays_off() {
    let directory = Directory::new();
    let path = directory.input("merrill.x3f");
    fs::hard_link(std::env::var_os("MERRILL_X3F").unwrap(), &path).unwrap();
    let mut session = session(true);
    let mut settings = load(&mut session, &path);
    assert!(settings.reconstruction.is_some());
    drop(session);
    assert!(
        !sidecar::path(&path).exists(),
        "viewing must not create a recipe"
    );

    let mut session = self::session(true);
    settings.reconstruction = None;
    session.apply(path.clone(), settings);
    drop(session);
    let saved = fs::read(sidecar::path(&path)).unwrap();
    assert_eq!(sidecar::decode(&saved).unwrap().reconstruction, None);

    let mut session = self::session(true);
    assert_eq!(load(&mut session, &path).reconstruction, None);
    drop(session);
    assert_eq!(fs::read(sidecar::path(&path)).unwrap(), saved);
}

#[test]
fn fresh_capture_defaults_follow_the_recorded_adjustments() {
    let properties = [
        ("CAMMODEL".into(), "SIGMA dp2 Quattro".into()),
        ("X3F_ADJUSTMENT_EXPOSURE".into(), "0.75".into()),
        ("X3F_ADJUSTMENT_CONTRAST".into(), "-0.5".into()),
        ("X3F_ADJUSTMENT_SATURATION".into(), "1".into()),
        ("X3F_ADJUSTMENT_SHARPNESS".into(), "-1".into()),
    ]
    .into();
    let fresh = capture_defaults(MetadataSummary::from_properties(properties)).unwrap();
    assert_eq!(fresh.exposure, 0.75);
    assert_eq!(fresh.tone.contrast, -0.5);
    assert_eq!(fresh.saturation, 1.5);
    assert_eq!(fresh.sharpen_amount, None);
}

#[test]
fn neutral_saved_recipe_is_not_reinitialized_from_capture() {
    let directory = Directory::new();
    let path = directory.input("neutral.x3f");
    let saved = Settings::default();
    sidecar::save(&path, &saved).unwrap();
    let mut session = session(true);
    assert_eq!(load(&mut session, &path), saved);
}

#[test]
fn fresh_monochrome_marks_select_monochrome_saved_tones() {
    let settings = capture_defaults(foveonic::source::MetadataSummary::from_properties(
        [
            ("CAMMODEL".into(), "SIGMA sd Quattro".into()),
            ("X3F_MARK_BITS".into(), "2".into()),
            ("X3F_ADJUSTMENT_EXPOSURE".into(), "0.75".into()),
            ("X3F_ADJUSTMENT_MONO_EXPOSURE".into(), "-0.5".into()),
        ]
        .into(),
    ))
    .unwrap();
    assert!(settings.is_monochrome());
    assert_eq!(settings.exposure, -0.5);
}

#[test]
fn fresh_capture_defaults_read_the_recorded_mixture() {
    let fresh = capture_defaults(foveonic::source::MetadataSummary::from_properties(
        [
            ("CAMMODEL".into(), "SIGMA sd Quattro".into()),
            ("X3F_ADJUSTMENT_MONO_MIX_RED".into(), "10813".into()),
            ("X3F_ADJUSTMENT_MONO_MIX_GREEN".into(), "10813".into()),
            ("X3F_ADJUSTMENT_MONO_MIX_BLUE".into(), "11142".into()),
        ]
        .into(),
    ))
    .unwrap();
    assert_eq!(
        fresh.monochrome.mix,
        [10813. / 32768., 10813. / 32768., 11142. / 32768.]
    );
}

#[test]
fn editing_subset_resolution_replaces_old_reads_and_keeps_import_edits_in_memory() {
    let directory = Directory::new();
    let a = directory.input("a.x3f");
    let b = directory.input("b.x3f");
    let mut saved = Settings::new("Sunlight", "Standard");
    saved.exposure = 0.3;
    sidecar::save(&a, &saved).unwrap();
    sidecar::save(&b, &saved).unwrap();
    let mut session = Session::new(
        egui::Context::default(),
        true,
        Arc::new(foveonic::decode_cache::Cache::new()),
    );
    session.resolve_settings(std::slice::from_ref(&a));
    session.resolve_settings(std::slice::from_ref(&b));
    let deadline = Instant::now() + Duration::from_secs(5);
    let resolved = loop {
        session.receive();
        let replies = session.take_resolved();
        if !replies.is_empty() {
            break replies;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].path, b);
    assert_eq!(resolved[0].settings, saved);
    session.set_memory_only(true);
    let mut edited = saved.clone();
    edited.exposure = 1.7;
    session.apply(b.clone(), edited.clone());
    session.resolve_settings(std::slice::from_ref(&b));
    let resolved = session.take_resolved();
    assert_eq!(resolved[0].settings, edited);
    drop(session);
    assert_eq!(sidecar::load(&b).unwrap().settings(), Some(saved));
}

#[test]
fn malformed_recipes_open_with_defaults_and_a_warning_until_the_next_edit_replaces_them() {
    let directory = Directory::new();
    for (name, bytes) in [
        ("broken.x3f", b"broken recipe".as_slice()),
        (
            "future.x3f",
            b"{\"format\":\"foveonic-editing\",\"version\":2}".as_slice(),
        ),
    ] {
        let input = directory.input(name);
        fs::write(sidecar::path(&input), bytes).unwrap();
        let mut session = session(true);
        assert_eq!(load(&mut session, &input), Settings::new("", ""), "{name}");
        assert!(
            session
                .warning()
                .is_some_and(|warning| warning.contains("opened with defaults")),
            "{name}"
        );
        let edited = Settings::new("Shade", "Vivid");
        session.apply(input.clone(), edited.clone());
        settle_saves(&mut session);
        assert!(session.warning().is_none(), "{name}: the edit replaced it");
        drop(session);
        assert_eq!(
            sidecar::load(&input).unwrap().settings(),
            Some(edited),
            "{name}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_recipe_that_cannot_be_read_opens_with_defaults_and_is_not_replaced() {
    use std::os::unix::fs::PermissionsExt;
    let directory = Directory::new();
    let input = directory.input("a.x3f");
    let saved = Settings::new("Shade", "Vivid");
    sidecar::save(&input, &saved).unwrap();
    let recipe = sidecar::path(&input);
    fs::set_permissions(&recipe, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&recipe).is_ok() {
        return; // Permissions do not restrict this user, such as root.
    }
    let mut session = session(true);
    assert_eq!(load(&mut session, &input), Settings::new("", ""));
    assert!(session.warning().is_some());
    session.apply(input.clone(), Settings::new("Sunlight", "Standard"));
    failure(&mut session);
    drop(session);
    fs::set_permissions(&recipe, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(sidecar::load(&input).unwrap().settings(), Some(saved));
}
