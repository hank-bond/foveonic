//! Checks rating bounds, independent persistence, sibling merges and protected invalid review files.

use super::*;

#[test]
fn ratings_are_bounded_and_roundtrip_without_edit_recipes_or_original_writes() {
    let folder = tempfile::tempdir().unwrap();
    let first = folder.path().join("a.x3f");
    let second = folder.path().join("b.x3f");
    fs::write(&first, b"original").unwrap();
    save(&first, Rating::default()).unwrap();
    assert!(!path(folder.path()).exists());
    for stars in 0..=5 {
        let rating = Rating::try_from(stars).unwrap();
        assert_eq!(
            serde_json::from_str::<Rating>(&serde_json::to_string(&rating).unwrap()).unwrap(),
            rating
        );
    }
    assert!(serde_json::from_str::<Rating>("6").is_err());
    assert!(serde_json::from_str::<Rating>("-1").is_err());
    save(&first, Rating(3)).unwrap();
    save(&second, Rating(5)).unwrap();
    assert_eq!(load(folder.path()).unwrap().len(), 2);
    save_many(
        folder.path(),
        &BTreeMap::from([
            (PathBuf::from("a.x3f"), Rating(4)),
            (PathBuf::from("c.x3f"), Rating(2)),
        ]),
    )
    .unwrap();
    assert_eq!(
        load(folder.path()).unwrap().get(Path::new("b.x3f")),
        Some(&Rating(5))
    );
    save(&first, Rating(0)).unwrap();
    assert_eq!(
        load(folder.path()).unwrap(),
        BTreeMap::from([
            (PathBuf::from("b.x3f"), Rating(5)),
            (PathBuf::from("c.x3f"), Rating(2))
        ])
    );
    assert_eq!(fs::read(&first).unwrap(), b"original");
    assert!(!crate::sidecar::path(&first).exists());
}

#[test]
fn malformed_future_and_invalid_path_records_are_not_overwritten() {
    let folder = tempfile::tempdir().unwrap();
    fs::create_dir(folder.path().join("foveonic")).unwrap();
    for text in [
        "broken",
        r#"{"format":"foveonic-ratings","version":2,"ratings":{}}"#,
        r#"{"format":"foveonic-ratings","version":1,"ratings":{"../a.x3f":3}}"#,
        r#"{"format":"foveonic-ratings","version":1,"ratings":{"a.x3f":6}}"#,
    ] {
        fs::write(path(folder.path()), text).unwrap();
        assert!(save(&folder.path().join("a.x3f"), Rating(2)).is_err());
        assert_eq!(fs::read_to_string(path(folder.path())).unwrap(), text);
    }
}
