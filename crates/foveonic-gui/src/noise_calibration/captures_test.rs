//! Checks offline readouts, dataset replacement and source-availability generation boundaries.

use super::*;

fn capture(path: PathBuf) -> (Capture, Properties) {
    let properties = BTreeMap::from([
        ("CAMMODEL".into(), "SIGMA DP3 Merrill".into()),
        ("CAMSERIAL".into(), "camera-one".into()),
        ("ISO".into(), "100".into()),
        ("SHUTTER".into(), "0.01563".into()),
        ("SH_DESC".into(), "1/60".into()),
    ]);
    (
        Capture::from_properties(path, &properties).unwrap(),
        properties,
    )
}
#[test]
fn offline_dataset_retains_shared_readouts_and_replaces_changed_identity() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("cap.X3F");
    std::fs::write(&source, []).unwrap();
    let (capture, properties) = capture(source.clone());
    let mut dataset = Dataset::default();
    assert!(dataset.insert(capture.clone(), properties.clone()));
    assert!(!dataset.insert(capture.clone(), properties.clone()));
    dataset.save(dir.path()).unwrap();
    std::fs::remove_file(&source).unwrap();
    let (restored, error) = Dataset::load(Some(dir.path()), &[]);
    assert!(error.is_none());
    assert_eq!(restored.captures.as_slice(), std::slice::from_ref(&capture));
    let rows = crate::metadata::property_rows(&restored.properties[&capture.path]);
    assert!(rows.contains(&("Shutter", "1/60".into())));
    assert!(restored.missing_metadata().is_empty());
    let mut changed = capture.clone();
    changed.condition.iso = 200;
    assert!(dataset.insert(changed.clone(), properties));
    assert_eq!(dataset.captures, [changed.clone()]);
    dataset.remove(&changed.condition);
    assert!(dataset.captures.is_empty());
    assert!(dataset.properties.is_empty());
}
#[test]
fn invalid_readout_cache_does_not_discard_pending_captures() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("cap.X3F");
    std::fs::write(&source, []).unwrap();
    let (capture, properties) = capture(source);
    let mut dataset = Dataset::default();
    dataset.insert(capture.clone(), properties);
    dataset.save(dir.path()).unwrap();
    std::fs::write(dir.path().join(PROPERTIES_FILE), b"invalid JSON").unwrap();
    let (restored, error) = Dataset::load(Some(dir.path()), &[]);
    assert!(error.unwrap().contains(PROPERTIES_FILE));
    assert_eq!(restored.captures.as_slice(), std::slice::from_ref(&capture));
    assert_eq!(restored.missing_metadata(), [capture.path]);
}
#[test]
fn stale_availability_cannot_enable_training_for_a_changed_dataset() {
    let (tx, rx) = mpsc::channel();
    let mut status = Availability {
        receiver: Some(rx),
        ..Default::default()
    };
    tx.send((0, BTreeSet::new())).unwrap();
    status.invalidate();
    // Refresh first consumes the old result, then queues a check for this generation.
    let cache = Arc::new(foveonic::decode_cache::Cache::default());
    status.refresh(&[], &cache, &egui::Context::default());
    assert!(!status.ready());
    assert!(status.missing.is_empty());
}
