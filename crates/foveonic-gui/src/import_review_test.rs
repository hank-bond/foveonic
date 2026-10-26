//! Checks frozen Browse selection processing and destination-scoped import completion receipts.

use super::*;
use std::{
    fs,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR"]
fn browse_processes_all_highlights_with_frozen_per_photo_edits_and_conflict_policy() {
    use crate::{
        export::{self, Export, ExportFormat},
        preferences::Organizer,
    };
    use foveonic::{
        importing::{ConflictPolicy, Identity},
        profile::Profile,
        render::{RenderSession, Settings},
    };
    let directory = tempfile::tempdir().unwrap();
    let fixture = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let paths: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|folder| {
            let folder = directory.path().join(folder);
            fs::create_dir(&folder).unwrap();
            let path = folder.join("photo.x3f");
            fs::hard_link(&fixture, &path).unwrap();
            path
        })
        .collect();
    let mut recipes: Vec<_> = [0.1, 0.6]
        .into_iter()
        .map(|exposure| {
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.exposure = exposure;
            settings.output_size = Some([80, 120]);
            settings
        })
        .collect();
    for (path, recipe) in paths.iter().zip(&recipes) {
        foveonic::sidecar::save(path, recipe).unwrap();
    }
    let identities: Vec<_> = paths.iter().map(|p| Identity::read(p).unwrap()).collect();
    let destination = directory.path().join("outputs");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("photo.tiff"), b"old render").unwrap();
    let context = egui::Context::default();
    let engine = Engine::new().unwrap();
    let cache = Arc::new(Cache::new());
    cache.configure(Default::default());
    let mut review = Review::new(engine.executor(), cache.clone(), context.clone());
    let mut browser = Browser::new(context.clone());
    browser.files = paths.clone();
    let mut input = egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        ..Default::default()
    };
    let mut browser_actions = Vec::new();
    for invocation in crate::shortcuts::capture(
        &context,
        &mut input,
        crate::shortcuts::Availability {
            enabled: true,
            browser: true,
            import: browser.import_mode,
            settings: false,
        },
    ) {
        if let crate::shortcuts::Action::Browser(action) = invocation.action {
            browser_actions.push((action, invocation.modifiers));
        }
    }
    browser.set_actions(browser_actions);
    let _ = context.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            browser.organizer(ui, Some(&paths[0]), Organizer::Icons);
        });
    });
    assert_eq!(browser.processing_selection(Some(&paths[0])), paths);
    review.rescan(&mut browser, Some(&destination));
    review.prepare_browse(&browser, Some(&paths[0]));
    let deadline = Instant::now() + Duration::from_secs(15);
    while review.scanning {
        review.update(&mut browser);
        assert!(Instant::now() < deadline, "selected recipes did not load");
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut sidecars = sidecars::Session::new(context.clone(), true, cache.clone());
    sidecars.select(paths[0].clone());
    let deadline = Instant::now() + Duration::from_secs(5);
    while sidecars.receive().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    recipes[0].exposure = 1.2;
    sidecars.edited(&recipes[0]);
    let priority = cache.foreground();
    let mut options = processing::Settings {
        format: ExportFormat::Tiff,
        ..Default::default()
    };
    review.submit(&browser, &sidecars, &options);
    assert_eq!(review.worker.progress().total, 2);
    assert!(!review.ready(&browser), "a second batch must wait");
    options.format = ExportFormat::Dng;
    options.conflict = ConflictPolicy::Rename;
    assert_eq!(options.formats(), [ExportFormat::Dng]);
    assert_eq!(options.conflict, ConflictPolicy::Rename);
    let mut later = recipes[0].clone();
    later.exposure = 2.2;
    sidecars.edited(&later);
    browser.files.clear();
    review.prepare_browse(&browser, None);
    let source = Arc::new(foveonic::source::Source::open(&fixture).unwrap());
    for (index, recipe) in recipes.iter().enumerate() {
        export::write(
            &engine,
            &mut RenderSession::new(source.clone()),
            recipe,
            &Export {
                path: directory.path().join(format!("expected-{index}.tiff")),
                format: ExportFormat::Tiff,
                tiff_color_space: None,
                conflict: ConflictPolicy::Overwrite,
            },
            &mut Profile::default(),
            |_| {},
        )
        .unwrap();
    }
    assert_eq!(review.worker.progress().completed, 0, "foreground must win");
    assert_eq!(
        fs::read(destination.join("photo.tiff")).unwrap(),
        b"old render"
    );
    drop(priority);
    let deadline = Instant::now() + Duration::from_secs(90);
    while review.worker.progress().running {
        assert!(Instant::now() < deadline, "Browse batch did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
    let progress = review.worker.progress();
    assert_eq!(progress.completed, 2);
    assert!(progress.errors.is_empty(), "{:?}", progress.errors);
    assert!(progress.imported.is_empty());
    for path in &paths {
        assert!(
            cache.open_cached(path, |_| {}).is_none(),
            "batch cache misses must not populate the browsing cache"
        );
    }
    for (index, name) in ["photo.tiff", "photo-1.tiff"].into_iter().enumerate() {
        assert_eq!(
            fs::read(destination.join(name)).unwrap(),
            fs::read(directory.path().join(format!("expected-{index}.tiff"))).unwrap()
        );
    }
    assert_eq!(
        fs::read_dir(&destination).unwrap().count(),
        2,
        "Browse must only write rendered outputs"
    );
    for (path, identity) in paths.iter().zip(identities) {
        assert_eq!(Identity::read(path).unwrap(), identity);
    }
}
#[test]
fn stale_scan_replays_completions_without_erasing_later_picks_or_hidden_choices() {
    let mut browser = Browser::new(egui::Context::default());
    browser.import_mode = true;
    let path = PathBuf::from("card/image.x3f");
    browser.files = vec![path.clone()];
    browser.marks.insert(path.clone(), Mark::Pick);
    let progress = batch::Progress {
        destination: Some("photos".into()),
        imported: vec![path.clone()],
        ..Default::default()
    };
    apply_imports(
        &mut browser,
        &progress,
        Some(std::path::Path::new("photos")),
        0,
        0,
    );
    assert_eq!(browser.mark(&path), Mark::Imported);
    assert!(!browser.marks.contains_key(&path));
    // A scan started before completion replaces recorded status. A new pick
    // made after the first receipt must survive replaying that completion.
    for mark in [Mark::Pick, Mark::Hidden] {
        browser.recorded_marks.clear();
        browser.marks.insert(path.clone(), mark);
        apply_imports(
            &mut browser,
            &progress,
            Some(std::path::Path::new("photos")),
            0,
            1,
        );
        assert_eq!(browser.recorded_marks[&path], Mark::Imported);
        assert_eq!(browser.mark(&path), mark);
    }
    browser.recorded_marks.clear();
    apply_imports(
        &mut browser,
        &progress,
        Some(std::path::Path::new("elsewhere")),
        0,
        0,
    );
    assert!(browser.recorded_marks.is_empty());
}
