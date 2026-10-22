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
        processing_version: None,
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
        processing_version: None,
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
        let started = std::time::Instant::now();
        let source = Source::open(&path).unwrap();
        let opened_ms = started.elapsed().as_secs_f64() * 1000.;
        assert_eq!(
            read_capture_properties(&path).unwrap(),
            source.metadata.properties
        );
        let timings = source.load_timings();
        assert_eq!(timings[0].stage, "file read");
        assert_eq!(timings[1].stage, "X3F decode wall");
        assert!(timings[0].milliseconds + timings[1].milliseconds <= opened_ms);
        let planes = timings
            .iter()
            .find(|timing| timing.stage == "sensor planes wall")
            .unwrap();
        assert!(planes.milliseconds <= timings[1].milliseconds);
        eprintln!(
            "SOURCE-LOAD file_read_ms={:.3} decode_ms={:.3} source_open_ms={opened_ms:.3} cache_state=uncontrolled path={}",
            timings[0].milliseconds,
            timings[1].milliseconds,
            path.display()
        );
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

#[test]
fn classic_selection_stays_at_input_boundary() {
    for model in [
        "SIGMA SD9",
        "SIGMA SD10",
        "SIGMA SD14",
        "SIGMA SD15",
        "SIGMA DP1",
        "SIGMA DP1s",
        "SIGMA DP1x",
        "SIGMA DP2",
        "SIGMA DP2s",
        "SIGMA DP2x",
    ] {
        assert_eq!(family(Some(model)), Some(ProcessingFamily::Classic));
    }
    assert_eq!(
        family(Some("SIGMA DP2 Merrill")),
        Some(ProcessingFamily::Merrill)
    );
    assert_eq!(
        family(Some("SIGMA dp2 Quattro")),
        Some(ProcessingFamily::Quattro)
    );
    let mut metadata = Metadata {
        width: 2688,
        height: 1792,
        rotation: 0,
        processing_version: None,
        properties: [("CAMMODEL".into(), "SIGMA DP1S".into())].into(),
        matrices: Default::default(),
        lists: Default::default(),
    };
    assert_eq!(metadata.recorded_color_mode().unwrap(), "Standard");
    metadata
        .properties
        .insert("WB_DESC".into(), "Daylight".into());
    metadata.lists.insert(
        "DP1_WhiteBalanceGains".into(),
        PropertyList::new(vec![("Sunlight".into(), "DayGain".into())]).unwrap(),
    );
    assert_eq!(metadata.recorded_white_balance().unwrap(), "Sunlight");
    metadata
        .properties
        .insert("CM_DESC".into(), "Vivid ".into());
    metadata.lists.insert(
        "ColorModeCompensations".into(),
        PropertyList::new(vec![("Vivid ".into(), "VividMatrix".into())]).unwrap(),
    );
    assert_eq!(metadata.recorded_color_mode().unwrap(), "Vivid ");
}

#[test]
fn legacy_white_balance_catalogue_resolves_recorded_preset() {
    let metadata = Metadata {
        width: 2304,
        height: 1531,
        rotation: 0,
        processing_version: None,
        properties: [
            ("CAMMODEL".into(), "SIGMA SD9".into()),
            ("WB_DESC".into(), "Daylight".into()),
        ]
        .into(),
        matrices: Default::default(),
        lists: [(
            "WhiteBalanceCorrections".into(),
            PropertyList::new(vec![("Sunlight".into(), "CorrectionDaylight".into())]).unwrap(),
        )]
        .into(),
    };
    assert_eq!(metadata.white_balance_presets(), ["Sunlight"]);
    assert_eq!(metadata.recorded_white_balance().unwrap(), "Sunlight");
}

#[test]
fn classic_white_balance_choices_exclude_monochrome_rendering() {
    let mut metadata = Metadata {
        width: 1,
        height: 1,
        rotation: 0,
        processing_version: None,
        properties: [("CAMMODEL".into(), "SIGMA DP2".into())].into(),
        matrices: Default::default(),
        lists: [(
            "WhiteBalanceGains".into(),
            PropertyList::new(vec![
                ("Sunlight".into(), "sun".into()),
                ("Monochrome".into(), "mono".into()),
                ("Shade".into(), "shade".into()),
            ])
            .unwrap(),
        )]
        .into(),
    };
    assert_eq!(metadata.white_balance_presets(), ["Sunlight", "Shade"]);
    metadata
        .properties
        .insert("CAMMODEL".into(), "SIGMA DP2 Merrill".into());
    assert_eq!(
        metadata.white_balance_presets(),
        ["Sunlight", "Monochrome", "Shade"]
    );
}

#[test]
fn quattro_camera_labels_do_not_determine_processing_version() {
    for model in [
        "SIGMA dp0 Quattro",
        "SIGMA dp1 Quattro",
        "SIGMA dp2 Quattro",
        "SIGMA dp3 Quattro",
        "sd Quattro",
        "sd Quattro H",
    ] {
        assert_eq!(
            ProcessingFamily::from_camera_model(model),
            Some(ProcessingFamily::Quattro)
        );
    }
}

#[test]
#[ignore = "requires QUATTRO_FIXTURE_DIR pointing to original Quattro X3Fs"]
fn decode_quattro_files_preserves_storage_and_capture_contracts() {
    let directory = std::env::var_os("QUATTRO_FIXTURE_DIR").expect("set QUATTRO_FIXTURE_DIR");
    let mut checked = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("quattro")
            || path
                .extension()
                .is_none_or(|e| !e.eq_ignore_ascii_case("x3f"))
        {
            continue;
        }
        let source = Source::open(&path).unwrap();
        assert_eq!(
            source.metadata.processing_family().unwrap(),
            ProcessingFamily::Quattro
        );
        assert_eq!(source.storage(), SourceStorage::PackedRgbBluePhases);
        assert_eq!(
            source.storage_extent(),
            [source.metadata.width / 2, source.metadata.height / 2]
        );
        assert_eq!(
            source.samples().len(),
            source.metadata.width * source.metadata.height / 4 * 6
        );
        assert!(source
            .samples()
            .iter()
            .all(|v| v.is_finite() && (0.0..=16383.0).contains(v)));
        assert!(source.metadata.processing_version.is_some());
        assert_eq!(
            read_capture_properties(&path).unwrap(),
            source.metadata.properties
        );
        assert!(!source.metadata.properties["WB_DESC"].is_empty());
        assert!(!source.metadata.properties["CM_DESC"].is_empty());
        assert!(!source.metadata.matrices.is_empty());
        let preview = crate::preview::read(&path).unwrap();
        assert_eq!(
            preview.rgb.len(),
            preview.extent[0] as usize * preview.extent[1] as usize * 3
        );
        eprintln!(
            "{} {:?} {:?} {:?}",
            path.display(),
            source.storage_extent(),
            source.metadata.processing_version,
            source.metadata.properties
        );
        checked += 1;
    }
    assert!(checked > 0);
}
