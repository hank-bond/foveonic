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

#[test]
#[ignore = "requires CLASSIC_X3F and CLASSIC_REFERENCE_DIR original-file fixtures"]
fn classic_original_stage2_matches_reference() {
    let source = Source::open(std::env::var("CLASSIC_X3F").unwrap()).unwrap();
    let white_balance =
        std::env::var("CLASSIC_WHITE_BALANCE").unwrap_or_else(|_| "Sunlight".into());
    let mut settings = Settings::new(white_balance, "Standard");
    settings.reconstruction = Some(Default::default());
    settings.sharpen_amount = Some(1.);
    let engine = Engine::new().unwrap();
    let iso = source
        .metadata
        .scalar("CaptureISO")
        .unwrap_or_else(|_| source.metadata.properties["ISO"].parse().unwrap());
    let recursive_chroma = iso < 400. && source.metadata.scalar("ChromaBlur").unwrap_or(0.) > 100.;
    let stage2_name = if recursive_chroma {
        "chroma.rgb-codes"
    } else {
        "reconstruction.codes"
    };
    let artifacts = engine
        .render_artifacts(
            &source,
            &settings,
            &["despeckle.layers", stage2_name],
            &mut Profile::default(),
        )
        .unwrap();
    for artifact in artifacts {
        let Content::Image(image) = artifact.content else {
            panic!("expected image")
        };
        let data = engine.readback_image(&image).unwrap();
        if let Some(directory) = std::env::var_os("CLASSIC_DUMP_DIR") {
            let directory = Path::new(&directory);
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(directory.join(format!("{}.f32", artifact.name)), &data).unwrap();
        }
        if artifact.name == stage2_name {
            let directory = std::env::var_os("CLASSIC_REFERENCE_DIR").unwrap();
            let native = Path::new(&directory).join("progress-0060/cache-0258.bin");
            let pixels = bytemuck::cast_slice::<u8, f32>(&data);
            let difference = if std::env::var_os("CLASSIC_STAGE2_VISIBLE").is_some() {
                let active: [f64; 4] = source.metadata.fixed("ActiveImageArea").unwrap();
                let keep: [f64; 4] = source.metadata.fixed("KeepImageArea").unwrap();
                let expected = std::fs::read(&native).unwrap();
                let expected: Vec<f32> = expected
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|v| i16::from_le_bytes(*v) as f32)
                    .collect();
                let mut changed = 0;
                let mut maximum = 0_f32;
                let mut total = 0_f64;
                let mut count = 0;
                for y in (active[1] - keep[1]) as usize..=(active[3] - keep[1]) as usize {
                    for x in (active[0] - keep[0]) as usize..=(active[2] - keep[0]) as usize {
                        for c in 0..3 {
                            let at = (y * source.metadata.width + x) * 3 + c;
                            let delta = (pixels[at] - expected[at]).abs();
                            changed += usize::from(delta != 0.);
                            maximum = maximum.max(delta);
                            total += f64::from(delta);
                            count += 1;
                        }
                    }
                }
                (changed, maximum, total / count as f64)
            } else {
                reference_difference(pixels, &native)
            };
            eprintln!(
                "Complete Stage2: changed={}, max={}, mean={}",
                difference.0, difference.1, difference.2
            );
            let maximum =
                std::env::var("CLASSIC_STAGE2_MAX_ERROR").map_or(1., |v| v.parse::<f32>().unwrap());
            let mean = std::env::var("CLASSIC_STAGE2_MAX_MEAN")
                .map_or(0.001, |v| v.parse::<f64>().unwrap());
            assert!(
                difference.1 <= maximum && difference.2 <= mean,
                "composed Stage2 exceeds the supplied reference precision budget"
            );
        }
    }
}

#[test]
#[ignore = "requires CLASSIC_X3F original and a GPU"]
fn classic_low_iso_filter_toggle_preserves_photographic_and_dng_brightness() {
    let source = Source::open(std::env::var("CLASSIC_X3F").unwrap()).unwrap();
    assert!(
        source.metadata.scalar("CaptureISO").unwrap() < 400.,
        "use a low-ISO original for the fixed-point reconstruction bypass regression"
    );
    let engine = Engine::new().unwrap();
    let mut means = Vec::new();
    for enabled in [false, true] {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = enabled.then(Default::default);
        settings.sharpen_amount = Some(0.);
        let mut profile = Profile::default();
        let photograph = engine
            .render_artifacts(&source, &settings, &["final"], &mut profile)
            .unwrap();
        let Content::Image(photo) = &photograph[0].content else {
            panic!("expected image")
        };
        let dng = engine
            .render_dng_base(&source, &settings, &mut profile)
            .unwrap();
        let mean = |image: &crate::image::RenderedImage| {
            let data = engine.readback_image(image).unwrap();
            let values = bytemuck::cast_slice::<u8, f32>(&data);
            values.iter().map(|v| f64::from(*v)).sum::<f64>() / values.len() as f64
        };
        means.push([mean(photo), mean(&dng)]);
    }
    for (i, domain) in ["photographic linear RGB", "DNG linear codes"]
        .into_iter()
        .enumerate()
    {
        let stops = (means[1][i] / means[0][i]).log2();
        eprintln!(
            "{domain}: off {}, on {}, change {stops} stops",
            means[0][i], means[1][i]
        );
        // Spatial filtering can change averages slightly. It must not drop the
        // camera normalization or digital gain at either export boundary.
        assert!(
            stops.abs() < 0.1,
            "{domain} brightness changes by {stops} stops"
        );
    }
}

#[test]
#[ignore = "requires CLASSIC_X3F and optionally CLASSIC_REFERENCE_DIR monochrome original captures"]
fn classic_monochrome_original_matches_reference_and_ignores_color_wb() {
    let mut source = Source::open(std::env::var("CLASSIC_X3F").unwrap()).unwrap();
    if std::env::var_os("CLASSIC_REFERENCE_DIR").is_some() {
        // The explicit native fixture request uses eRotate=1 (unrotated).
        // Match its presentation before comparing spatial pixels.
        std::sync::Arc::make_mut(&mut source.metadata).rotation = 0;
    }
    let engine = Engine::new().unwrap();
    let mode = std::env::var("CLASSIC_COLOR_MODE").unwrap_or_else(|_| "Standard".into());
    let mut settings = Settings::new("Auto", mode);
    settings.rendering = Rendering::Monochrome;
    settings.reconstruction = Some(Default::default());
    settings.sharpen_amount = Some(1.);
    settings.tiff_color_space = Some(TiffColorSpace::Srgb);
    let high_iso = source.metadata.scalar("CaptureISO").unwrap() >= 400.;
    let codes_name = if high_iso {
        "reconstruction.codes"
    } else {
        "color.codes"
    };
    let all_names = crate::inspection::output_names(&settings, Some(&source.metadata));
    assert!(!all_names.contains(&"red-detail.layers"));
    assert!(!all_names.contains(&"highlight.layers"));
    assert!(!all_names.contains(&"chroma.layers"));
    assert!(!all_names.contains(&"color.gained-codes"));
    let artifacts = engine
        .render_artifacts(
            &source,
            &settings,
            &[codes_name, "final"],
            &mut Profile::default(),
        )
        .unwrap();
    let Content::Image(codes) = &artifacts[0].content else {
        panic!("expected codes")
    };
    let Content::Image(photo) = &artifacts[1].content else {
        panic!("expected photograph")
    };
    let codes = engine.readback_image(codes).unwrap();
    let photograph = engine.readback_image(photo).unwrap();
    assert!(bytemuck::cast_slice::<u8, f32>(&photograph)
        .iter()
        .all(|x| x.is_finite()));
    settings.white_balance = "Incandescent".into();
    let again = engine
        .render_artifacts(&source, &settings, &["final"], &mut Profile::default())
        .unwrap();
    let Content::Image(again) = &again[0].content else {
        panic!("expected photograph")
    };
    assert_eq!(
        photograph,
        engine.readback_image(again).unwrap(),
        "ordinary WB must be inactive"
    );
    if let Some(directory) = std::env::var_os("CLASSIC_REFERENCE_DIR") {
        let native =
            std::fs::read(Path::new(&directory).join("progress-0060/cache-0258.bin")).unwrap();
        let native: Vec<_> = native
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b) as f32)
            .collect();
        let actual = bytemuck::cast_slice::<u8, f32>(&codes);
        assert_eq!(actual.len(), native.len());
        let active: [f64; 4] = source.metadata.fixed("ActiveImageArea").unwrap();
        let keep: [f64; 4] = source.metadata.fixed("KeepImageArea").unwrap();
        let mut total = 0.;
        let mut maximum = 0_f32;
        let mut count = 0;
        for y in (active[1] - keep[1]) as usize..=(active[3] - keep[1]) as usize {
            for x in (active[0] - keep[0]) as usize..=(active[2] - keep[0]) as usize {
                for c in 0..3 {
                    let at = (y * source.metadata.width + x) * 3 + c;
                    let error = (actual[at] - native[at]).abs();
                    total += f64::from(error);
                    maximum = maximum.max(error);
                    count += 1;
                }
            }
        }
        let mean = total / count as f64;
        eprintln!("Monochrome Stage2 mean {mean} codes, max {maximum}");
        assert!(
            mean < 2. && maximum < 100.,
            "monochrome technical boundary differs from reference"
        );
        let temporary = std::env::temp_dir().join(format!(
            "classic-mono-reference-{}.tiff",
            std::process::id()
        ));
        engine
            .write_tiff(photo, &temporary, &mut Profile::default())
            .unwrap();
        let mut decoder = tiff::decoder::Decoder::new(std::io::BufReader::new(
            std::fs::File::open(&temporary).unwrap(),
        ))
        .unwrap();
        let receipt: serde_json::Value = serde_json::from_slice(
            &std::fs::read(Path::new(&directory).join("receipt.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            decoder.dimensions().unwrap(),
            (
                receipt["output_dimensions"][0].as_u64().unwrap() as u32,
                receipt["output_dimensions"][1].as_u64().unwrap() as u32,
            ),
            "reference presentation dimensions differ"
        );
        let tiff::decoder::DecodingResult::U16(actual) = decoder.read_image().unwrap() else {
            panic!("expected RGB16")
        };
        std::fs::remove_file(&temporary).unwrap();
        let native = std::fs::read(Path::new(&directory).join("output.pixels.bin")).unwrap();
        assert_eq!(actual.len() * 2, native.len());
        let mean = actual
            .iter()
            .zip(native.as_chunks::<2>().0)
            .map(|(a, b)| f64::from((*a as i32 - u16::from_le_bytes(*b) as i32).abs()))
            .sum::<f64>()
            / actual.len() as f64;
        eprintln!("Monochrome photographic mean {mean} sRGB16 codes");
        assert!(
            mean < 256.,
            "monochrome photographic boundary differs from reference"
        );
    }
}
