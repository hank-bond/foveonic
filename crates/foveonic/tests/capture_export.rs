//! Exercises capture loading and edited exports from pinned example X3Fs through public APIs.

use foveonic::{
    dng,
    output::{EncodingOptions, Format},
    profile::Profile,
    render::{Engine, Settings},
    source::{read_capture_properties, Source},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn examples() -> PathBuf {
    std::env::var_os("FOVEONIC_FIXTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/example-photographs")
        })
}

fn inspect(path: &Path) -> Value {
    let output = Command::new("exiftool")
        .args(["-j", "-n", "-EXIF:all", "-ImageWidth", "-ImageHeight"])
        .arg(path)
        .output()
        .expect("install ExifTool to run capture-export tests");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    records.into_iter().next().unwrap()
}

fn run(format: Format, file: &str, expected: Value) {
    let path = examples().join(file);
    // Reuse the maintained input identities rather than accepting whichever
    // photograph happens to have the expected filename.
    let definitions: Value =
        serde_json::from_str(include_str!("../../../scripts/profiling/test_inputs.json")).unwrap();
    let external: Value = serde_json::from_str(include_str!(
        "../../../scripts/profiling/external_raw_samples.json"
    ))
    .unwrap();
    let hash = definitions["sample_hashes"][file]
        .as_str()
        .or_else(|| {
            external["files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["file"] == file)
                .and_then(|v| v["sha256"].as_str())
        })
        .expect("fixture must have a maintained identity");
    let bytes = fs::read(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; see docs/testing-coverage.md", path.display()));
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        hash,
        "fixture identity: {file}"
    );
    drop(bytes);

    let directory = tempfile::tempdir().unwrap();
    let local = directory.path().join(file);
    fs::copy(path, &local).unwrap();
    let capture = read_capture_properties(&local).unwrap();
    assert_eq!(capture["EXIF_CAPTURE_CHECKED"], "1");
    let source = Source::open(&local).unwrap();
    assert_eq!(
        capture.get("EXIF_CAPTURE_TAGS"),
        source.metadata.properties.get("EXIF_CAPTURE_TAGS"),
        "browser/source metadata differ: {file}"
    );
    // Export must use loaded metadata even after the source path disappears.
    fs::remove_file(&local).unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.rotation.turns = 1;
    settings.crop.bounds = [0.25, 0.25, 0.75, 0.75];
    settings.output_size = Some([96, 64]);
    let engine = Engine::new().unwrap();
    let destination = directory
        .path()
        .join(format!("edited.{}", format.extension()));
    let size = if format == Format::Dng {
        dng::write_source(
            &engine,
            &source,
            &settings,
            &destination,
            &mut Profile::default(),
        )
        .unwrap();
        [96, 64]
    } else {
        let image = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        let size = engine
            .write_image(
                &image,
                &destination,
                format,
                EncodingOptions {
                    jxl_effort: 1,
                    ..Default::default()
                },
                &mut Profile::default(),
            )
            .unwrap();
        assert_eq!(size, [96, 64]);
        size
    };
    let tags = inspect(&destination);
    for (key, expected) in expected.as_object().unwrap() {
        let actual = &tags[key];
        if let Some(number) = expected.as_f64() {
            assert!(
                (actual
                    .as_f64()
                    .unwrap_or_else(|| panic!("missing numeric {key}: {tags}"))
                    - number)
                    .abs()
                    < 1e-5,
                "{file} {} {key}: expected {expected}, got {actual}",
                format.label()
            );
        } else {
            assert_eq!(actual, expected, "{file} {} {key}", format.label());
        }
    }
    for (key, expected) in [
        ("Orientation", 1),
        ("ExifImageWidth", size[0]),
        ("ExifImageHeight", size[1]),
        ("ImageWidth", size[0]),
        ("ImageHeight", size[1]),
    ] {
        assert_eq!(
            tags[key],
            expected,
            "{file} {} {key}: {tags}",
            format.label()
        );
    }
    assert_eq!(tags["Software"], "Foveonic GPU");
    if format != Format::Dng {
        assert!(tags.get("DNGVersion").is_none());
    }
}

fn merrill() -> Value {
    json!({"Make":"SIGMA", "Model":"SIGMA DP2 Merrill", "ISO":200, "DateTimeOriginal":"2026:09:30 18:11:58", "ExposureTime":0.01, "FNumber":3.2, "FocalLength":30, "ExposureCompensation":0.3, "ExposureProgram":3, "MeteringMode":5, "SerialNumber":90406890})
}

#[test]
#[ignore = "uses cached example X3Fs, GPU and ExifTool; see docs/testing-coverage.md"]
fn tiff_preserves_merrill_capture_after_rotation_crop_and_resize() {
    run(Format::Tiff, "blue-parking-sign.x3f", merrill());
}

#[test]
#[ignore = "uses cached example X3Fs, GPU and ExifTool; see docs/testing-coverage.md"]
fn jpeg_preserves_classic_capture_after_rotation_crop_and_resize() {
    run(
        Format::Jpeg,
        "sd9.x3f",
        json!({"Make":"SIGMA", "Model":"SIGMA SD9", "ISO":100, "DateTimeOriginal":"2003:04:12 12:40:05", "ExposureTime":1.0/30.0, "FNumber":4.5, "FocalLength":53, "ExposureProgram":2, "MeteringMode":2, "SerialNumber":"01004355"}),
    );
}

#[test]
#[ignore = "uses cached example X3Fs, GPU and ExifTool; see docs/testing-coverage.md"]
fn png_preserves_quattro_embedded_capture_after_geometry_edits() {
    run(
        Format::Png,
        "dp0-quattro.x3f",
        json!({"Make":"SIGMA", "Model":"SIGMA dp0 Quattro", "ISO":100, "DateTimeOriginal":"2017:02:03 16:07:55", "ExposureTime":1.3, "FNumber":6.3, "FocalLength":14, "ExposureProgram":1, "SerialNumber":91002254, "Flash":0}),
    );
}

#[test]
#[ignore = "uses cached example X3Fs, GPU and ExifTool; see docs/testing-coverage.md"]
fn jxl_preserves_merrill_capture_after_rotation_crop_and_resize() {
    run(Format::Jxl, "blue-parking-sign.x3f", merrill());
}

#[test]
#[ignore = "uses cached example X3Fs, GPU and ExifTool; see docs/testing-coverage.md"]
fn dng_preserves_quattro_h_capture_after_rotation_crop_and_resize() {
    run(
        Format::Dng,
        "sd-quattro-h.x3f",
        json!({"Make":"SIGMA", "Model":"sd Quattro H", "ISO":100, "DateTimeOriginal":"2018:04:09 11:49:15", "ExposureTime":0.00025, "FNumber":2.8, "FocalLength":135, "ExposureCompensation":0.3, "ExposureProgram":3, "SerialNumber":91203029, "Flash":0}),
    );
}
