//! Checks source camera selection, metadata identity, and optional original-file decoding.

use super::*;

#[test]
fn accepts_merrill_models() {
    for model in [
        "SIGMA DP1 Merrill",
        "SIGMA DP2 Merrill",
        "SIGMA DP3 Merrill",
        "SIGMA SD1 Merrill",
    ] {
        assert!(is_merrill(Some(model)));
    }
    assert!(!is_merrill(Some("SIGMA sd Quattro")));
    assert!(!is_merrill(None));
}

#[test]
fn metadata_preserves_matrix_encoding_and_property_order() {
    for code in [0, 1, 2, 3, 5, 6] {
        let encoding = MatrixEncoding::try_from(code).unwrap();
        assert_eq!(serde_json::to_string(&encoding).unwrap(), code.to_string());
    }
    assert!(MatrixEncoding::try_from(4).is_err());
    let list = PropertyList::new(vec![
        ("Vivid".into(), "v".into()),
        ("Standard".into(), "s".into()),
    ])
    .unwrap();
    assert_eq!(list.index("Standard"), Some(1));
    assert_eq!(list.get("Vivid").map(String::as_str), Some("v"));
    assert_eq!(
        serde_json::to_string(&list).unwrap(),
        r#"[["Vivid","v"],["Standard","s"]]"#
    );
    assert!(PropertyList::new(vec![("A".into(), "1".into()), ("A".into(), "2".into())]).is_err());
}

#[test]
fn recorded_camera_choices_resolve_aliases_and_reject_unknown_presets() {
    let mut metadata = Metadata {
        width: 1,
        height: 1,
        rotation: 0,
        properties: [
            ("WB_DESC".into(), " Daylight ".into()),
            ("CM_DESC".into(), "vivid".into()),
        ]
        .into(),
        matrices: BTreeMap::new(),
        lists: [
            (
                "WhiteBalanceGains".into(),
                PropertyList::new(vec![
                    ("Sunlight".into(), "s".into()),
                    ("Auto".into(), "a".into()),
                    ("Custom".into(), "c".into()),
                ])
                .unwrap(),
            ),
            (
                "ColorModeCompensations".into(),
                PropertyList::new(vec![
                    ("Standard".into(), "s".into()),
                    ("Vivid".into(), "v".into()),
                ])
                .unwrap(),
            ),
        ]
        .into(),
    };
    assert_eq!(metadata.recorded_white_balance().unwrap(), "Sunlight");
    assert_eq!(metadata.recorded_color_mode().unwrap(), "Vivid");
    for name in ["Auto", "Custom"] {
        metadata.properties.insert("WB_DESC".into(), name.into());
        assert_eq!(metadata.recorded_white_balance().unwrap(), name);
    }
    metadata
        .properties
        .insert("WB_DESC".into(), "Unknown".into());
    assert!(metadata.recorded_white_balance().is_err());
    metadata.properties.remove("CM_DESC");
    assert!(metadata.recorded_color_mode().is_err());
}

#[test]
fn matrix_lookup_preserves_names_and_ignores_ascii_case() {
    let metadata = Metadata {
        width: 0,
        height: 0,
        rotation: 0,
        properties: BTreeMap::new(),
        matrices: [(
            "SensorISO".into(),
            Matrix {
                encoding: MatrixEncoding::Float32,
                representation: "f64",
                dimensions: vec![1],
                values: vec![100.],
            },
        )]
        .into(),
        lists: BTreeMap::new(),
    };
    assert_eq!(metadata.scalar("sensoriso").unwrap(), 100.);
    assert!(metadata.matrices.contains_key("SensorISO"));
    assert!(metadata.fixed::<3>("SensorISO").is_err());
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR pointing to original Merrill X3Fs"]
fn decode_original_merrill_files() {
    let directory = std::env::var_os("FOVEONIC_FIXTURE_DIR").expect("set FOVEONIC_FIXTURE_DIR");
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("x3f"))
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "fixture directory contains no X3Fs");
    for path in files {
        let source = Source::open(&path).unwrap();
        assert_eq!(source.pixels.len(), SENSOR_WIDTH * SENSOR_HEIGHT * 3);
        assert!(source
            .pixels
            .iter()
            .all(|value| value.is_finite() && (0.0..=4095.0).contains(value)));
        assert!(!source.camf.is_empty());
        assert!(!source.metadata.matrices.is_empty());
        assert!(!source.metadata.lists.is_empty());
        assert_eq!(
            source.metadata.fixed::<4>("ActiveImageArea").unwrap().len(),
            4
        );
        println!(
            "decoded {}: {} samples, {} matrices, {} lists",
            path.display(),
            source.pixels.len(),
            source.metadata.matrices.len(),
            source.metadata.lists.len()
        );
    }
}
