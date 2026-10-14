//! Checks exact editing recipes, named override linkage, bounded malformed inputs and atomic preservation.

use super::*;
use crate::{noise, render::RepairThreshold};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "foveonic-sidecar-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn input(&self) -> PathBuf {
        self.0.join("photo.X3F")
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn edited() -> Settings {
    let mut settings = Settings::new("Shade", "Vivid");
    settings.exposure = -0.0;
    settings.sensor_gain = 1.375;
    settings.saturation = 0.8;
    settings.tone.contrast = f64::from_bits(0x3fd5555555555555);
    settings.tone.shadows = -1.25;
    settings.tone.highlights = 0.75;
    settings.repair = Some(RepairThreshold::Manual(7));
    settings.despeckle = false;
    settings.row_correction = false;
    settings.highlight_restoration = false;
    settings.highlight_threshold_ev = Some(1.25);
    settings.sharpen_amount = 0.25;
    settings.output_size = Some([313, 470]);
    let mut controls = noise::Controls {
        chroma: 4,
        luminance: 0,
        banding: 4,
        detail: 0.42,
        ..Default::default()
    };
    controls
        .overrides
        .set(noise::Parameter::MeanDecay, Some(0.7));
    controls.overrides.set(noise::Parameter::LayerMix, Some(0.));
    settings.reconstruction = Some(controls);
    settings
}

#[test]
fn recipes_round_trip_all_settings_and_named_automatic_choices_exactly() {
    for original in [
        Settings::new("", ""),
        Settings::new("Auto", "Standard"),
        edited(),
    ] {
        let bytes = encode(&original).unwrap();
        let restored = decode(&bytes).unwrap();
        assert_eq!(original, restored);
        assert_eq!(original.exposure.to_bits(), restored.exposure.to_bits());
        assert_eq!(
            original.tone.contrast.to_bits(),
            restored.tone.contrast.to_bits()
        );
        assert_eq!(bytes, encode(&restored).unwrap());
    }
    let value: serde_json::Value = serde_json::from_slice(&encode(&edited()).unwrap()).unwrap();
    assert!(value["settings"]["color_dq_gain"].is_null());
    assert!(value["settings"]["reconstruction"]["overrides"]["base-blend"].is_null());
    assert_eq!(
        value["settings"]["reconstruction"]["overrides"]["layer-mix"],
        0.
    );
    assert!(value["settings"].get("viewport").is_none());
}

#[test]
fn missing_edit_fields_use_shared_defaults_and_preserve_supplied_values() {
    let defaults = Settings::default();
    assert_eq!(defaults, Settings::new("", ""));
    let minimal = br#"{"format":"foveonic-editing","version":1,"settings":{}}"#;
    assert_eq!(decode(minimal).unwrap(), defaults);
    let partial = br#"{"format":"foveonic-editing","version":1,"settings":{"exposure":1.25,"repair":null,"tone":{"shadows":-0.5},"reconstruction":{"chroma":4,"overrides":{"mean-decay":0.7}}}}"#;
    let mut expected = defaults;
    expected.exposure = 1.25;
    expected.repair = None;
    expected.tone.shadows = -0.5;
    let mut noise = noise::Controls {
        chroma: 4,
        ..Default::default()
    };
    noise.overrides.set(noise::Parameter::MeanDecay, Some(0.7));
    expected.reconstruction = Some(noise);
    assert_eq!(decode(partial).unwrap(), expected);
    assert_eq!(decode(&encode(&expected).unwrap()).unwrap(), expected);

    let original: serde_json::Value = serde_json::from_slice(&encode(&edited()).unwrap()).unwrap();
    let default_fields = serde_json::to_value(Settings::default()).unwrap();
    for (field, default_value) in default_fields.as_object().unwrap() {
        let mut missing = original.clone();
        missing["settings"].as_object_mut().unwrap().remove(field);
        let restored = decode(&serde_json::to_vec(&missing).unwrap()).unwrap();
        let mut expected = original["settings"].clone();
        expected[field] = default_value.clone();
        let actual: serde_json::Value =
            serde_json::from_slice(&encode(&restored).unwrap()).unwrap();
        assert_eq!(actual["settings"], expected, "{field}");
    }
    // Existing recipes keep their column midpoint when the new field is absent.
    let mut older = original.clone();
    older["settings"]["reconstruction"]
        .as_object_mut()
        .unwrap()
        .remove("banding");
    let restored = decode(&serde_json::to_vec(&older).unwrap()).unwrap();
    let mut expected = edited();
    expected.reconstruction.as_mut().unwrap().banding = 2;
    assert_eq!(restored, expected);
}

#[test]
fn malformed_unknown_version_fields_and_invalid_numbers_are_rejected() {
    assert!(decode(b"{").is_err());
    assert!(decode(&vec![b' '; MAX_BYTES as usize + 1]).is_err());
    for edit in [
        ("version", serde_json::json!(2)),
        ("format", serde_json::json!("another-app")),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_slice(&encode(&edited()).unwrap()).unwrap();
        value[edit.0] = edit.1;
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let mut value: serde_json::Value = serde_json::from_slice(&encode(&edited()).unwrap()).unwrap();
    value["settings"]["sensor_gain"] = serde_json::json!(-1.);
    assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    value["settings"]["sensor_gain"] = serde_json::json!(1.);
    value["settings"]["new_setting"] = serde_json::json!(1.);
    assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut invalid = edited();
    invalid.tone.highlights = f64::NAN;
    assert!(encode(&invalid).is_err());
    invalid = edited();
    invalid
        .reconstruction
        .as_mut()
        .unwrap()
        .overrides
        .set(noise::Parameter::MeanDecay, Some(2.));
    assert!(encode(&invalid).is_err());
    let bytes = String::from_utf8(encode(&edited()).unwrap()).unwrap();
    let duplicate = bytes.replace("\"version\": 1,", "\"version\": 1, \"version\": 1,");
    assert!(decode(duplicate.as_bytes()).is_err());
    let duplicate_setting = bytes.replace(
        "\"sensor_gain\": 1.375,",
        "\"sensor_gain\": 1.375, \"sensor_gain\": 1.0,",
    );
    assert!(decode(duplicate_setting.as_bytes()).is_err());
    let invalid_setting = bytes.replace("\"sensor_gain\": 1.375,", "\"sensor_gain\": null,");
    assert!(decode(invalid_setting.as_bytes()).is_err());
    let duplicate_override = bytes.replace(
        "\"base-blend\": null,",
        "\"base-blend\": null, \"base-blend\": 0.5,",
    );
    assert!(decode(duplicate_override.as_bytes()).is_err());
    let missing_override = bytes.replace("\"base-blend\": null,", "");
    assert_eq!(decode(missing_override.as_bytes()).unwrap(), edited());
    for (field, invalid) in [
        ("banding", serde_json::json!(5)),
        ("banding", serde_json::json!(1.5)),
        ("banding", serde_json::Value::Null),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_slice(&encode(&edited()).unwrap()).unwrap();
        value["settings"]["reconstruction"][field] = invalid;
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let mut value: serde_json::Value = serde_json::from_slice(&encode(&edited()).unwrap()).unwrap();
    value["settings"]["last_folder"] = serde_json::json!("/some/folder");
    assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    value["settings"]
        .as_object_mut()
        .unwrap()
        .remove("last_folder");
    value["settings"]["tone"]["last_folder"] = serde_json::json!("/some/folder");
    assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    value["settings"]["tone"]
        .as_object_mut()
        .unwrap()
        .remove("last_folder");
    value["settings"]["reconstruction"]["overrides"]["unknown"] = serde_json::json!(0.5);
    assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut oversized = edited();
    oversized.color_mode = "a".repeat(MAX_BYTES as usize);
    assert!(encode(&oversized).is_err());
}

#[test]
fn recovery_threshold_keeps_auto_manual_and_disabled_recipes() {
    for enabled in [false, true] {
        for ev in [None, Some(-2.), Some(-0.), Some(0.), Some(1.25), Some(2.)] {
            let original = Settings {
                highlight_restoration: enabled,
                highlight_threshold_ev: ev,
                ..Default::default()
            };
            let restored = decode(&encode(&original).unwrap()).unwrap();
            assert_eq!(restored, original);
            assert_eq!(
                restored.highlight_threshold_ev.map(f32::to_bits),
                ev.map(f32::to_bits)
            );
        }
    }
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -2.01, 2.01] {
        let original = Settings {
            highlight_threshold_ev: Some(invalid),
            ..Default::default()
        };
        assert!(encode(&original).is_err());
    }
    for invalid in [
        serde_json::json!(-2.01),
        serde_json::json!(2.01),
        serde_json::json!("auto"),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_slice(&encode(&edited()).unwrap()).unwrap();
        value["settings"]["highlight_threshold_ev"] = invalid;
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}

#[test]
fn atomic_save_preserves_x3f_and_rejects_corrupt_existing_recipes() {
    let directory = Directory::new();
    let input = directory.input();
    fs::write(&input, b"immutable source").unwrap();
    assert!(load(&input).unwrap().is_none());
    assert_eq!(path(&input).file_name().unwrap(), "photo.X3F.foveonic.json");
    save(&input, &edited()).unwrap();
    assert_eq!(load(&input).unwrap(), Some(edited()));
    save(&input, &Settings::new("Auto", "Standard")).unwrap();
    assert_eq!(
        load(&input).unwrap(),
        Some(Settings::new("Auto", "Standard"))
    );
    assert_eq!(fs::read(&input).unwrap(), b"immutable source");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
    fs::write(path(&input), b"invalid recipe").unwrap();
    assert!(save(&input, &edited()).is_err());
    assert_eq!(fs::read(path(&input)).unwrap(), b"invalid recipe");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn missing_parent_or_unreadable_destination_cannot_create_a_recipe() {
    let directory = Directory::new();
    assert!(save(&directory.0.join("missing/photo.x3f"), &edited()).is_err());
    fs::create_dir(path(&directory.input())).unwrap();
    assert!(load(&directory.input()).is_err());
    assert!(save(&directory.input(), &edited()).is_err());
}

#[cfg(unix)]
#[test]
fn sidecar_paths_preserve_non_utf8_filenames() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let input = PathBuf::from(std::ffi::OsString::from_vec(b"photo-\xff.X3F".to_vec()));
    assert_eq!(
        path(&input).as_os_str().as_bytes(),
        b"photo-\xff.X3F.foveonic.json"
    );
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn restored_recipe_replays_exact_photographic_pixels() {
    use crate::{
        profile::Profile,
        render::{Engine, RenderSession},
        source::Source,
    };
    let input = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let source = Source::open(input).unwrap();
    let engine = Engine::new().unwrap();
    let mut settings = edited();
    settings.white_balance = "Sunlight".into();
    settings.color_mode = "Standard".into();
    settings.output_size = Some([96, 144]);
    settings.highlight_restoration = true;
    let restored = decode(&encode(&settings).unwrap()).unwrap();
    let original = engine
        .render(&source, &settings, &mut Profile::default())
        .unwrap();
    let mut session = RenderSession::new(source);
    let mut artifacts = engine
        .render_session(
            &mut session,
            &restored,
            &["final"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    let crate::inspection::Content::Image(replayed) = artifacts.pop().unwrap().content else {
        panic!("image")
    };
    assert!(
        engine.readback_image(&original).unwrap() == engine.readback_image(&replayed).unwrap(),
        "recipe replay changes photographic samples"
    );
}
