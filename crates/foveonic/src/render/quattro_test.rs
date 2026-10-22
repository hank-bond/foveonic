//! Checks mixed-resolution sensor inspection, selective execution, and immutable retained planes.

use super::*;
use crate::{
    image::{Layout, Space},
    inspection::Content,
    source::{Metadata, ProcessingFamily, Source, SourceStorage},
};

// Native boundary fixtures use explicit diagnostic controls, independently of
// the file defaults selected for photographic rendering.
fn diagnostic_settings() -> Settings {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(crate::noise::Controls {
        chroma: 0,
        luminance: 0,
        banding: 0,
        detail: 1.,
        ..recipes::quattro::default_noise_controls()
    });
    settings
}

fn source() -> Source {
    Source {
        storage: SourceStorage::PackedRgbBluePhases,
        metadata: Arc::new(Metadata {
            processing_version: Some([3, 1, 2, 0]),
            width: 6,
            height: 4,
            rotation: 90,
            properties: [("CAMMODEL".into(), "SIGMA dp2 Quattro".into())].into(),
            matrices: Default::default(),
            lists: Default::default(),
        }),
        pixels: (0..6)
            .flat_map(|i| [2100. + i as f32, 2200., 2300., 2400., 2500., 2600.])
            .collect(),
        camf: vec![],
        timings: vec![],
    }
}

#[test]
fn raw_catalogue_does_not_require_calibration_fields() {
    let mut source = source();
    let m = Arc::make_mut(&mut source.metadata);
    m.processing_version = Some([4, 0, 0, 0]);
    assert_eq!(m.processing_family().unwrap(), ProcessingFamily::Quattro);
    let settings = Settings::new("", "");
    assert_eq!(
        crate::inspection::output_names(&settings, Some(m)),
        ["sensor.rgb", "sensor.blue"]
    );
}

#[test]
#[ignore = "requires a GPU"]
fn planes_keep_their_extents_and_retained_siblings() {
    let engine = Engine::new().unwrap();
    let settings = Settings::new("", "");
    let mut session = RenderSession::new(source());
    let mut retained = Vec::new();
    // Begin with a side output. Later requests must preserve earlier snapshots.
    for name in [
        "sensor.blue",
        "sensor.rgb",
        "extraction.blue",
        "extraction.rgb",
        "sensor.blue",
    ] {
        let artifacts = engine
            .render_session(
                &mut session,
                &settings,
                &[name],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        let Content::Image(image) = &artifacts[0].content else {
            panic!("expected plane");
        };
        let blue = name.ends_with("blue");
        let corrected = name.starts_with("extraction.");
        let extent = if blue { [6, 4] } else { [3, 2] };
        assert_eq!(image.description().extent, extent);
        assert_eq!(
            image.description().layout,
            if blue {
                Layout::Scalar32Float
            } else {
                Layout::Rgb32Float
            }
        );
        assert_eq!(
            image.description().space,
            match (blue, corrected) {
                (false, false) => Space::SensorRgb,
                (true, false) => Space::SensorBlue,
                (false, true) => Space::CorrectedSensorRgb,
                (true, true) => Space::CorrectedSensorBlue,
            }
        );
        assert_eq!(image.view().crop, [0, 0, extent[0], extent[1]]);
        assert_eq!(image.view().extent, [extent[1], extent[0]]);
        let bytes = engine.readback_image(image).unwrap();
        let values: &[f32] = bytemuck::cast_slice(&bytes);
        assert_eq!(
            values[0],
            if blue {
                if corrected {
                    252.
                } else {
                    2300.
                }
            } else if corrected {
                52.
            } else {
                2100.
            }
        );
        let scopes = engine.scopes(image, None, &mut Profile::default()).unwrap();
        assert_eq!(scopes.channels(), if blue { 1 } else { 3 });
        assert_eq!(
            scopes.histogram(0).iter().sum::<u32>(),
            extent[0] * extent[1]
        );
        assert_eq!(scopes.outside(0), [0, 0, 0]);
        retained.push((image.clone(), bytes));
        for (prior, expected) in &retained {
            assert_eq!(&engine.readback_image(prior).unwrap(), expected);
        }
    }
    let mixed = engine
        .render_session(
            &mut session,
            &settings,
            &["sensor.blue", "extraction.rgb"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(mixed.len(), 2);
    assert!(engine
        .render(session.source(), &settings, &mut Profile::default())
        .is_err());

    // Earlier requests must stop before unrelated calibration preparation.
    let mut uncalibrated = source();
    Arc::make_mut(&mut uncalibrated.metadata).matrices.insert(
        "RawOffset".into(),
        crate::source::Matrix {
            encoding: crate::source::MatrixEncoding::Float32,
            representation: "f64",
            dimensions: vec![1],
            values: vec![f64::NAN],
        },
    );
    assert!(engine
        .render_artifacts(
            &uncalibrated,
            &settings,
            &["sensor.rgb", "sensor.blue"],
            &mut Profile::default(),
        )
        .is_ok());
    assert!(engine
        .render_artifacts(
            &uncalibrated,
            &settings,
            &["extraction.rgb"],
            &mut Profile::default(),
        )
        .is_err());
}

#[test]
#[ignore = "requires QUATTRO_X3F and a GPU"]
fn original_quattro_planes_have_independent_geometry() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let settings = Settings::new("", "");
    let directory = std::env::var_os("QUATTRO_RESOURCE_DIRECTORY");
    let engine = if let Some(directory) = &directory {
        Engine::new()
            .unwrap()
            .with_calibration_resources(directory)
            .unwrap()
    } else {
        Engine::new().unwrap()
    };
    let names = crate::inspection::output_names(&settings, Some(&source.metadata));
    for name in names.into_iter().filter(|name| {
        name.starts_with("sensor.") || name.starts_with("extraction.") || directory.is_some()
    }) {
        let result = engine
            .render_artifacts(&source, &settings, &[name], &mut Profile::default())
            .unwrap();
        let Content::Image(image) = &result[0].content else {
            panic!("expected plane");
        };
        let extent = source.storage_extent().map(|v| v as u32);
        assert_eq!(
            image.description().extent,
            if name.ends_with("blue") {
                extent.map(|v| v * 2)
            } else if name == "hlp.scratch" {
                [(extent[0] - 1) / 16, (extent[1] - 1) / 4]
            } else {
                extent
            }
        );
        let bytes = engine.readback_image(image).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .all(|v| v.is_finite()));
    }
}

#[test]
#[ignore = "requires DP2 QUATTRO_X3F and PHASE_GAIN_REFERENCE"]
fn original_gain_stage_matches_independent_native_output() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source);
    let settings = Settings::new("", "");
    let result = engine
        .render_session(
            &mut session,
            &settings,
            &["phase-gain.blue"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    let Content::Image(image) = &result[0].content else {
        panic!("expected blue plane");
    };
    let native = std::path::PathBuf::from(std::env::var_os("PHASE_GAIN_REFERENCE").unwrap())
        .join("c81-original-output.f32");
    assert!(
        engine.readback_image(image).unwrap() == std::fs::read(native).unwrap(),
        "bundled phase-gain output differs from native"
    );
    let stored = engine.readback_image(image).unwrap();
    let again = engine
        .render_session(
            &mut session,
            &settings,
            &["extraction.blue", "phase-gain.blue"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(again.len(), 2);
    assert_eq!(engine.readback_image(image).unwrap(), stored);
}

#[test]
#[ignore = "requires DP2 QUATTRO_X3F, QUATTRO_RESOURCE_DIRECTORY and QUATTRO_CALIBRATION_BOUNDARY"]
fn composed_calibration_matches_stopped_native_boundaries() {
    let mut source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    // The retained native request explicitly uses f/4. BData still selects its
    // aperture from CAMF independently of this request field.
    Arc::make_mut(&mut source.metadata)
        .properties
        .insert("APERTURE".into(), "4".into());
    let directory =
        std::path::PathBuf::from(std::env::var_os("QUATTRO_CALIBRATION_BOUNDARY").unwrap());
    let engine = Engine::new()
        .unwrap()
        .with_calibration_resources(std::env::var_os("QUATTRO_RESOURCE_DIRECTORY").unwrap())
        .unwrap();
    let mut session = RenderSession::new(source);
    let names = [
        "blue-refresh.rgb",
        "hlp.rgb",
        "hlp.blue",
        "hlp.scratch",
        "hlp-repair.rgb",
        "hlp-repair.blue",
    ];
    let results = engine
        .render_session(
            &mut session,
            &Settings::new("", ""),
            &names,
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    for (name, result) in names.into_iter().zip(&results) {
        let Content::Image(image) = &result.content else {
            panic!("expected image");
        };
        let expected_name = match name {
            "blue-refresh.rgb" => "hlp-input-rgb.f32",
            "hlp.rgb" => "hlp-output-rgb.f32",
            "hlp.blue" => "hlp-output-blue.f32",
            "hlp.scratch" => "hlp-output-scratch.f32",
            "hlp-repair.rgb" => "sparse-output-rgb.f32",
            "hlp-repair.blue" => "sparse-output-blue.f32",
            _ => unreachable!(),
        };
        let actual = engine.readback_image(image).unwrap();
        let expected = std::fs::read(directory.join(expected_name)).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&actual);
        let expected: &[f32] = bytemuck::cast_slice(&expected);
        // Native scratch has an unwritten trailing row and column. Compare only
        // the compact initialized samples, respecting its physical row stride.
        let scratch_width = (session.source().metadata.width / 2) / 16;
        let valid_width = image.description().extent[0] as usize;
        let mut maximum = 0_f32;
        let mut total = 0_f64;
        let mut different = 0;
        let mut native_padding_writes = 0;
        for (i, &a) in actual.iter().enumerate() {
            let index = if name == "hlp.scratch" {
                (i / (valid_width * 3)) * scratch_width * 3 + i % (valid_width * 3)
            } else {
                i
            };
            let b = expected[index];
            assert!(a.is_finite() && b.is_finite(), "{name} component {i}");
            // The captured eight-strip AverageBlue callback writes an extra
            // column. At each strip boundary it races with the next strip's
            // first blue component. Our operation stays inside logical rows.
            // These seven known caller artifacts sit outside the active crop.
            let extent = image.description().extent;
            let row = (i / 3) / extent[0] as usize;
            let reference_strip_rows = 1836 / 8;
            if !name.ends_with(".blue")
                && name != "hlp.scratch"
                && i % 3 == 2
                && (i / 3).is_multiple_of(extent[0] as usize)
                && row > 0
                && row.is_multiple_of(reference_strip_rows)
                && row / reference_strip_rows < 8
            {
                native_padding_writes += usize::from(a != b);
                continue;
            }
            let error = (a - b).abs();
            if error != 0. {
                if different < 8 {
                    eprintln!("{name} difference {i}: {a} vs {b}");
                }
                different += 1;
            }
            maximum = maximum.max(error);
            total += f64::from(error);
        }
        eprintln!(
            "{name}: {} components, {different} different, {native_padding_writes} native padding writes, max {maximum}, mean {}",
            actual.len(),
            total / actual.len() as f64
        );
        assert!(maximum <= 0.05, "{name} maximum code error {maximum}");
    }
}

#[test]
#[ignore = "requires DP2 original, explicit maps and stopped native PREPROCESS/Stage1 captures"]
fn composed_preprocess_and_noise_match_native_active_image() {
    let mut source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    Arc::make_mut(&mut source.metadata)
        .properties
        .insert("APERTURE".into(), "4".into());
    let pre = std::path::PathBuf::from(std::env::var_os("QUATTRO_PREPROCESS_BOUNDARY").unwrap());
    let noise = std::path::PathBuf::from(std::env::var_os("QUATTRO_STAGE1_BOUNDARY").unwrap());
    let engine = Engine::new()
        .unwrap()
        .with_calibration_resources(std::env::var_os("QUATTRO_RESOURCE_DIRECTORY").unwrap())
        .unwrap();
    let mut session = RenderSession::new(source);
    let names = [
        "highlight-restoration.rgb",
        "blue-mismatch.blue",
        "noise.rgb",
        "noise.source-after",
    ];
    let results = engine
        .render_session(
            &mut session,
            &diagnostic_settings(),
            &names,
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    for (name, result) in names.into_iter().zip(&results) {
        let Content::Image(image) = &result.content else {
            panic!("expected image")
        };
        let expected = std::fs::read(match name {
            "highlight-restoration.rgb" => pre.join("rgb.f32"),
            "blue-mismatch.blue" => pre.join("blue.f32"),
            "noise.rgb" => noise.join("output-rgb.f32"),
            "noise.source-after" => noise.join("source-after-rgb.f32"),
            _ => unreachable!(),
        })
        .unwrap();
        let actual = engine.readback_image(image).unwrap();
        let a: &[f32] = bytemuck::cast_slice(&actual);
        let b: &[f32] = bytemuck::cast_slice(&expected);
        assert_eq!(a.len(), b.len());
        let blue = name == "blue-mismatch.blue";
        let [left, top, right, bottom] = if blue {
            [216, 28, 5640, 3644]
        } else {
            [108, 14, 2820, 1822]
        };
        let channels = if blue { 1 } else { 3 };
        let width = image.description().extent[0] as usize;
        let mut count = 0;
        let mut changed = 0;
        let mut max = 0_f32;
        let mut sum = 0_f64;
        for y in top..bottom {
            for x in left..right {
                for c in 0..channels {
                    let i = (y * width + x) * channels + c;
                    assert!(a[i].is_finite() && b[i].is_finite());
                    let error = (a[i] - b[i]).abs();
                    count += 1;
                    changed += usize::from(error != 0.);
                    max = max.max(error);
                    sum += f64::from(error);
                }
            }
        }
        eprintln!(
            "{name}: active {count} words, changed {changed}, max {max}, mean {}",
            sum / count as f64
        );
        // Tiny preprocessing differences can cross a Stage1 classification
        // boundary. Bound the composed result separately from native-input
        // worker fixtures, which have a much smaller numerical error.
        let maximum = if name == "noise.rgb" { 6. } else { 0.004 };
        assert!(max < maximum, "{name}: composed maximum {max}");
        assert!(sum / (count as f64) < 0.0001, "{name}: composed mean");
    }
}

#[test]
#[ignore = "requires a Quattro original and a new QUATTRO_PHOTOGRAPH TIFF path"]
fn complete_color_photograph_and_tiff() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source);
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
    let results = engine
        .render_session(
            &mut session,
            &Settings::new("Sunlight", "Standard"),
            &["final", "sharpen.rgb"],
            || false,
            &mut profile,
        )
        .unwrap()
        .unwrap();
    let Content::Image(image) = &results[0].content else {
        panic!("expected photograph")
    };
    assert_eq!(
        image.description().space,
        Space::ToneMappedRgb(crate::color_space::ColorSpace::Srgb)
    );
    assert_eq!(image.view().rotation, 0);
    let Content::Image(working) = &results[1].content else {
        panic!("expected working image")
    };
    let area = session
        .source
        .metadata
        .fixed::<4>("ActiveImageArea")
        .unwrap();
    assert_eq!(
        working.view().crop,
        [
            area[0] as u32,
            area[1] as u32,
            area[2] as u32 + 1,
            area[3] as u32 + 1
        ]
    );
    let mut expected_extent = [
        (area[2] - area[0] + 1.) as u32,
        (area[3] - area[1] + 1.) as u32,
    ];
    if matches!(session.source.metadata.rotation, 90 | 270) {
        expected_extent.swap(0, 1);
    }
    assert_eq!(
        image.view().crop,
        [0, 0, expected_extent[0], expected_extent[1]]
    );
    assert_eq!(working.view().rotation, session.source.metadata.rotation);
    assert!(!working.description().view_baked);
    assert!(image.description().view_baked);
    let bytes = engine.readback_image(image).unwrap();
    let values: &[f32] = bytemuck::cast_slice(&bytes);
    assert!(values
        .iter()
        .all(|v| v.is_finite() && (0. ..=1.).contains(v)));
    assert!(values.iter().any(|v| *v > 0.5));
    assert!(values.iter().any(|v| *v < 0.01));
    let path = std::path::PathBuf::from(std::env::var_os("QUATTRO_PHOTOGRAPH").unwrap());
    let extent = engine.write_tiff(image, &path, &mut profile).unwrap();
    assert_eq!(extent, expected_extent);
    eprintln!("Quattro photograph {extent:?}: GPU {:?}", profile.gpu);
}

#[test]
#[ignore = "requires an ISO100 Quattro original and its complete native diagnostic export"]
fn complete_photograph_matches_native_export() {
    compare_complete_native_photograph(false);
}

#[test]
#[ignore = "requires the DP3 ISO800 original and its complete native diagnostic export"]
fn complete_high_iso_photograph_matches_native_export() {
    compare_complete_native_photograph(true);
}

fn compare_complete_native_photograph(recorded_range_cutoff_witness: bool) {
    let mut source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let camera_iso = (
        source.metadata.scalar("CAMERAID").unwrap(),
        source.metadata.scalar("CaptureISO").unwrap(),
    );
    // The reference diagnostic requests unrotated output explicitly.
    Arc::make_mut(&mut source.metadata).rotation = 0;
    if recorded_range_cutoff_witness {
        assert_eq!(source.metadata.scalar("CAMERAID").unwrap(), 82.);
        assert_eq!(source.metadata.scalar("CaptureISO").unwrap(), 800.);
        assert_eq!(source.metadata.scalar("SensorISO").unwrap(), 400.);
    }
    Arc::make_mut(&mut source.metadata)
        .properties
        .insert("APERTURE".into(), "4".into());
    Arc::make_mut(&mut source.metadata)
        .properties
        .insert("SHUTTER".into(), "0.01".into());
    let mut settings = Settings::new(
        std::env::var("QUATTRO_WB").unwrap_or_else(|_| "Sunlight".into()),
        std::env::var("QUATTRO_MODE").unwrap_or_else(|_| "Standard".into()),
    );
    // Native diagnostics explicitly select minimum noise and maximum Crisp.
    // Keep these choices independent of photographic file-loader defaults.
    settings.reconstruction = Some(crate::noise::Controls {
        chroma: std::env::var("QUATTRO_CHROMA").map_or(0, |v| v.parse().unwrap()),
        luminance: std::env::var("QUATTRO_LUMA").map_or(0, |v| v.parse().unwrap()),
        detail: std::env::var("QUATTRO_DETAIL").map_or(1., |v| v.parse().unwrap()),
        ..super::recipes::quattro::default_noise_controls()
    });
    if std::env::var_os("QUATTRO_MONO").is_some() {
        settings.rendering = super::Rendering::Monochrome;
        if let Ok(mix) = std::env::var("QUATTRO_MONO_MIX") {
            settings.monochrome.mix = mix
                .split(',')
                .map(|v| v.parse::<f32>().unwrap())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
        }
        settings.monochrome_smoothness =
            Some(std::env::var("QUATTRO_DETAIL").map_or(1., |v| v.parse().unwrap()));
        settings.monochrome.noise = std::env::var("QUATTRO_LUMA").map_or(0, |v| v.parse().unwrap());
    }
    if let Ok(base) = std::env::var("QUATTRO_SHARPNESS") {
        settings.sharpen_amount = Some(base.parse().unwrap());
    }
    let filtered_cutoff_witness = recorded_range_cutoff_witness
        && settings
            .reconstruction
            .is_some_and(|controls| controls.luminance == 2 && controls.detail == 1.);
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source);
    let results = engine
        .render_session(
            &mut session,
            &settings,
            &["final"],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    let Content::Image(image) = &results[0].content else {
        panic!("expected photograph")
    };
    let mut graph = crate::graph::Graph::default();
    let input = graph
        .add(
            crate::graph::Operation::RenderedInput {
                data: image.description().space.into(),
            },
            &[],
        )
        .unwrap();
    let mut params = image.params();
    params.output[2] = 0;
    params.output[3] = 0;
    let encoded = graph
        .add(
            crate::graph::Operation::Encode {
                params,
                space: crate::color_space::ColorSpace::Srgb,
                destination: TiffColorSpace::Srgb,
            },
            &[input],
        )
        .unwrap();
    let values = engine
        .execute(
            execute::Input::Rendered(image),
            &graph,
            &[encoded],
            &mut Profile::default(),
        )
        .unwrap();
    let actual = engine.gpu.readback(&values[0].buffer).unwrap();
    assert_eq!(actual.len(), 5424 * 3616 * 8);
    if let Some(path) = std::env::var_os("QUATTRO_COMPOSED_RGB16") {
        std::fs::write(path, &actual).unwrap();
    }
    let root = std::path::PathBuf::from(std::env::var_os("QUATTRO_FULL_REFERENCE").unwrap());
    let expected = std::fs::read(root.join("output.pixels.bin")).unwrap();
    assert_eq!(expected.len(), 5424 * 3616 * 6);
    let mut maximum = 0_u16;
    let mut sum = 0_u64;
    let mut different = 0;
    let mut over_one = 0;
    let mut over_256 = 0;
    let mut over_256_by_channel = [0_usize; 3];
    let mut unexpected_outliers = Vec::new();
    for (pixel, (packed, wanted)) in actual
        .as_chunks::<8>()
        .0
        .iter()
        .zip(expected.as_chunks::<6>().0)
        .enumerate()
    {
        for c in 0..3 {
            let a = u16::from_le_bytes([packed[c * 2], packed[c * 2 + 1]]);
            let b = u16::from_le_bytes([wanted[c * 2], wanted[c * 2 + 1]]);
            let error = a.abs_diff(b);
            maximum = maximum.max(error);
            sum += u64::from(error);
            different += usize::from(error != 0);
            over_one += usize::from(error > 1);
            if error > 256 {
                over_256 += 1;
                over_256_by_channel[c] += 1;
                if recorded_range_cutoff_witness {
                    // A one-ULP preprocessing change crosses the second-pass
                    // range-table cutoff. Keep its measured exception confined
                    // to this original's small green and blue patches.
                    let x = pixel % 5424;
                    let y = pixel / 5424;
                    let blue_witness = c == 2
                        && (3110..=3118).contains(&x)
                        && (1962..=1968).contains(&y)
                        && error <= 966;
                    // The measured step-two filter spreads the same cutoff
                    // witness into two neighboring green positions.
                    let green_bounds = if filtered_cutoff_witness {
                        ((3112..=3116).contains(&x), (1965..=1967).contains(&y))
                    } else {
                        ((3112..=3115).contains(&x), (1966..=1967).contains(&y))
                    };
                    let green_witness = c == 1 && green_bounds.0 && green_bounds.1 && error <= 345;
                    if !(blue_witness || green_witness) {
                        unexpected_outliers.push((x, y, c, error));
                    }
                }
            }
        }
    }
    let count = 5424 * 3616 * 3;
    eprintln!(
        "complete native TIFF response: {count} components, {different} different, {over_one} over one, max {maximum}, mean {}",
        sum as f64 / count as f64
    );
    let measured_file_defaults = recorded_range_cutoff_witness
        && settings.white_balance == "Auto"
        && settings.color_mode == "Standard"
        && settings.reconstruction.is_some_and(|controls| {
            controls.chroma == 2 && controls.luminance == 2 && controls.detail == 0.5
        });
    if settings.is_monochrome() {
        // Recorded dyadic channel weights have their own native comparisons.
        // Equal-weight requests remain explicit diagnostics with separate bounds.
        let smoothness = settings.monochrome_smoothness.unwrap_or(0.5);
        let file_mix =
            settings.monochrome.mix == [10813_f32 / 32768., 10813_f32 / 32768., 11142_f32 / 32768.];
        let (mean_limit, maximum_limit, different_per_mille) =
            match (settings.monochrome.noise, smoothness, file_mix) {
                (0, 1., false) => (2.6, 210, 160),
                (2, 0.5, false) => (2.3, 145, 150),
                (2, 0.5, true) if camera_iso == (40., 100.) => (0.04, 145, 5),
                (2, 0.5, true) if camera_iso == (82., 800.) => (0.16, 65, 13),
                _ => panic!("no measured complete monochrome boundary for these controls"),
            };
        assert!(maximum <= maximum_limit, "monochrome maximum code error");
        assert!(
            (sum as f64 / count as f64) < mean_limit,
            "monochrome mean code error"
        );
        assert!(
            different * 1000 < count * different_per_mille,
            "monochrome exact component fraction"
        );
    } else if measured_file_defaults {
        // The retained ISO800 Auto/default-control request has maximum 93,
        // mean 0.14709 RGB16 codes and 98.91% exact components. Its midpoint
        // smoothing avoids the minimum-noise fixture's isolated cutoff witness.
        assert_eq!(over_256, 0, "file-default outliers");
        assert!(maximum <= 96, "file-default maximum code error");
        assert!(different * 1000 < count * 12, "file-default exact fraction");
        assert!(over_one * 100 < count, "file-default errors above one code");
        assert!(
            (sum as f64 / count as f64) < 0.16,
            "file-default mean error"
        );
    } else if recorded_range_cutoff_witness {
        assert!(
            unexpected_outliers.is_empty(),
            "{} unexpected high-ISO outliers, first positions: {:?}",
            unexpected_outliers.len(),
            &unexpected_outliers[..unexpected_outliers.len().min(30)]
        );
        assert_eq!(over_256_by_channel[0], 0, "unexpected red cutoff witness");
        let green_limit = if filtered_cutoff_witness { 10 } else { 7 };
        assert!(
            over_256_by_channel[1] <= green_limit,
            "green cutoff witness grew"
        );
        assert!(over_256_by_channel[2] <= 48, "blue cutoff witness grew");
        assert!(
            over_256 <= 48 + green_limit,
            "recorded cutoff witness grew beyond its measured patch"
        );
        assert!(maximum <= 966, "recorded cutoff witness maximum code error");
        if filtered_cutoff_witness {
            assert!(
                different * 100 < count * 3,
                "filtered high-ISO exact component fraction"
            );
            assert!(
                (sum as f64 / count as f64) < 0.4,
                "filtered high-ISO mean code error"
            );
        } else {
            assert!(different * 200 < count, "high-ISO exact component fraction");
            assert!(
                (sum as f64 / count as f64) < 0.05,
                "high-ISO mean code error"
            );
        }
    } else if settings.color_mode == "Portrait"
        || settings
            .reconstruction
            .is_some_and(|controls| controls.luminance > 0)
    {
        // Parallel blue filtering avoids the reference histogram's accumulated
        // Float32 history. Its measured nonzero-noise route needs its own bounds.
        assert!(maximum < 256, "filtered photograph maximum code error");
        assert!(
            different * 100 < count * 3,
            "filtered photograph exact component fraction"
        );
        assert!(
            (sum as f64 / count as f64) < 0.4,
            "filtered photograph mean code error"
        );
    } else {
        assert!(maximum < 256, "complete photograph maximum code error");
        assert!(
            different * 100 < count,
            "complete photograph exact component fraction"
        );
        assert!(
            (sum as f64 / count as f64) < 0.1,
            "complete photograph mean code error"
        );
    }
}

#[test]
#[ignore = "requires QUATTRO_X3F and QUATTRO_PREPROCESS_BOUNDARY native full planes"]
fn composed_preprocess_matches_native_camera() {
    let mut source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    Arc::make_mut(&mut source.metadata)
        .properties
        .insert("APERTURE".into(), "4".into());
    let reference =
        std::path::PathBuf::from(std::env::var_os("QUATTRO_PREPROCESS_BOUNDARY").unwrap());
    let active = source
        .metadata
        .fixed::<4>("ActiveImageArea")
        .unwrap()
        .map(|v| v as u32);
    let settings = diagnostic_settings();
    let outputs = crate::inspection::image_outputs(&settings, Some(&source.metadata));
    let blue = if outputs
        .iter()
        .any(|output| output.name == "blue-mismatch.blue")
    {
        "blue-mismatch.blue"
    } else {
        "luma-clusters.blue"
    };
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source);
    let noise_reference = std::env::var_os("QUATTRO_STAGE1_BOUNDARY").map(std::path::PathBuf::from);
    let mut names = vec!["highlight-restoration.rgb", blue];
    if noise_reference.is_some() {
        names.extend(["noise.rgb", "noise.source-after"]);
    }
    let results = engine
        .render_session(
            &mut session,
            &settings,
            &names,
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    let mut within_bounds = true;
    for (name, result) in names.into_iter().zip(&results) {
        let Content::Image(image) = &result.content else {
            panic!("expected camera plane")
        };
        let blue = name.ends_with(".blue");
        let (filename, directory) = match name {
            "noise.rgb" => ("output-rgb.f32", noise_reference.as_ref().unwrap()),
            "noise.source-after" => ("source-after-rgb.f32", noise_reference.as_ref().unwrap()),
            _ => (if blue { "blue.f32" } else { "rgb.f32" }, &reference),
        };
        let actual = engine.readback_image(image).unwrap();
        if let Some(output) = std::env::var_os("QUATTRO_COMPOSED_PREPROCESS") {
            std::fs::create_dir_all(&output).unwrap();
            std::fs::write(std::path::Path::new(&output).join(filename), &actual).unwrap();
        }
        let expected = std::fs::read(directory.join(filename)).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&actual);
        let expected: &[f32] = bytemuck::cast_slice(&expected);
        assert_eq!(actual.len(), expected.len());
        // Sensor-layer inspection retains physical margins. Compare the
        // photographic crop, independently of that inspection view.
        let crop = [active[0], active[1], active[2] + 1, active[3] + 1];
        let [left, top, right, bottom] = if blue { crop } else { crop.map(|v| v / 2) };
        let width = image.description().extent[0] as usize;
        let channels = if blue { 1 } else { 3 };
        let mut count = 0;
        let mut different = 0;
        let mut maximum = 0_f32;
        let mut sum = 0_f64;
        for y in top..bottom {
            for x in left..right {
                for channel in 0..channels {
                    let i = (y as usize * width + x as usize) * channels + channel;
                    assert!(actual[i].is_finite() && expected[i].is_finite());
                    let error = (actual[i] - expected[i]).abs();
                    maximum = maximum.max(error);
                    sum += f64::from(error);
                    count += 1;
                    different += usize::from(error != 0.);
                }
            }
        }
        eprintln!(
            "{name}: {count} active components, {different} different, max{maximum}, mean{}",
            sum / count as f64
        );
        let maximum_bound = if name == "noise.rgb" { 6. } else { 0.004 };
        within_bounds &= maximum < maximum_bound && sum / (count as f64) < 0.0001;
    }
    assert!(within_bounds, "preprocessing camera bounds");
}

#[test]
#[ignore = "requires QUATTRO_X3F and a GPU"]
fn photographic_edits_preserve_reconstruction_and_cached_defaults() {
    use std::hash::{Hash, Hasher};
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source);
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.output_size = Some([678, 452]);
    let mut render = |settings: &Settings| {
        let results = engine
            .render_session(
                &mut session,
                settings,
                &["final", "reconstruction.rgb"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        let Content::Image(final_image) = &results[0].content else {
            panic!("expected photograph")
        };
        let Content::Image(reconstruction) = &results[1].content else {
            panic!("expected reconstruction")
        };
        assert!(final_image.description().view_baked);
        assert_eq!(final_image.view().rotation, 0);
        let bytes = engine.readback_image(reconstruction).unwrap();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hash);
        (engine.readback_image(final_image).unwrap(), hash.finish())
    };
    let baseline = render(&settings);
    for change in 0..5 {
        let mut edited = settings.clone();
        match change {
            0 => edited.exposure = 0.3,
            1 => edited.tone.contrast = 0.5,
            2 => edited.tone.shadows = 0.5,
            3 => edited.tone.highlights = -0.5,
            _ => edited.saturation = 0.,
        }
        let result = render(&edited);
        assert_eq!(
            result.1, baseline.1,
            "photographic edit changed reconstruction"
        );
        assert_ne!(result.0, baseline.0, "photographic edit had no effect");
        let pixels: &[f32] = bytemuck::cast_slice(&result.0);
        assert!(pixels.iter().all(|v| v.is_finite()));
        if change == 4 {
            for rgb in pixels.as_chunks::<3>().0 {
                assert!((rgb[0] - rgb[1]).abs() < 0.000001);
                assert!((rgb[1] - rgb[2]).abs() < 0.000001);
            }
        }
    }
    assert_eq!(
        render(&settings),
        baseline,
        "restoring defaults changed cached output"
    );
}

#[test]
#[ignore = "requires a DP Quattro QUATTRO_X3F and a GPU"]
fn selected_recorded_balance_keeps_identity_shading_inspectable() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let balance = source.metadata.recorded_white_balance().unwrap();
    let settings = Settings::new(balance, "Standard");
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source);
    let results = engine
        .render_session(
            &mut session,
            &settings,
            &[
                "noise.rgb",
                "shading.rgb",
                "noise.source-after",
                "shading.source-after",
            ],
            || false,
            &mut Profile::default(),
        )
        .unwrap()
        .unwrap();
    let bytes: Vec<_> = results
        .iter()
        .map(|artifact| {
            let Content::Image(image) = &artifact.content else {
                panic!("expected image")
            };
            engine.readback_image(image).unwrap()
        })
        .collect();
    assert_eq!(bytes[0], bytes[1]);
    assert_eq!(bytes[2], bytes[3]);
}

#[test]
#[ignore = "requires QUATTRO_X3F and a GPU"]
fn every_camera_white_balance_renders_and_restores_defaults() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let balances: Vec<String> = source
        .metadata
        .white_balance_presets()
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert!(!balances.is_empty());
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source);
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.output_size = Some([339, 226]);
    let mut render = |settings: &Settings| {
        let results = engine
            .render_session(
                &mut session,
                settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        let Content::Image(image) = &results[0].content else {
            panic!("expected photograph")
        };
        let bytes = engine.readback_image(image).unwrap();
        let values: &[f32] = bytemuck::cast_slice(&bytes);
        assert!(
            values
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "{}",
            settings.white_balance
        );
        bytes
    };
    let baseline = render(&settings);
    for balance in balances {
        settings.white_balance = balance;
        let result = render(&settings);
        if settings.white_balance == "Shade" {
            assert_ne!(result, baseline);
        }
        eprintln!("rendered {}", settings.white_balance);
    }
    for mode in ["Vivid", "Neutral"] {
        settings.color_mode = mode.into();
        settings.white_balance = "Sunlight".into();
        assert_ne!(render(&settings), baseline);
        settings.white_balance = "Shade".into();
        render(&settings);
        eprintln!("rendered {mode} with Sunlight and Shade");
    }
    settings.color_mode = "Standard".into();
    settings.white_balance = "Sunlight".into();
    assert_eq!(render(&settings), baseline);
}

#[test]
#[ignore = "requires QUATTRO_X3F and a GPU"]
fn noise_and_detail_keep_factory_corrections_and_exposure_independent() {
    use std::hash::{Hash, Hasher};
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let mut session = RenderSession::new(source);
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.output_size = Some([678, 452]);
    let mut render = |settings: &Settings| {
        let results = engine
            .render_session(
                &mut session,
                settings,
                &["final", "highlight-restoration.rgb", "noise.rgb"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap();
        let bytes: Vec<_> = results
            .iter()
            .map(|artifact| {
                let Content::Image(image) = &artifact.content else {
                    panic!("expected image")
                };
                engine.readback_image(image).unwrap()
            })
            .collect();
        let fingerprints: Vec<_> = bytes[1..]
            .iter()
            .map(|bytes| {
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                bytes.hash(&mut hash);
                hash.finish()
            })
            .collect();
        let values: &[f32] = bytemuck::cast_slice(&bytes[0]);
        assert!(values
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v)));
        let mean = values.iter().map(|v| f64::from(*v)).sum::<f64>() / values.len() as f64;
        (bytes[0].clone(), fingerprints, mean)
    };
    let baseline = render(&settings);
    for (luminance, detail) in [(0, 0.), (2, 1.), (4, 1.)] {
        settings.reconstruction = Some(crate::noise::Controls {
            luminance,
            detail,
            ..super::recipes::quattro::default_noise_controls()
        });
        let result = render(&settings);
        assert_eq!(
            result.1[0], baseline.1[0],
            "edit changed factory corrections"
        );
        assert_eq!(
            result.1[1], baseline.1[1],
            "photographic edit changed technical filtering"
        );
        assert_ne!(result.0, baseline.0, "edit had no photographic effect");
        let ratio = result.2 / baseline.2;
        eprintln!("luma {luminance} detail {detail}: mean brightness ratio {ratio}");
        assert!(
            (0.9..=1.1).contains(&ratio),
            "noise/detail changed global brightness"
        );
    }
    for chroma in [2, 4] {
        settings.reconstruction = Some(crate::noise::Controls {
            chroma,
            ..super::recipes::quattro::default_noise_controls()
        });
        let result = render(&settings);
        assert_eq!(
            result.1[0], baseline.1[0],
            "chroma changed factory corrections"
        );
        assert_ne!(
            result.1[1], baseline.1[1],
            "chroma failed to change the RGB filter"
        );
        assert_ne!(result.0, baseline.0, "chroma had no photographic effect");
        assert!(
            (0.9..=1.1).contains(&(result.2 / baseline.2)),
            "chroma changed exposure"
        );
    }
    settings.reconstruction = None;
    settings.sharpen_amount = Some(1.);
    let sharpened = render(&settings);
    assert_eq!(
        sharpened.1, baseline.1,
        "sharpness changed sensor or noise correction"
    );
    assert_ne!(
        sharpened.0, baseline.0,
        "sharpness had no photographic effect"
    );
    settings.sharpen_amount = Some(0.);
    settings.reconstruction = None;
    assert_eq!(render(&settings), baseline);
}

#[test]
#[ignore = "requires QUATTRO_X3F and a GPU"]
fn missing_white_balance_uses_the_recorded_file_choice() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let recorded = source.metadata.recorded_white_balance().unwrap();
    let engine = Engine::new().unwrap();
    let mut automatic = Settings::new("", "Standard");
    automatic.output_size = Some([678, 452]);
    let mut explicit = automatic.clone();
    explicit.white_balance = recorded.clone();
    let render = |settings: &Settings| {
        let artifacts = engine
            .render_artifacts(&source, settings, &["final"], &mut Profile::default())
            .unwrap();
        let Content::Image(image) = &artifacts[0].content else {
            panic!("expected photograph")
        };
        engine.readback_image(image).unwrap()
    };
    assert_eq!(render(&automatic), render(&explicit));
    explicit.white_balance = "Sunlight".into();
    let manual = render(&explicit);
    assert_eq!(manual, render(&explicit));
    if recorded != "Sunlight" {
        assert_ne!(manual, render(&automatic));
    }
}

#[test]
#[ignore = "requires a GPU"]
fn preparation_can_stop_before_later_calibration() {
    let engine = Engine::new().unwrap();
    let source = source();
    let checks = std::cell::Cell::new(0);
    let cancel = || {
        checks.set(checks.get() + 1);
        checks.get() >= 2
    };
    let mut session = RenderSession::new(source);
    let before = engine.gpu_activity();
    let result = engine
        .render_session(
            &mut session,
            &Settings::new("", ""),
            &["extraction.rgb"],
            cancel,
            &mut Profile::default(),
        )
        .expect("stop before unrelated calibration is required");
    assert!(result.is_none());
    assert_eq!(checks.get(), 2);
    assert_eq!(engine.gpu_activity().since(before).dispatches, 0);
}

#[test]
#[ignore = "requires GPU, FOVEONIC_FIXTURE_DIR and optional QUATTRO_FULL_GOLDENS"]
fn original_fast_previews_match_fresh_renders_through_quality_and_edit_changes() {
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::new().unwrap();
    for name in [
        "dp0-quattro",
        "dp1-quattro-pblog",
        "dp2-quattro",
        "dp3-quattro",
        "sd-quattro",
        "dp3-quattro-iso800",
    ] {
        let mut session =
            RenderSession::new(Source::open(root.join(format!("{name}.x3f"))).unwrap());
        let mut full = Settings::new("Sunlight", "Standard");
        // Full must still match the committed complete image on every source.
        let reference = engine
            .render(session.source(), &full, &mut Profile::default())
            .unwrap();
        let full_bytes = engine.readback_image(&reference).unwrap();
        if let Some(goldens) = std::env::var_os("QUATTRO_FULL_GOLDENS") {
            let path = std::path::PathBuf::from(goldens)
                .join(format!("{name}-color-baseline-floats/{name}.f32"));
            assert!(
                full_bytes == std::fs::read(path).unwrap(),
                "Full baseline {name}"
            );
        }
        let native = reference.view().extent;
        drop(reference);
        let mut retained = None;
        let mut retained_bytes = Vec::new();
        for step in 0..6 {
            full.preview_quality = if step == 0 || step == 4 {
                PreviewQuality::Full
            } else {
                PreviewQuality::Fast
            };
            full.exposure = if step == 2 || step == 4 { 0.5 } else { 0. };
            full.reconstruction = if step == 3 {
                Some(crate::noise::Controls {
                    chroma: 1,
                    luminance: 4,
                    detail: 0.25,
                    ..recipes::quattro::default_noise_controls()
                })
            } else {
                None
            };
            let cached = engine
                .render_session(
                    &mut session,
                    &full,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap()
                .unwrap()
                .pop()
                .unwrap();
            let Content::Image(cached) = cached.content else {
                panic!("image required")
            };
            let fresh = engine
                .render(session.source(), &full, &mut Profile::default())
                .unwrap();
            assert!(
                engine.readback_image(&cached).unwrap() == engine.readback_image(&fresh).unwrap(),
                "cached {name} step {step}"
            );
            assert_eq!(
                cached.view().extent,
                if full.preview_quality == PreviewQuality::Fast {
                    native.map(|v| v / 2)
                } else {
                    native
                }
            );
            if step == 1 {
                retained_bytes = engine.readback_image(&cached).unwrap();
                retained = Some(cached.clone());
            }
            if let Some(retained) = &retained {
                assert!(
                    engine.readback_image(retained).unwrap() == retained_bytes,
                    "retained {name}"
                );
            }
        }
        eprintln!(
            "{name}: Full golden, Fast cached edits, quality switches and immutable image passed"
        );
    }
}

#[test]
fn supported_preview_policy_survives_photographic_control_normalization() {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.preview_quality = PreviewQuality::Fast;
    assert!(!recipes::quattro::normalize_photographic(&mut settings));
    assert!(recipes::quattro::validate_photographic(&settings).is_ok());
}

#[test]
#[ignore = "requires GPU and QUATTRO_X3F"]
fn progress_observers_report_real_load_and_render_boundaries_without_changing_pixels() {
    use std::cell::RefCell;
    let loads = RefCell::new(Vec::new());
    let source = Source::open_with_progress(std::env::var_os("QUATTRO_X3F").unwrap(), |stage| {
        loads.borrow_mut().push(stage)
    })
    .unwrap();
    assert_eq!(
        *loads.borrow(),
        [
            crate::source::LoadStage::FileRead,
            crate::source::LoadStage::Decoded
        ]
    );
    let mut session = RenderSession::new(source);
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    for (quality, exposure) in [
        (PreviewQuality::Full, 0.),
        (PreviewQuality::Fast, 0.),
        (PreviewQuality::Fast, 0.5),
    ] {
        settings.preview_quality = quality;
        settings.exposure = exposure;
        let stages = RefCell::new(Vec::new());
        let mut artifacts = engine
            .render_session_with_progress(
                &mut session,
                &settings,
                &["final"],
                || false,
                &mut Profile::default(),
                |stage| stages.borrow_mut().push(stage),
            )
            .unwrap()
            .unwrap();
        let Content::Image(observed) = artifacts.pop().unwrap().content else {
            panic!("image required")
        };
        let fresh = engine
            .render(session.source(), &settings, &mut Profile::default())
            .unwrap();
        assert!(
            engine.readback_image(&observed).unwrap() == engine.readback_image(&fresh).unwrap()
        );
        let stages = stages.into_inner();
        assert_eq!(stages[0], RenderProgress::Prepared);
        let counts: Vec<_> = stages[1..]
            .iter()
            .map(|stage| {
                let RenderProgress::Stage { completed, total } = stage else {
                    panic!("unexpected preparation event")
                };
                (*completed, *total)
            })
            .collect();
        let total = counts.last().unwrap().1;
        if total == 0 {
            assert_eq!(counts, [(0, 0)]);
        } else {
            assert_eq!(counts, (1..=total).map(|i| (i, total)).collect::<Vec<_>>());
        }
        eprintln!("{quality:?} exposure={exposure}: {total} completed stages, pixels unchanged");
    }
}
