//! Checks capture identity, atomic profile persistence, matrix fitting and GPU cap statistics.

use super::*;
pub(super) fn condition() -> Condition {
    Condition {
        camera_model: "SIGMA DP3 Merrill".into(),
        serial: Some("camera-one".into()),
        iso: 100,
        shutter_micros: 15630,
    }
}
fn profile() -> Calibration {
    let c = condition();
    Calibration {
        version: FORMAT_VERSION,
        signature: Default::default(),
        captures: (0..7)
            .map(|i| Capture {
                path: PathBuf::from(format!("cap-{i}.x3f")),
                condition: c.clone(),
            })
            .collect(),
        model: noise_model::embedded().unwrap().clone(),
        defects: vec![SensorDefects {
            camera_model: c.camera_model,
            serial: c.serial.unwrap(),
            extent: [100, 80],
            points: vec![Defect {
                pixel: 2020,
                channels: 1,
            }],
        }],
    }
}
fn metadata(serial: Option<&str>) -> source::Metadata {
    let mut properties = BTreeMap::from([
        ("CAMMODEL".into(), "SIGMA DP3 Merrill".into()),
        ("ISO".into(), "100".into()),
        ("SHUTTER".into(), "0.01563".into()),
    ]);
    if let Some(serial) = serial {
        properties.insert("CAMSERIAL".into(), serial.into());
    }
    source::Metadata {
        width: 100,
        height: 80,
        rotation: 0,
        processing_version: None,
        properties,
        matrices: Default::default(),
        lists: Default::default(),
    }
}
#[test]
fn identity_and_grouping_keep_camera_bodies_separate() {
    let p = profile();
    let mut other = p.captures[0].clone();
    other.path = "other.x3f".into();
    other.condition.serial = Some("camera-two".into());
    let mut captures = p.captures.clone();
    captures.push(captures[0].clone());
    captures.push(other);
    assert_eq!(groups(&captures).len(), 2);
    assert_eq!(groups(&captures)[&condition()].len(), 7);
    assert!(p.matching_defects(&metadata(Some("camera-one"))).is_some());
    for serial in [None, Some("camera-two"), Some("")] {
        assert!(p.matching_defects(&metadata(serial)).is_none());
    }
    let mut m = metadata(Some("camera-one"));
    m.properties.insert("ISO".into(), "200".into());
    assert!(p.matching_defects(&m).is_some());
    m.properties.insert("ISO".into(), "100".into());
    m.properties.insert("SHUTTER".into(), "0.1".into());
    assert!(p.matching_defects(&m).is_some());
}
#[test]
fn saved_profile_survives_without_caps_and_failed_replacement_preserves_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calibration.json");
    let p = profile();
    p.save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let restored = Calibration::load(&path).unwrap();
    assert_eq!(restored.captures, p.captures);
    assert_eq!(restored.signature(), p.signature());
    assert_eq!(restored.model.shield_tables, p.model.shield_tables);
    assert!(restored
        .matching_defects(&metadata(Some("camera-one")))
        .is_some());
    let mut bad = p.clone();
    bad.defects[0].serial.clear();
    assert!(bad.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut bad = p;
    bad.captures.truncate(6);
    assert!(bad.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn calibration_shader_validates_without_a_device() {
    let module = wgpu::naga::front::wgsl::parse_str(include_str!("training.wgsl")).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn merges_locations_across_conditions_without_crossing_camera_identity() {
    let map = profile().defects.remove(0);
    let mut additional = map.clone();
    additional.points[0].channels = 4;
    additional.points.push(Defect {
        pixel: 3000,
        channels: 2,
    });
    let mut other = map.clone();
    other.serial = "camera-two".into();
    let merged = merge_defects(vec![map, additional, other]);
    assert_eq!(merged.len(), 2);
    assert_eq!(
        merged[0]
            .points
            .iter()
            .map(|v| (v.pixel, v.channels))
            .collect::<Vec<_>>(),
        vec![(2020, 5), (3000, 2)]
    );
    assert_eq!(merged[1].points[0].channels, 1);
    let p = profile();
    let mut m = metadata(Some("camera-one"));
    m.properties.remove("ISO");
    m.properties.remove("SHUTTER");
    assert!(p.matching_defects(&m).is_some());
    m.width += 1;
    assert!(p.matching_defects(&m).is_none());
    for invalid in ["", "0", "0000", "unknown", "N/A"] {
        assert!(
            Condition::from_properties(&metadata(Some(invalid)).properties)
                .unwrap()
                .serial
                .is_none()
        );
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn learned_repairs_follow_serial_switches_bypasses_and_cached_rendering() {
    use crate::{
        inspection::Content,
        nodes::sensor::calibration,
        profile::Profile,
        render::{Engine, RenderSession, Settings},
    };
    use std::sync::Arc;
    let path = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let mut source = source::Source::open(path).unwrap();
    let area = calibration::rectangle(&source.metadata, "ActiveImageArea").unwrap();
    let x = area[0] + 50;
    let y = area[1] + 50;
    let width = source.metadata.width;
    let pixel = y as usize * width + x as usize;
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            let point = (y as i32 + dy) as usize * width + (x as i32 + dx) as usize;
            source.pixels[point * 3..point * 3 + 3].copy_from_slice(&[100.; 3]);
        }
    }
    source.pixels[pixel * 3] = 4095.;
    Arc::make_mut(&mut source.metadata)
        .properties
        .insert("CAMSERIAL".into(), "fixture-body".into());
    let identity = Condition::from_properties(&source.metadata.properties).unwrap();
    let mut trained = profile();
    for cap in &mut trained.captures {
        cap.condition = identity.clone();
    }
    trained.defects = vec![SensorDefects {
        camera_model: identity.camera_model.clone(),
        serial: identity.serial.clone().unwrap(),
        extent: [source.metadata.width as u32, source.metadata.height as u32],
        points: vec![Defect {
            pixel: pixel as u32,
            channels: 1,
        }],
    }];
    trained.validate().unwrap();
    let trained = Arc::new(trained);
    let source = Arc::new(source);
    let mut engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source.clone());
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.repair = None;
    let render = |engine: &Engine, session: &mut RenderSession, settings: &Settings| {
        let mut artifacts = engine
            .render_session(
                session,
                settings,
                &["calibration.layers"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        let Content::Image(image) = artifacts.remove(0).content else {
            panic!("native calibration image")
        };
        engine.readback_image(&image).unwrap()
    };
    let baseline = render(&engine, &mut session, &settings);
    engine.set_noise_calibration(Some(trained.clone()));
    let fixed = render(&engine, &mut session, &settings);
    assert!(
        fixed != baseline,
        "calibration must repair the injected outlier"
    );
    assert!(fixed
        .as_chunks::<4>()
        .0
        .iter()
        .zip(baseline.as_chunks::<4>().0)
        .enumerate()
        .all(|(i, (a, b))| i == pixel * 3 || a == b));
    engine.set_noise_calibration(None);
    assert!(render(&engine, &mut session, &settings) == baseline);
    engine.set_noise_calibration(Some(trained));
    settings.despeckle = false;
    assert!(render(&engine, &mut session, &settings) == baseline);
    settings.despeckle = true;
    settings.processing_sections.noise = false;
    assert!(render(&engine, &mut session, &settings) == baseline);
    settings.processing_sections.noise = true;
    let mut other = source::Source::open(
        PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f"),
    )
    .unwrap();
    other.pixels.clone_from(&source.pixels);
    for serial in [Some("different-body"), None] {
        let properties = &mut Arc::make_mut(&mut other.metadata).properties;
        if let Some(serial) = serial {
            properties.insert("CAMSERIAL".into(), serial.into());
        } else {
            properties.remove("CAMSERIAL");
        }
        let mut artifacts = engine
            .render_artifacts(
                &other,
                &settings,
                &["calibration.layers"],
                &mut Profile::default(),
            )
            .unwrap();
        let Content::Image(image) = artifacts.remove(0).content else {
            panic!("native calibration image")
        };
        assert!(engine.readback_image(&image).unwrap() == baseline);
    }
}
