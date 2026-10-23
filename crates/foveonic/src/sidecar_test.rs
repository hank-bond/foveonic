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
        fs::create_dir(path.join("foveonic")).unwrap();
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
    settings.saturation = 0.8;
    settings.tone.contrast = f64::from_bits(0x3fd5555555555555);
    settings.tone.shadows = -1.25;
    settings.tone.highlights = 0.75;
    settings.repair = Some(RepairThreshold::Manual(7));
    settings.despeckle = false;
    settings.row_correction = false;
    settings.highlight_restoration = false;
    settings.highlight_threshold_ev = Some(1.25);
    settings.sharpen_amount = Some(0.25);
    settings.tiff_color_space = Some(crate::render::TiffColorSpace::ProPhotoRgb);
    settings.highlight_mode = crate::render::HighlightMode::ColorField;
    settings.color_field.selectivity = 0.5;
    settings.color_field.approach = crate::render::ColorFieldApproach::LocalGradient;
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
    assert!(value["settings"].get("sensor_gain").is_none());
    let mut retired = value;
    retired["settings"]["sensor_gain"] = serde_json::json!(1.);
    assert!(decode(&serde_json::to_vec(&retired).unwrap()).is_err());
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
    // Compare the on-disk representation on both sides. Direct to_value widens f32 fractions.
    let default_fields: serde_json::Value =
        serde_json::from_slice(&encode(&Settings::default()).unwrap()).unwrap();
    let default_fields = &default_fields["settings"];
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
fn older_color_field_edits_migrate_to_zero_without_rewriting_sidecars() {
    let mut older: serde_json::Value = serde_json::from_slice(&encode(&edited()).unwrap()).unwrap();
    older["settings"]["color_field"]
        .as_object_mut()
        .unwrap()
        .remove("selectivity");
    let mut expected = edited();
    expected.color_field.selectivity = 0.;
    assert_eq!(
        decode(&serde_json::to_vec(&older).unwrap()).unwrap(),
        expected
    );
    let directory = Directory::new();
    older["settings"]["color_field"]["donation"] = serde_json::json!(0.5);
    older["settings"]["color_field"]["agreement"] = serde_json::json!(0.75);
    older["settings"]["color_field"]["boundary_ev"] = serde_json::json!(0.4);
    older["settings"]["color_field"]["area"] = serde_json::json!("active");
    for amount in [0., 0.75, 8.] {
        older["settings"]["color_field"]["restraint"] = serde_json::json!(amount);
        let bytes = serde_json::to_vec(&older).unwrap();
        fs::write(path(&directory.input()), &bytes).unwrap();
        let migrated = load(&directory.input()).unwrap().unwrap();
        assert_eq!(migrated, expected);
        assert_eq!(fs::read(path(&directory.input())).unwrap(), bytes);
        let encoded: serde_json::Value =
            serde_json::from_slice(&encode(&migrated).unwrap()).unwrap();
        assert_eq!(
            encoded["settings"]["color_field"]["selectivity"],
            serde_json::json!(0.)
        );
        assert!(encoded["settings"]["color_field"].get("cutoff").is_none());
        assert!(encoded["settings"]["color_field"]
            .get("restraint")
            .is_none());
    }
    older["settings"]["color_field"]["cutoff"] = serde_json::json!(0.01);
    assert_eq!(
        decode(&serde_json::to_vec(&older).unwrap()).unwrap(),
        edited()
    );
    for (cutoff, strength) in [(0.005, 1.), (0.01, 0.5), (0.0125, 0.4), (0.02, 0.25)] {
        older["settings"]["color_field"]["cutoff"] = serde_json::json!(cutoff);
        let migrated = decode(&serde_json::to_vec(&older).unwrap()).unwrap();
        assert!((migrated.color_field.selectivity - strength).abs() < 1e-7);
        let encoded: serde_json::Value =
            serde_json::from_slice(&encode(&migrated).unwrap()).unwrap();
        assert!(encoded["settings"]["color_field"].get("cutoff").is_none());
    }
    older["settings"]["color_field"]["selectivity"] = serde_json::json!(0.);
    let mut zero = edited();
    zero.color_field.selectivity = 0.;
    assert_eq!(decode(&serde_json::to_vec(&older).unwrap()).unwrap(), zero);
    older["settings"]["color_field"]["selectivity"] = serde_json::Value::Null;
    assert_eq!(
        decode(&serde_json::to_vec(&older).unwrap()).unwrap(),
        expected
    );
    older["settings"]["color_field"]
        .as_object_mut()
        .unwrap()
        .remove("selectivity");
    for invalid in [0., 0.0049, 0.0201] {
        older["settings"]["color_field"]["cutoff"] = serde_json::json!(invalid);
        assert!(decode(&serde_json::to_vec(&older).unwrap()).is_err());
    }
    older["settings"]["color_field"]["cutoff"] = serde_json::json!(0.01);
    for invalid in [-0.001, 1.001] {
        older["settings"]["color_field"]["selectivity"] = serde_json::json!(invalid);
        assert!(decode(&serde_json::to_vec(&older).unwrap()).is_err());
    }
    assert!(decode(br#"{"format":"foveonic-editing","version":1,"settings":{"color_field":{"selectivity":0,"selectivity":1}}}"#).is_err());
    assert!(decode(br#"{"format":"foveonic-editing","version":1,"settings":{"color_field":{"cutoff":0.01,"cutoff":0.02}}}"#).is_err());
    assert!(decode(br#"{"format":"foveonic-editing","version":1,"settings":{"color_field":{"restraint":1,"restraint":2}}}"#).is_err());
    assert!(decode(
        br#"{"format":"foveonic-editing","version":1,"settings":{"color_field":{"unknown":1}}}"#
    )
    .is_err());
}

#[test]
fn exploratory_color_field_recipes_migrate_to_supported_brightness_only() {
    use crate::render::ColorFieldApproach;
    let mut saved: serde_json::Value = serde_json::from_slice(&encode(&edited()).unwrap()).unwrap();
    for name in ["donation", "agreement", "boundary_ev", "area"] {
        saved["settings"]["color_field"][name] = serde_json::json!("retired");
    }
    for (name, approach) in [
        ("faithful", ColorFieldApproach::Faithful),
        ("anchors", ColorFieldApproach::Faithful),
        ("uniform_support", ColorFieldApproach::Faithful),
        ("coarse", ColorFieldApproach::Faithful),
        ("fast_solve", ColorFieldApproach::Faithful),
        ("fine", ColorFieldApproach::Faithful),
        ("local_gradient", ColorFieldApproach::LocalGradient),
    ] {
        saved["settings"]["color_field"]["approach"] = serde_json::json!(name);
        let migrated = decode(&serde_json::to_vec(&saved).unwrap()).unwrap();
        let mut expected = edited();
        expected.color_field.approach = approach;
        assert_eq!(migrated, expected);
        let encoded: serde_json::Value =
            serde_json::from_slice(&encode(&migrated).unwrap()).unwrap();
        assert_eq!(
            encoded["settings"]["color_field"]
                .as_object()
                .unwrap()
                .len(),
            2
        );
    }
}

#[test]
fn monochrome_retains_both_modes_and_older_recipes_select_color() {
    let mut s = edited();
    s.rendering = crate::render::Rendering::Monochrome;
    s.monochrome = crate::render::MonochromeControls {
        mix: [0., 0., 1.],
        noise: 4,
        banding: 0,
    };
    let restored = decode(&encode(&s).unwrap()).unwrap();
    assert_eq!(s, restored);
    let mut color = restored.clone();
    color.rendering = crate::render::Rendering::Color;
    let color = decode(&encode(&color).unwrap()).unwrap();
    assert_eq!(color.monochrome, restored.monochrome);
    assert_eq!(color.reconstruction, restored.reconstruction);
    assert_eq!(color.color_mode, "Vivid");
    assert_eq!(color.saturation, 0.8);
    let mut older: serde_json::Value = serde_json::from_slice(&encode(&edited()).unwrap()).unwrap();
    for field in ["rendering", "monochrome"] {
        older["settings"].as_object_mut().unwrap().remove(field);
    }
    assert_eq!(
        decode(&serde_json::to_vec(&older).unwrap()).unwrap(),
        edited()
    );
    for mix in [[0.; 3], [-0.5, 0.5, 1.], [0.5; 3], [f32::NAN, 0., 1.]] {
        s.monochrome.mix = mix;
        assert!(encode(&s).is_err());
    }
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
    value["settings"]["saturation"] = serde_json::json!(-1.);
    assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    value["settings"]["saturation"] = serde_json::json!(1.);
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
        "\"saturation\": 0.8,",
        "\"saturation\": 0.8, \"saturation\": 1.0,",
    );
    assert!(decode(duplicate_setting.as_bytes()).is_err());
    let invalid_setting = bytes.replace("\"saturation\": 0.8,", "\"saturation\": null,");
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

#[test]
fn adjacent_recipes_move_to_visible_folder_only_on_save() {
    let directory = Directory::new();
    let input = directory.input();
    fs::remove_dir(directory.0.join("foveonic")).unwrap();
    fs::write(legacy_path(&input), encode(&edited()).unwrap()).unwrap();
    assert_eq!(load(&input).unwrap(), Some(edited()));
    assert!(!path(&input).exists());
    assert!(legacy_path(&input).exists());
    save(&input, &edited()).unwrap();
    assert_eq!(
        path(&input),
        directory.0.join("foveonic/photo.X3F.foveonic.json")
    );
    assert_eq!(load(&input).unwrap(), Some(edited()));
    assert!(!legacy_path(&input).exists());
}

#[test]
fn corrupt_legacy_recipes_are_protected_and_new_location_has_precedence() {
    let directory = Directory::new();
    let input = directory.input();
    fs::write(legacy_path(&input), b"recover me").unwrap();
    assert!(load(&input).is_err());
    assert!(save(&input, &edited()).is_err());
    fs::write(path(&input), encode(&edited()).unwrap()).unwrap();
    assert_eq!(load(&input).unwrap(), Some(edited()));
    save(&input, &Settings::default()).unwrap();
    assert_eq!(fs::read(legacy_path(&input)).unwrap(), b"recover me");
    fs::write(path(&input), b"invalid new recipe").unwrap();
    fs::write(legacy_path(&input), encode(&edited()).unwrap()).unwrap();
    assert!(load(&input).is_err());
    assert!(save(&input, &edited()).is_err());
}

#[cfg(unix)]
#[test]
fn sidecar_paths_preserve_non_utf8_filenames() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let input = PathBuf::from(std::ffi::OsString::from_vec(b"photo-\xff.X3F".to_vec()));
    assert_eq!(
        path(&input).as_os_str().as_bytes(),
        b"foveonic/photo-\xff.X3F.foveonic.json"
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
