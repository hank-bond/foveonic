//! Checks import copies, compact history, metadata matching, and protected destinations.

use super::*;

#[test]
fn copy_preserves_samples_and_mtime_without_touching_source_or_existing_destination() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let input = source.path().join("IMG0001.x3f");
    let output = destination.path().join("IMG0001.x3f");
    fs::write(&input, b"original camera bytes").unwrap();
    let time = UNIX_EPOCH + std::time::Duration::from_secs(123456);
    File::options()
        .write(true)
        .open(&input)
        .unwrap()
        .set_modified(time)
        .unwrap();
    let identity = Identity::read(&input).unwrap();
    copy_original(&input, &output, &identity, ConflictPolicy::Rename).unwrap();
    assert_eq!(fs::read(&output).unwrap(), b"original camera bytes");
    assert_eq!(fs::metadata(&output).unwrap().modified().unwrap(), time);
    assert_eq!(fs::read_dir(source.path()).unwrap().count(), 1);
    assert!(copy_original(&input, &output, &identity, ConflictPolicy::Rename).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"original camera bytes");
    fs::write(&output, b"old imported bytes").unwrap();
    copy_original(&input, &output, &identity, ConflictPolicy::Overwrite).unwrap();
    assert_eq!(fs::read(&output).unwrap(), b"original camera bytes");
    assert_eq!(fs::metadata(&output).unwrap().modified().unwrap(), time);
    fs::write(&input, b"changed").unwrap();
    assert!(copy_original(
        &input,
        &destination.path().join("changed.x3f"),
        &identity,
        ConflictPolicy::Rename
    )
    .is_err());
    assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 1);
}

#[test]
fn history_retains_one_current_decision_and_recognizes_a_different_mount_path() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let path = source.path().join("IMG0001.x3f");
    fs::write(&path, b"original").unwrap();
    let identity = Identity::read(&path).unwrap();
    let mut history = History::default();
    history.set(Record {
        identity: identity.clone(),
        source: path.display().to_string(),
        disposition: Disposition::Hidden,
    });
    history.save(destination.path()).unwrap();
    let mut history = History::load(destination.path()).unwrap();
    assert!(matches!(
        history.disposition(destination.path(), &identity),
        Some(Disposition::Hidden)
    ));
    let other_mount = tempfile::tempdir().unwrap();
    copy_original(
        &path,
        &other_mount.path().join("IMG0001.x3f"),
        &identity,
        ConflictPolicy::Rename,
    )
    .unwrap();
    assert_eq!(
        Identity::read(&other_mount.path().join("IMG0001.x3f")).unwrap(),
        identity
    );
    let original = PathBuf::from("IMG0001.x3f");
    copy_original(
        &path,
        &destination.path().join(&original),
        &identity,
        ConflictPolicy::Rename,
    )
    .unwrap();
    history.set(Record {
        identity: identity.clone(),
        source: path.display().to_string(),
        disposition: Disposition::Imported {
            original: original.clone(),
        },
    });
    assert_eq!(history.records.len(), 1);
    history.save(destination.path()).unwrap();
    assert!(matches!(
        History::load(destination.path())
            .unwrap()
            .disposition(destination.path(), &identity),
        Some(Disposition::Imported { .. })
    ));
    fs::remove_file(destination.path().join(original)).unwrap();
    assert!(history.disposition(destination.path(), &identity).is_none());
}

#[test]
fn output_failure_and_collision_leave_no_partial_output() {
    let destination = tempfile::tempdir().unwrap();
    let path = destination.path().join("image.dng");
    assert!(write_output(&path, ConflictPolicy::Rename, |temporary| {
        fs::write(temporary, b"partial")?;
        Err("failed render".into())
    })
    .is_err());
    assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 0);
    write_output(&path, ConflictPolicy::Rename, |temporary| {
        fs::write(temporary, b"complete")?;
        Ok(())
    })
    .unwrap();
    assert!(write_output(&path, ConflictPolicy::Rename, |temporary| {
        fs::write(temporary, b"replacement")?;
        Ok(())
    })
    .is_err());
    assert_eq!(fs::read(path).unwrap(), b"complete");
    assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 1);
}

#[test]
fn overwrite_publishes_only_complete_outputs_and_preserves_the_previous_file_on_failure() {
    let destination = tempfile::tempdir().unwrap();
    let path = destination.path().join("image.dng");
    fs::write(&path, b"previous render").unwrap();
    assert!(write_output(&path, ConflictPolicy::Overwrite, |temporary| {
        fs::write(temporary, b"incomplete render")?;
        Err("render failed".into())
    })
    .is_err());
    assert_eq!(fs::read(&path).unwrap(), b"previous render");
    write_output(&path, ConflictPolicy::Overwrite, |temporary| {
        fs::write(temporary, b"finished render")?;
        assert_eq!(fs::read(&path)?, b"previous render");
        Ok(())
    })
    .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"finished render");
    assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 1);
}

#[test]
fn name_conflicts_include_rendered_outputs_and_edit_sidecars() {
    let destination = tempfile::tempdir().unwrap();
    let originals = destination.path().join("x3f");
    fs::create_dir(&originals).unwrap();
    fs::write(destination.path().join("image.tiff"), b"existing export").unwrap();
    crate::sidecar::save(
        &originals.join("image-1.x3f"),
        &crate::render::Settings::default(),
    )
    .unwrap();
    assert_eq!(
        available_name(
            Path::new("image.x3f"),
            &originals,
            destination.path(),
            &["tiff"],
            ConflictPolicy::Rename,
            &HashSet::new(),
        )
        .unwrap(),
        Path::new("image-2.x3f")
    );
    assert_eq!(
        available_name(
            Path::new("image.x3f"),
            &originals,
            destination.path(),
            &["tiff"],
            ConflictPolicy::Overwrite,
            &HashSet::new()
        )
        .unwrap(),
        Path::new("image.x3f")
    );
    let reserved = [destination.path().join("image.tiff")].into();
    assert_eq!(
        available_name(
            Path::new("image.x3f"),
            &originals,
            destination.path(),
            &["tiff"],
            ConflictPolicy::Overwrite,
            &reserved
        )
        .unwrap(),
        Path::new("image-1.x3f")
    );
}

#[test]
fn replacing_an_import_retires_the_previous_identitys_claim_to_that_path() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.x3f");
    let second = directory.path().join("second.x3f");
    fs::write(&first, b"first original").unwrap();
    fs::write(&second, b"second original").unwrap();
    let old_identity = Identity::read(&first).unwrap();
    let new_identity = Identity::read(&second).unwrap();
    let original = PathBuf::from("imported.x3f");
    fs::write(directory.path().join(&original), b"second original").unwrap();
    let mut history = History::default();
    for (identity, source) in [
        (old_identity.clone(), first),
        (new_identity.clone(), second),
    ] {
        history.set(Record {
            identity,
            source: source.display().to_string(),
            disposition: Disposition::Imported {
                original: original.clone(),
            },
        });
    }
    assert!(history
        .disposition(directory.path(), &old_identity)
        .is_none());
    assert!(matches!(
        history.disposition(directory.path(), &new_identity),
        Some(Disposition::Imported { .. })
    ));
}

#[test]
fn malformed_history_is_not_replaced_or_interpreted_as_empty() {
    let directory = tempfile::tempdir().unwrap();
    let path = History::path(directory.path());
    fs::create_dir(path.parent().unwrap()).unwrap();
    fs::write(&path, br#"{"version":99,"records":[]}"#).unwrap();
    assert!(History::load(directory.path()).is_err());
    assert_eq!(fs::read(&path).unwrap(), br#"{"version":99,"records":[]}"#);
}
