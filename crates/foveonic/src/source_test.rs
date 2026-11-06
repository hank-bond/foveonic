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
fn classic_reduced_modes_require_slr_identity_and_declared_photograph_geometry() {
    let mut metadata = Metadata {
        width: 1344,
        height: 896,
        rotation: 0,
        processing_version: None,
        properties: Default::default(),
        matrices: Default::default(),
        lists: Default::default(),
    };
    for model in ["SIGMA SD9", "SIGMA SD10", "SIGMA SD14", "SIGMA SD15"] {
        metadata.properties.insert("CAMMODEL".into(), model.into());
        let early = matches!(model, "SIGMA SD9" | "SIGMA SD10");
        metadata.width = if early { 1152 } else { 1344 };
        metadata.height = if early { 766 } else { 896 };
        for (resolution, photo) in [
            ("LOW", if early { [1134, 756] } else { [1296, 864] }),
            ("MED", if early { [1512, 1008] } else { [1776, 1184] }),
        ] {
            metadata
                .properties
                .insert("RESOLUTION".into(), resolution.into());
            metadata
                .properties
                .insert("X3F_IMAGE_WIDTH".into(), photo[0].to_string());
            metadata
                .properties
                .insert("X3F_IMAGE_HEIGHT".into(), photo[1].to_string());
            assert!(classic_extent_supported(&metadata));
            metadata
                .properties
                .insert("X3F_IMAGE_WIDTH".into(), "1".into());
            assert!(!classic_extent_supported(&metadata));
        }
    }
    metadata
        .properties
        .insert("CAMMODEL".into(), "SIGMA DP1".into());
    assert!(!classic_extent_supported(&metadata));
    metadata
        .properties
        .insert("CAMMODEL".into(), "SIGMA SD14".into());
    metadata
        .properties
        .insert("X3F_IMAGE_WIDTH".into(), "1776".into());
    metadata.width = 3000;
    assert!(!classic_extent_supported(&metadata));
    metadata.width = 0;
    assert!(!classic_extent_supported(&metadata));
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

#[test]
#[ignore = "requires GPU and FOVEONIC_MERRILL_FIXTURE_DIR with HI, MED and LOW originals"]
fn original_merrill_resolutions_render_color_and_monochrome() {
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_MERRILL_FIXTURE_DIR").unwrap());
    let engine = crate::render::Engine::new().unwrap();
    for (file, model, resolution, extent) in [
        (
            "sd1-merrill-full.x3f",
            "SIGMA SD1 Merrill",
            "HI",
            [4928, 3264],
        ),
        (
            "sd1-merrill-low.x3f",
            "SIGMA SD1 Merrill",
            "LOW",
            [2464, 1632],
        ),
        (
            "dp3-merrill-low.x3f",
            "SIGMA DP3 Merrill",
            "LOW",
            [2464, 1632],
        ),
        (
            "dp3-merrill-medium.x3f",
            "SIGMA DP3 Merrill",
            "MED",
            [4928, 1632],
        ),
    ] {
        let source = std::sync::Arc::new(Source::open(root.join(file)).unwrap());
        assert_eq!(source.metadata.properties["CAMMODEL"], model);
        assert_eq!(source.metadata.properties["RESOLUTION"], resolution);
        assert_eq!(source.storage_extent(), extent);
        assert_eq!(source.samples().len(), extent[0] * extent[1] * 3);
        assert!(source.samples().iter().all(|v| (0. ..=4095.).contains(v)));
        assert!(!source.camf.is_empty());
        let presentation = crate::render::native_extent(&source.metadata).unwrap();
        let mut session = crate::render::RenderSession::new(source.clone());
        for (rendering, method) in [
            (
                crate::render::Rendering::Color,
                crate::render::MerrillNoiseMethod::Reference,
            ),
            (
                crate::render::Rendering::Color,
                crate::render::MerrillNoiseMethod::MultiscaleRows,
            ),
            (
                crate::render::Rendering::Color,
                crate::render::MerrillNoiseMethod::MultiscaleRowsAndColumns,
            ),
            (
                crate::render::Rendering::Monochrome,
                crate::render::MerrillNoiseMethod::Reference,
            ),
        ] {
            for quality in [
                crate::render::PreviewQuality::Full,
                crate::render::PreviewQuality::Fast,
            ] {
                let mut settings = crate::render::Settings::new("Auto", "Standard");
                settings.preview_quality = quality;
                settings.rendering = rendering;
                settings.merrill_noise_method = method;
                settings.reconstruction = Some(crate::noise::Controls::default());
                let result = engine.render_session(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut crate::profile::Profile::default(),
                );
                if extent[0] == 2464
                    && method == crate::render::MerrillNoiseMethod::MultiscaleRowsAndColumns
                {
                    assert!(result
                        .err()
                        .unwrap()
                        .to_string()
                        .contains("requires a full-resolution"));
                    continue;
                }
                let artifacts = result.unwrap().unwrap();
                let crate::inspection::Content::Image(image) = &artifacts[0].content else {
                    panic!("expected photograph");
                };
                let scale = if quality == crate::render::PreviewQuality::Full
                    || rendering == crate::render::Rendering::Monochrome
                {
                    1
                } else {
                    2
                };
                assert_eq!(image.view().extent, presentation.map(|v| v / scale));
                let bytes = engine.readback_image(image).unwrap();
                if extent == [4928, 1632]
                    && quality == crate::render::PreviewQuality::Full
                    && rendering == crate::render::Rendering::Color
                    && method == crate::render::MerrillNoiseMethod::Reference
                {
                    for composed in [false, true] {
                        let mut detail = settings.clone();
                        if composed {
                            detail.crop.bounds = [0.1, 0.2, 0.9, 0.8];
                            detail.rotation.degrees = 17.;
                            detail.output_size = Some([700, 1050]);
                        }
                        let baseline = if composed {
                            engine
                                .render(&source, &detail, &mut Default::default())
                                .unwrap()
                        } else {
                            image.as_ref().clone()
                        };
                        detail.preview_region = Some([32, 32, 160, 160]);
                        assert!(!detail.regional_preview_available(&source.metadata));
                        let whole = engine
                            .render(&source, &detail, &mut Default::default())
                            .unwrap();
                        assert_eq!(whole.view(), baseline.view());
                        assert_eq!(whole.params().mapping, baseline.params().mapping);
                        assert_eq!(whole.settings().preview_region, None);
                        assert_eq!(
                            engine.readback_image(&whole).unwrap(),
                            engine.readback_image(&baseline).unwrap()
                        );
                    }
                }

                assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
                    .iter()
                    .all(|v| v.is_finite()));
            }
        }
    }
}
