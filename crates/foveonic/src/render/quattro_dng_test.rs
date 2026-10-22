//! Checks Quattro DNG separation, retained source ownership and native oriented samples.

use super::*;
use crate::{dng, image::Space, source::Source};

fn original() -> Source {
    Source::open(std::env::var_os("QUATTRO_X3F").expect("QUATTRO_X3F")).unwrap()
}

fn controls(source: &Source) -> Settings {
    let mut settings = Settings::default();
    ControlCapabilities::from_metadata(&source.metadata)
        .unwrap()
        .apply_capture_defaults(&mut settings, &source.metadata)
        .unwrap();
    settings.white_balance = "Sunlight".into();
    settings.color_mode = "Standard".into();
    settings.exposure = 0.;
    settings.tone = ToneControls::default();
    settings.saturation = 1.;
    settings.sharpen_amount = Some(0.);
    settings
}

fn samples(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
) -> (crate::image::RenderedImage, Vec<u8>) {
    let image = dng::prepare_base(engine, source, settings, &mut Default::default()).unwrap();
    let bytes = engine.gpu.readback(image.buffer()).unwrap();
    (image, bytes)
}

#[test]
#[ignore = "requires GPU and QUATTRO_X3F; optional QUATTRO_DNG_REPORT retains boundary statistics"]
fn quattro_dng_base_separates_appearance_and_preserves_technical_controls() {
    let source = original();
    let engine = Engine::new().unwrap();
    let settings = controls(&source);
    let (base, expected) = samples(&engine, &source, &settings);
    assert_eq!(base.description().space, Space::LinearCodeRgb);
    assert!(base.description().view_baked);
    let params = crate::nodes::sensor::calibration::encode(
        &source.metadata,
        crate::color_space::ColorSpace::Srgb,
    )
    .unwrap();
    let extent = [params.output[0], params.output[1]];
    assert_eq!(base.description().extent, extent);
    assert_eq!(base.view().extent, extent);
    assert_eq!(base.view().crop, [0, 0, extent[0], extent[1]]);
    assert_eq!(base.view().rotation, 0);
    assert_eq!(expected.len(), extent[0] as usize * extent[1] as usize * 12);
    assert!(Arc::ptr_eq(&base.metadata, &source.metadata));

    let mut look = settings.clone();
    look.color_mode = "Vivid".into();
    look.exposure = 0.75;
    look.tone = ToneControls {
        contrast: 0.3,
        shadows: 0.4,
        highlights: -0.3,
    };
    look.saturation = 0.5;
    assert_eq!(samples(&engine, &source, &look).1, expected);
    let original_profile = dng::appearance::Prepared::new(&source.metadata, &settings).unwrap();
    let edited_profile = dng::appearance::Prepared::new(&source.metadata, &look).unwrap();
    let original_packed = engine
        .encode_dng_base(&base, &original_profile, &mut Default::default())
        .unwrap();
    let edited_packed = engine
        .encode_dng_base(&base, &edited_profile, &mut Default::default())
        .unwrap();
    assert_eq!(original_packed, edited_packed);
    drop((original_packed, edited_packed));

    let mut wb = settings.clone();
    wb.white_balance = "Incandescent".into();
    assert_ne!(samples(&engine, &source, &wb).1, expected);
    let mut noise = settings.clone();
    let mut noise_controls = noise
        .reconstruction
        .unwrap_or_else(recipes::quattro::default_noise_controls);
    noise_controls.chroma = 0;
    noise_controls.luminance = 0;
    noise.reconstruction = Some(noise_controls);
    assert_ne!(samples(&engine, &source, &noise).1, expected);
    let mut smooth = settings.clone();
    let mut smooth_controls = smooth
        .reconstruction
        .unwrap_or_else(recipes::quattro::default_noise_controls);
    smooth_controls.detail = 0.;
    smooth.reconstruction = Some(smooth_controls);
    assert_ne!(samples(&engine, &source, &smooth).1, expected);
    let mut sharp = settings.clone();
    sharp.sharpen_amount = Some(1.);
    assert_ne!(samples(&engine, &source, &sharp).1, expected);
    assert_eq!(engine.gpu.readback(base.buffer()).unwrap(), expected);

    let prepared = dng::appearance::Prepared::new(&source.metadata, &settings).unwrap();
    let other = Engine::new().unwrap();
    assert_eq!(
        other
            .encode_dng_base(&base, &prepared, &mut Default::default())
            .unwrap_err()
            .to_string(),
        "image belongs to another engine"
    );

    let values: &[f32] = bytemuck::cast_slice(&expected);
    let report = serde_json::json!({
        "camera": source.metadata.properties.get("CAMMODEL"),
        "extent": extent,
        "source_rotation": source.metadata.rotation,
        "components": values.len(),
        "nominal_white": 16384,
        "minimum": values.iter().copied().fold(f32::INFINITY, f32::min),
        "maximum": values.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        "zero_components": values.iter().filter(|v| **v == 0.).count(),
        "above_nominal_white": values.iter().filter(|v| **v > 16384.).count(),
        "at_unsigned_limit": values.iter().filter(|v| **v >= 65534.).count(),
    });
    assert!(values
        .iter()
        .all(|v| v.is_finite() && (0. ..=65535.).contains(v)));
    eprintln!("{report}");
    if let Some(path) = std::env::var_os("QUATTRO_DNG_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}

#[test]
#[ignore = "requires GPU and QUATTRO_X3F"]
fn quattro_monochrome_dng_freezes_mix_and_defers_inactive_appearance() {
    let source = original();
    let engine = Engine::new().unwrap();
    let mut settings = controls(&source);
    settings.rendering = Rendering::Monochrome;
    let frozen_mix = settings.monochrome.mix;
    let (base, expected) = samples(&engine, &source, &settings);
    assert_eq!(base.settings().monochrome.mix, frozen_mix);
    let values: &[f32] = bytemuck::cast_slice(&expected);
    assert!(values
        .as_chunks::<3>()
        .0
        .iter()
        .all(|v| v[0] == v[1] && v[1] == v[2]));
    let mut look = settings.clone();
    look.white_balance = "Incandescent".into();
    look.color_mode = "Vivid".into();
    look.exposure = -0.5;
    look.tone = ToneControls {
        contrast: 0.3,
        shadows: 0.4,
        highlights: -0.3,
    };
    assert_eq!(samples(&engine, &source, &look).1, expected);
    let original_profile = dng::appearance::Prepared::new(&source.metadata, &settings).unwrap();
    let edited_profile = dng::appearance::Prepared::new(&source.metadata, &look).unwrap();
    assert_eq!(
        engine
            .encode_dng_base(&base, &original_profile, &mut Default::default())
            .unwrap(),
        engine
            .encode_dng_base(&base, &edited_profile, &mut Default::default())
            .unwrap()
    );
    let mut mix = settings.clone();
    mix.monochrome.mix = [0., 0., 1.];
    assert_ne!(samples(&engine, &source, &mix).1, expected);
    let mut noise = settings.clone();
    noise.monochrome.noise = 0;
    assert_ne!(samples(&engine, &source, &noise).1, expected);
    let mut smooth = settings.clone();
    smooth.monochrome_smoothness = Some(0.);
    assert_ne!(samples(&engine, &source, &smooth).1, expected);
    let mut sharp = settings.clone();
    sharp.sharpen_amount = Some(1.);
    assert_ne!(samples(&engine, &source, &sharp).1, expected);
    assert_eq!(engine.gpu.readback(base.buffer()).unwrap(), expected);
    assert_eq!(base.settings().monochrome.mix, frozen_mix);
}

#[test]
#[ignore = "requires GPU, QUATTRO_X3F and QUATTRO_DNG_DIR; retains DNG/TIFF pairs for independent SDK validation"]
fn quattro_dng_exports_native_oriented_review_pairs() {
    let source = original();
    let engine = Engine::new().unwrap();
    let root = std::path::PathBuf::from(std::env::var_os("QUATTRO_DNG_DIR").unwrap());
    std::fs::create_dir_all(&root).unwrap();
    let selected = std::env::var("QUATTRO_DNG_CASES")
        .unwrap_or_else(|_| "standard,adjusted,monochrome".into());
    let base = controls(&source);
    let name = std::path::PathBuf::from(std::env::var_os("QUATTRO_X3F").unwrap())
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    for case in selected.split(',') {
        let mut settings = base.clone();
        settings.tiff_color_space = Some(TiffColorSpace::Srgb);
        match case {
            "standard" => {}
            "recorded" => {
                settings = Settings::default();
                let capabilities = ControlCapabilities::from_metadata(&source.metadata).unwrap();
                settings.rendering = capabilities.default_rendering(&source.metadata).unwrap();
                settings.white_balance = capabilities
                    .default_white_balance(&source.metadata)
                    .unwrap();
                settings.color_mode = capabilities.default_color_mode(&source.metadata).unwrap();
                capabilities
                    .apply_capture_defaults(&mut settings, &source.metadata)
                    .unwrap();
                settings.tiff_color_space = Some(TiffColorSpace::Srgb);
            }
            "adjusted" => {
                settings.exposure = 0.75;
                settings.tone = ToneControls {
                    contrast: 0.3,
                    shadows: 0.4,
                    highlights: -0.3,
                };
                settings.saturation = 0.5;
            }
            "vivid" => {
                settings.color_mode = "Vivid".into();
                settings.exposure = 0.5;
            }
            "portrait" => settings.color_mode = "Portrait".into(),
            "fcblue" => settings.color_mode = "FCBlue".into(),
            "monochrome" => {
                settings.rendering = Rendering::Monochrome;
            }
            _ => panic!("unknown DNG review case"),
        }
        let stem = format!("{name}-{case}");
        let dng_path = root.join(format!("{stem}.dng"));
        dng::write_source(
            &engine,
            &source,
            &settings,
            &dng_path,
            &mut Default::default(),
        )
        .unwrap();
        let photograph = engine
            .render(&source, &settings, &mut Default::default())
            .unwrap();
        let tiff_path = root.join(format!("{stem}.tiff"));
        let dimensions = engine
            .write_tiff(&photograph, &tiff_path, &mut Default::default())
            .unwrap();
        assert_eq!(dimensions, photograph.view().extent);
        std::fs::write(
            root.join(format!("{stem}-settings.json")),
            serde_json::to_vec_pretty(&settings).unwrap(),
        )
        .unwrap();
        eprintln!("{} {}x{}", dng_path.display(), dimensions[0], dimensions[1]);
    }
}
