//! Checks neighborhood retention, prospective insertion, and the shared hard disk budget.

use super::*;
use std::time::{Duration, UNIX_EPOCH};

fn source(folder: &Path, name: &str) -> PathBuf {
    let path = folder.join(name);
    fs::write(&path, b"original identity").unwrap();
    path
}
fn decoded(root: &Path, source: &Path, bytes: usize, age: u64) -> PathBuf {
    let path = root.join(Identity::read(source).unwrap().name());
    fs::write(&path, vec![0; bytes]).unwrap();
    fs::File::open(&path)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(age))
        .unwrap();
    path
}

#[test]
fn old_nearby_data_survives_newer_unrelated_entries() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("cache");
    fs::create_dir(&root).unwrap();
    let selected = source(folder.path(), "selected.x3f");
    let neighbor = source(folder.path(), "neighbor.x3f");
    let unrelated = source(folder.path(), "unrelated.x3f");
    let selected_entry = decoded(&root, &selected, 6, 1);
    let unrelated_entry = decoded(&root, &unrelated, 6, 100);
    let incoming = root.join(Identity::read(&neighbor).unwrap().name());
    let nearby = [selected, neighbor];
    assert!(reserve(&root, &incoming, 6, 12, &nearby).unwrap());
    assert!(selected_entry.exists());
    assert!(!unrelated_entry.exists());
    fs::write(&incoming, [0; 6]).unwrap();
    let retained = prune(&root, 12, &nearby).unwrap();
    assert_eq!(retained.len(), 2);
    assert_eq!(retained.iter().map(|e| e.1).sum::<u64>(), 12);
}

#[test]
fn rejected_far_insertion_does_not_evict_any_existing_data() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("cache");
    fs::create_dir(&root).unwrap();
    let selected = source(folder.path(), "selected.x3f");
    let neighbor = source(folder.path(), "neighbor.x3f");
    let far = source(folder.path(), "far.x3f");
    let unrelated = source(folder.path(), "unrelated.x3f");
    let selected_entry = decoded(&root, &selected, 6, 1);
    let neighbor_entry = decoded(&root, &neighbor, 6, 2);
    let unrelated_entry = decoded(&root, &unrelated, 2, 3);
    let incoming = root.join(Identity::read(&far).unwrap().name());
    assert!(!reserve(&root, &incoming, 6, 14, &[selected, neighbor, far]).unwrap());
    assert!(selected_entry.exists());
    assert!(neighbor_entry.exists());
    assert!(unrelated_entry.exists());
    assert!(!incoming.exists());
}

#[test]
fn shared_budget_keeps_decoded_raw_before_its_preview_and_counts_replacements_once() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("cache");
    fs::create_dir(&root).unwrap();
    let selected = source(folder.path(), "selected.x3f");
    let raw = decoded(&root, &selected, 100, 1);
    let preview = previews::location(&root, &selected).unwrap();
    assert!(!reserve(&root, &preview, 10, 100, std::slice::from_ref(&selected)).unwrap());
    assert!(raw.exists());
    fs::remove_file(&raw).unwrap();
    fs::write(&preview, [0; 10]).unwrap();
    assert!(reserve(&root, &raw, 100, 100, std::slice::from_ref(&selected)).unwrap());
    assert!(!preview.exists());
    fs::write(&raw, [0; 100]).unwrap();
    assert!(reserve(&root, &raw, 100, 100, &[selected]).unwrap());
    assert!(raw.exists());
}

#[test]
fn smaller_budget_evicts_farther_neighbors_and_selection_changes_release_old_preferences() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("cache");
    fs::create_dir(&root).unwrap();
    let selected = source(folder.path(), "selected.x3f");
    let neighbor = source(folder.path(), "neighbor.x3f");
    let far = source(folder.path(), "far.x3f");
    let selected_entry = decoded(&root, &selected, 6, 1);
    let neighbor_entry = decoded(&root, &neighbor, 6, 2);
    let far_entry = decoded(&root, &far, 6, 3);
    let retained = prune(&root, 12, &[selected, neighbor.clone(), far]).unwrap();
    assert_eq!(retained.len(), 2);
    assert!(selected_entry.exists());
    assert!(neighbor_entry.exists());
    assert!(!far_entry.exists());
    prune(&root, 6, &[neighbor]).unwrap();
    assert!(!selected_entry.exists());
    assert!(neighbor_entry.exists());
}
