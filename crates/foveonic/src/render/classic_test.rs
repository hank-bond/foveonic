//! Exercises complete Classic originals through inspection, retained sessions, and common TIFF export.

use super::*;
use crate::{inspection::Content, source::Source};

fn reference_difference(actual: &[f32], path: &Path) -> (usize, f32, f64) {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(actual.len() * 2, bytes.len());
    let mut changed = 0;
    let mut maximum = 0_f32;
    let mut total = 0_f64;
    for (a, b) in actual.iter().zip(bytes.as_chunks::<2>().0.iter()) {
        let value = i16::from_le_bytes(*b) as f32;
        let delta = (*a - value).abs();
        changed += usize::from(delta != 0.);
        maximum = maximum.max(delta);
        total += f64::from(delta);
    }
    (changed, maximum, total / actual.len() as f64)
}

#[test]
#[ignore = "requires CLASSIC_X3F and optionally CLASSIC_REFERENCE_DIR for acquired original evidence"]
fn classic_original_inspection_session_and_tiff() {
    let source = Source::open(std::env::var("CLASSIC_X3F").unwrap()).unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    settings.tiff_color_space = Some(TiffColorSpace::Srgb);
    // The recovered neutral request adds one unit of signed root-space detail.
    settings.sharpen_amount = Some(1.);
    let engine = Engine::new().unwrap();
    let mut profile = Profile::default();
    let names = [
        "calibration.layers",
        "repair.layers",
        "highlight.layers",
        "despeckle.layers",
        "chroma.small-layers",
        "chroma.layers",
        "color.codes",
        "color.corrected-layers",
        "reconstruction.codes",
        "root.rgb",
        "detail.signal",
        "final",
    ];
    let artifacts = engine
        .render_artifacts(&source, &settings, &names, &mut profile)
        .unwrap();
    assert_eq!(artifacts.len(), names.len());
    let dump = std::env::var_os("CLASSIC_DUMP_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &dump {
        std::fs::create_dir_all(dir).unwrap();
    }
    for artifact in &artifacts {
        let Content::Image(image) = &artifact.content else {
            panic!("expected image")
        };
        let data = engine.readback_image(image).unwrap();
        let pixels = bytemuck::cast_slice::<u8, f32>(&data);
        assert!(
            pixels.iter().all(|v| v.is_finite()),
            "{} contains nonfinite pixels",
            artifact.name
        );
        if let Some(dir) = &dump {
            std::fs::write(dir.join(format!("{}.f32", artifact.name)), &data).unwrap();
        }
        if artifact.name == "reconstruction.codes" {
            if let Some(dir) = std::env::var_os("CLASSIC_REFERENCE_DIR") {
                let native = Path::new(&dir).join("progress-0060/cache-0258.bin");
                let difference = reference_difference(pixels, &native);
                eprintln!(
                    "Complete Stage2: changed={}, max={}, mean={}",
                    difference.0, difference.1, difference.2
                );
                assert!(
                    difference.1 <= 1. && difference.0 <= pixels.len() / 1000,
                    "composed Stage2 exceeds measured precision budget"
                );
            }
        }
    }
    drop(artifacts);
    let mut session = RenderSession::new(source);
    let first = engine
        .render_session(&mut session, &settings, &["final"], || false, &mut profile)
        .unwrap()
        .unwrap()
        .pop()
        .unwrap();
    let Content::Image(first) = first.content else {
        panic!("expected image")
    };
    let first_bytes = engine.readback_image(&first).unwrap();
    let activity = engine.gpu_activity();
    settings.exposure = 0.5;
    let second = engine
        .render_session(&mut session, &settings, &["final"], || false, &mut profile)
        .unwrap()
        .unwrap()
        .pop()
        .unwrap();
    let Content::Image(second) = second.content else {
        panic!("expected image")
    };
    assert_ne!(first_bytes, engine.readback_image(&second).unwrap());
    assert_eq!(first_bytes, engine.readback_image(&first).unwrap());
    assert!(
        engine.gpu_activity().upload_bytes - activity.upload_bytes < 10_000_000,
        "tone edit reuploaded original or reconstruction data"
    );
    let path = std::env::temp_dir().join(format!(
        "foveonic-classic-{}-{}.tiff",
        std::process::id(),
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    engine.write_tiff(&first, &path, &mut profile).unwrap();
    let mut tiff =
        tiff::decoder::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()))
            .unwrap();
    assert_eq!(
        tiff.dimensions().unwrap(),
        (first.view.extent[0], first.view.extent[1])
    );
    if let Some(dir) = std::env::var_os("CLASSIC_REFERENCE_DIR") {
        let tiff::decoder::DecodingResult::U16(actual) = tiff.read_image().unwrap() else {
            panic!("expected RGB16 TIFF")
        };
        let native = std::fs::read(Path::new(&dir).join("progress-0098/cache-0290.bin")).unwrap();
        assert_eq!(actual.len() * 2, native.len());
        let mut changed = 0;
        let mut maximum = 0_u16;
        let mut total = 0_u64;
        for (a, b) in actual.iter().zip(native.as_chunks::<2>().0) {
            let delta = a.abs_diff(u16::from_le_bytes(*b));
            changed += usize::from(delta != 0);
            maximum = maximum.max(delta);
            total += u64::from(delta);
        }
        let mean = total as f64 / actual.len() as f64;
        eprintln!("Complete TIFF: changed={changed}, max={maximum}, mean={mean}");
        // Shared output uses the standard destination profile rather than the
        // legacy fixed-point transform. This bounds the measured appearance
        // difference separately from the unchanged signed Stage 2 budget.
        assert!(
            maximum <= 256 && mean < 5.,
            "generic output exceeds measured legacy-profile difference"
        );
    }
    assert!(engine.write_tiff(&first, &path, &mut profile).is_err());
    std::fs::remove_file(path).unwrap();
}

use std::time::SystemTime;

#[test]
#[ignore = "requires CLASSIC_X3F original-file inspection evidence and a GPU adapter"]
fn classic_calibration_inspection_defers_color_selection() {
    let mut source = Source::open(std::env::var("CLASSIC_X3F").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let mut profile = Profile::default();
    for mode in ["", "unavailable"] {
        let settings = Settings::new("Sunlight", mode);
        let artifacts = engine
            .render_artifacts(&source, &settings, &["calibration.layers"], &mut profile)
            .unwrap();
        assert_eq!(artifacts.len(), 1);
        assert!(engine
            .render_artifacts(&source, &settings, &["final"], &mut profile)
            .is_err());
    }
    let metadata = std::sync::Arc::make_mut(&mut source.metadata);
    metadata.lists.remove("ColorModeCompensations");
    metadata.lists.remove("ColorModeContrastCompensations");
    metadata
        .matrices
        .get_mut("ColorDQCamRGB")
        .unwrap()
        .values
        .truncate(1);
    for mode in ["", "unavailable", "Standard"] {
        let settings = Settings::new("Sunlight", mode);
        let artifacts = engine
            .render_artifacts(&source, &settings, &["calibration.layers"], &mut profile)
            .unwrap();
        assert_eq!(artifacts.len(), 1);
        assert!(engine
            .render_artifacts(&source, &settings, &["final"], &mut profile)
            .is_err());
    }
}
