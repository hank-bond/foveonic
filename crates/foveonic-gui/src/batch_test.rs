//! Checks frozen batch exports, copy-only imports, failures, and source-side write isolation.

use super::*;
use std::time::{Duration, Instant};

#[test]
fn saving_eye_and_delete_decisions_preserves_import_provenance_and_undoes_manual_hides() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let imported = source.path().join("imported.x3f");
    let hidden = source.path().join("hidden.x3f");
    fs::write(&imported, b"original").unwrap();
    fs::write(&hidden, b"hidden original").unwrap();
    let imported_identity = Identity::read(&imported).unwrap();
    let hidden_identity = Identity::read(&hidden).unwrap();
    importing::copy_original(
        &imported,
        &destination.path().join("imported.x3f"),
        &imported_identity,
    )
    .unwrap();
    let mut history = History::default();
    history.set(Record {
        identity: imported_identity.clone(),
        source: imported.display().to_string(),
        disposition: Disposition::Imported {
            original: "imported.x3f".into(),
        },
    });
    apply_decisions(
        &mut history,
        destination.path(),
        vec![
            Decision {
                path: imported.clone(),
                identity: imported_identity.clone(),
                hidden: true,
            },
            Decision {
                path: hidden.clone(),
                identity: hidden_identity.clone(),
                hidden: true,
            },
        ],
    );
    assert!(matches!(
        history.disposition(destination.path(), &imported_identity),
        Some(Disposition::Imported { .. })
    ));
    assert!(matches!(
        history.disposition(destination.path(), &hidden_identity),
        Some(Disposition::Hidden)
    ));
    apply_decisions(
        &mut history,
        destination.path(),
        vec![
            Decision {
                path: imported,
                identity: imported_identity.clone(),
                hidden: false,
            },
            Decision {
                path: hidden,
                identity: hidden_identity.clone(),
                hidden: false,
            },
        ],
    );
    history.save(destination.path()).unwrap();
    let history = History::load(destination.path()).unwrap();
    assert!(matches!(
        history.disposition(destination.path(), &imported_identity),
        Some(Disposition::Imported { .. })
    ));
    assert!(history
        .disposition(destination.path(), &hidden_identity)
        .is_none());
}

#[test]
fn scanning_restores_destination_decisions_without_decoding_originals() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let path = source.path().join("image.x3f");
    // Intentionally not an X3F. A saved recipe plus filesystem metadata is enough.
    fs::write(&path, b"camera bytes").unwrap();
    let settings = Settings::new("Shade", "Vivid");
    foveonic::sidecar::save(&path, &settings).unwrap();
    let identity = Identity::read(&path).unwrap();
    let mut history = History::default();
    history.set(Record {
        identity,
        source: path.display().to_string(),
        disposition: Disposition::Hidden,
    });
    history.save(destination.path()).unwrap();
    let scan = scan_files(
        Scan {
            generation: 7,
            destination: Some(destination.path().to_owned()),
            files: vec![path.clone()],
        },
        3,
        || {},
    );
    assert!(scan.errors.is_empty());
    assert_eq!(scan.generation, 7);
    assert_eq!(scan.sources[&path].settings.as_ref().unwrap(), &settings);
    assert_eq!(scan.completed_imports, 3);
    assert!(matches!(scan.dispositions[&path], Disposition::Hidden));
}

#[test]
fn history_matching_and_hidden_identity_do_not_require_a_readable_editing_recipe() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let path = source.path().join("image.x3f");
    fs::write(&path, b"camera bytes").unwrap();
    let sidecar = foveonic::sidecar::path(&path);
    fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
    fs::write(sidecar, b"invalid recipe").unwrap();
    let identity = Identity::read(&path).unwrap();
    let mut history = History::default();
    history.set(Record {
        identity: identity.clone(),
        source: path.display().to_string(),
        disposition: Disposition::Hidden,
    });
    history.save(destination.path()).unwrap();
    let scan = scan_files(
        Scan {
            generation: 1,
            destination: Some(destination.path().to_owned()),
            files: vec![path.clone()],
        },
        0,
        || {},
    );
    assert_eq!(scan.errors.len(), 1);
    assert_eq!(scan.sources[&path].identity, identity);
    assert!(scan.sources[&path].settings.is_err());
    assert!(matches!(scan.dispositions[&path], Disposition::Hidden));
}

#[test]
#[ignore = "requires GPU and QUATTRO_X3F"]
fn frozen_batch_matches_full_exports_and_does_not_block_foreground_or_write_source() {
    let path = PathBuf::from(std::env::var_os("QUATTRO_X3F").unwrap());
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("rendered");
    let copied = directory.path().join("copied");
    let source_sidecar = fs::read(foveonic::sidecar::path(&path)).ok();
    let engine = Engine::new().unwrap();
    let cache = Arc::new(Cache::new());
    let worker = Worker::new(engine.executor(), cache.clone(), egui::Context::default());
    let active = cache.foreground();
    let mut requested = Settings::new("Sunlight", "Standard");
    requested.exposure = 0.35;
    requested.output_size = Some([600, 400]);
    let frozen = requested.clone();
    let output_space = Some(foveonic::render::TiffColorSpace::AdobeRgb);
    let mut output_settings = frozen.clone();
    output_settings.tiff_color_space = output_space;
    let identity = Identity::read(&path).unwrap();
    let failed = directory.path().join("missing.x3f");
    fs::write(&failed, b"will disappear").unwrap();
    let missing = Photo {
        path: failed.clone(),
        identity: Identity::read(&failed).unwrap(),
        settings: frozen.clone(),
    };
    fs::remove_file(failed).unwrap();
    assert!(worker.submit(Batch {
        destination: destination.clone(),
        photos: vec![
            Photo {
                path: path.clone(),
                identity: identity.clone(),
                settings: requested.clone()
            },
            missing
        ],
        decisions: Vec::new(),
        formats: vec![ExportFormat::Tiff, ExportFormat::Dng],
        tiff_color_space: output_space,
    }));
    requested.exposure = 1.35;
    // Complete actual foreground work while the submitted batch waits behind it.
    let source = Arc::new(foveonic::source::Source::open(&path).unwrap());
    let session = RenderSession::new(source);
    let baseline = engine
        .render(session.source(), &output_settings, &mut Profile::default())
        .unwrap();
    engine
        .write_tiff(
            &baseline,
            &directory.path().join("expected.tiff"),
            &mut Profile::default(),
        )
        .unwrap();
    drop(baseline);
    foveonic::dng::write_source(
        &engine,
        session.source(),
        &output_settings,
        &directory.path().join("expected.dng"),
        &mut Profile::default(),
    )
    .unwrap();
    let changed = engine
        .render(session.source(), &requested, &mut Profile::default())
        .unwrap();
    engine.device().poll(eframe::wgpu::Maintain::Wait);
    drop(changed);
    assert_eq!(worker.progress().completed, 0);
    assert!(worker.progress().running);
    drop(session);
    drop(active);
    let deadline = Instant::now() + Duration::from_secs(90);
    while worker.progress().running {
        assert!(Instant::now() < deadline, "batch did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
    let progress = worker.progress();
    assert_eq!(progress.completed, 2);
    assert_eq!(progress.imported, vec![path.clone()]);
    assert_eq!(progress.errors.len(), 1, "{:?}", progress.errors);
    for extension in ["tiff", "dng"] {
        assert_eq!(
            fs::read(directory.path().join(format!("expected.{extension}"))).unwrap(),
            fs::read(
                destination
                    .join(path.file_name().unwrap())
                    .with_extension(extension)
            )
            .unwrap()
        );
    }
    let original = destination.join("x3f").join(path.file_name().unwrap());
    assert_eq!(
        foveonic::sidecar::load(&original).unwrap(),
        Some(frozen.clone())
    );
    assert_eq!(Identity::read(&original).unwrap(), identity);
    fs::create_dir(&copied).unwrap();
    let before = engine.gpu_activity();
    let relative = import_photo(
        &engine,
        &cache,
        &Photo {
            path: path.clone(),
            identity: identity.clone(),
            settings: frozen.clone(),
        },
        &copied,
        &[],
        None,
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(relative, PathBuf::from(path.file_name().unwrap()));
    assert_eq!(
        serde_json::to_value(engine.gpu_activity()).unwrap(),
        serde_json::to_value(before).unwrap(),
        "copy-only import must not render"
    );
    // A history publication failure belongs to the same photo operation as
    // output creation. Keep prior successful files and roll back only the retry.
    let failed_original =
        copied.join(importing::available_name(&path, &copied, &copied, &[]).unwrap());
    let error = import_photo(
        &engine,
        &cache,
        &Photo {
            path: path.clone(),
            identity,
            settings: frozen,
        },
        &copied,
        &[],
        None,
        |_| Err("cannot publish history".into()),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cannot publish history"));
    assert!(!failed_original.exists());
    assert!(!foveonic::sidecar::path(&failed_original).exists());
    assert!(copied.join(&relative).is_file());
    assert_eq!(
        fs::read(foveonic::sidecar::path(&path)).ok(),
        source_sidecar
    );
}
