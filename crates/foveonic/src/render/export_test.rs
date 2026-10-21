//! Checks TIFF-only color conversion and automatic/manual sharpening equivalence.

use super::*;
use crate::{image::ColorSpace, inspection::Content};

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "foveonic-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn tiff_destinations_convert_pixels_match_profiles_and_preserve_working_image() {
    let engine = Engine::new().unwrap();
    let directory = Directory::new();
    let pixels = [
        [0_f32; 3],
        [1.; 3],
        [0.25; 3],
        [0.0001; 3],
        [0.001953125; 3],
        [0.004; 3],
        [1., 0., 0.],
        [0., 0., 1.],
        [-0.1, 0.2, 1.3],
        [0.1, 0.3, 0.7],
    ]
    .concat();
    let metadata = Arc::new(crate::source::Metadata {
        width: 10,
        height: 1,
        rotation: 0,
        properties: Default::default(),
        matrices: Default::default(),
        lists: Default::default(),
    });
    let encode = |value: f64, destination| {
        let value = value.clamp(0., 1.);
        let encoded = match destination {
            TiffColorSpace::Srgb if value <= 0.0031308 => value * 12.92,
            TiffColorSpace::Srgb => 1.055 * value.powf(1. / 2.4) - 0.055,
            TiffColorSpace::AdobeRgb => value.powf(1. / 2.2),
            TiffColorSpace::ProPhotoRgb if value < 1. / 512. => value * 16.,
            TiffColorSpace::ProPhotoRgb => value.powf(1. / 1.8),
        };
        (encoded * 65535. + 0.5).floor() as u16
    };
    for source in [
        ColorSpace::Srgb,
        ColorSpace::AdobeRgb,
        ColorSpace::ProPhotoRgb,
    ] {
        let mut image = RenderedImage {
            buffer: engine.gpu.upload("Destination fixture", &pixels).unwrap(),
            description: Description {
                extent: [10, 1],
                layout: Layout::Rgb32Float,
                space: Space::ToneMappedRgb(source),
            },
            view: crate::image::View {
                crop: [0, 0, 10, 1],
                rotation: 0,
                extent: [10, 1],
            },
            metadata: metadata.clone(),
            settings: Settings::new("Sunlight", "Standard"),
            owner: engine.owner.clone(),
        };
        let texture = engine
            .view_texture(&image, None, &mut Profile::default())
            .unwrap();
        let preview_before = engine
            .gpu
            .readback_texture(texture.texture(), [10, 1], 256)
            .unwrap();
        let mut identity_bytes = None;
        let mut distinct = Vec::new();
        for (index, destination) in [
            None,
            Some(TiffColorSpace::Srgb),
            Some(TiffColorSpace::AdobeRgb),
            Some(TiffColorSpace::ProPhotoRgb),
        ]
        .into_iter()
        .enumerate()
        {
            image.settings.tiff_color_space = destination;
            let resolved = destination.unwrap_or(TiffColorSpace::Srgb);
            let path = directory.0.join(format!("{source:?}-{index}.tiff"));
            engine
                .write_tiff(&image, &path, &mut Profile::default())
                .unwrap();
            let bytes = std::fs::read(&path).unwrap();
            if destination.is_none() {
                identity_bytes = Some(bytes.clone());
            }
            if destination == Some(TiffColorSpace::Srgb) {
                assert_eq!(Some(&bytes), identity_bytes.as_ref());
            }
            let mut decoder = tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)).unwrap();
            assert_eq!(
                decoder
                    .get_tag_u8_vec(tiff::tags::Tag::Unknown(34675))
                    .unwrap(),
                resolved.profile()
            );
            let tiff::decoder::DecodingResult::U16(actual) = decoder.read_image().unwrap() else {
                panic!("expected RGB16");
            };
            let transform = resolved.transform(source);
            for (p, rgb) in pixels.as_chunks::<3>().0.iter().enumerate() {
                for c in 0..3 {
                    let converted: f64 = (0..3)
                        .map(|k| f64::from(transform[c][k]) * f64::from(rgb[k]))
                        .sum();
                    let expected = encode(converted, resolved);
                    assert!(
                        actual[p * 3 + c].abs_diff(expected) <= 2,
                        "{source:?} to {resolved:?} pixel {p} component {c}: {} vs {expected}",
                        actual[p * 3 + c]
                    );
                }
            }
            distinct.push(actual);
            assert_eq!(
                engine.readback_image(&image).unwrap(),
                bytemuck::cast_slice::<f32, u8>(&pixels)
            );
            assert_eq!(image.description.space, Space::ToneMappedRgb(source));
        }
        assert_ne!(distinct[1], distinct[2]);
        assert_ne!(distinct[2], distinct[3]);
        let texture = engine
            .view_texture(&image, None, &mut Profile::default())
            .unwrap();
        assert_eq!(
            engine
                .gpu
                .readback_texture(texture.texture(), [10, 1], 256)
                .unwrap(),
            preview_before
        );
        image.settings.rendering = Rendering::Monochrome;
        image.view.crop = [0, 0, 6, 1];
        image.view.extent = [6, 1];
        for destination in TiffColorSpace::ALL {
            image.settings.tiff_color_space = Some(destination);
            let path = directory
                .0
                .join(format!("mono-{source:?}-{destination:?}.tiff"));
            engine
                .write_tiff(&image, &path, &mut Profile::default())
                .unwrap();
            let mut decoder =
                tiff::decoder::Decoder::new(std::fs::File::open(path).unwrap()).unwrap();
            let tiff::decoder::DecodingResult::U16(actual) = decoder.read_image().unwrap() else {
                panic!("expected RGB16");
            };
            for (p, rgb) in actual.as_chunks::<3>().0.iter().enumerate() {
                assert_eq!(rgb[0], rgb[1]);
                assert_eq!(rgb[1], rgb[2]);
                assert!(rgb[0].abs_diff(encode(f64::from(pixels[p * 3]), destination)) <= 2);
            }
        }
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn zero_auto_strength_keeps_signed_rgb_and_headroom_unchanged() {
    let engine = Engine::new().unwrap();
    let pixels = [-10_f32, 0.5, 40000.].repeat(25);
    let image = RenderedImage {
        buffer: engine
            .gpu
            .upload("Zero sharpening fixture", &pixels)
            .unwrap(),
        description: Description {
            extent: [5, 5],
            layout: Layout::Rgb32Float,
            space: Space::LinearRgb(ColorSpace::Srgb),
        },
        view: crate::image::View {
            crop: [0, 0, 5, 5],
            rotation: 0,
            extent: [5, 5],
        },
        metadata: Arc::new(crate::source::Metadata {
            width: 5,
            height: 5,
            rotation: 0,
            properties: Default::default(),
            matrices: Default::default(),
            lists: Default::default(),
        }),
        settings: Settings::new("Sunlight", "Standard"),
        owner: engine.owner.clone(),
    };
    let mut graph = Graph::default();
    let input = graph
        .add(
            Operation::RenderedInput {
                data: image.description.space.into(),
            },
            &[],
        )
        .unwrap();
    let signal = graph
        .add(
            Operation::SharpSignal {
                space: ColorSpace::Srgb,
            },
            &[input],
        )
        .unwrap();
    let color = graph
        .add(
            Operation::Sharpen {
                amount: 0.,
                space: ColorSpace::Srgb,
            },
            &[input, signal],
        )
        .unwrap();
    let mono = graph
        .add(
            Operation::MonoSharpen {
                amount: 0.,
                space: ColorSpace::Srgb,
            },
            &[input, signal],
        )
        .unwrap();
    let outputs = engine
        .execute(
            Input::Rendered(&image),
            &graph,
            &[color, mono],
            &mut Profile::default(),
        )
        .unwrap();
    for output in outputs {
        assert_eq!(
            engine.gpu.readback(&output.buffer).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&pixels)
        );
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn auto_sharpen_matches_manual_for_color_mono_cached_and_dng_recipes() {
    let directory = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::new().unwrap();
    let mut session =
        RenderSession::new(Source::open(directory.join("blue-parking-sign.x3f")).unwrap());
    let render = |session: &mut RenderSession, settings: &Settings| {
        let artifact = engine
            .render_session(
                session,
                settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap()
            .pop()
            .unwrap();
        let Content::Image(image) = artifact.content else {
            panic!("expected image");
        };
        *image
    };
    for route in 0..3 {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.output_size = Some([64, 96]);
        settings.sharpen_amount = None;
        if route == 1 {
            settings.reconstruction = Some(Default::default());
        }
        if route == 2 {
            settings.rendering = Rendering::Monochrome;
        }
        for choice in [0, 2, 4] {
            if let Some(controls) = &mut settings.reconstruction {
                controls.luminance = choice;
            }
            settings.monochrome.noise = choice;
            let amount = settings
                .effective_sharpen_amount(&session.source.metadata)
                .unwrap();
            assert!(amount.is_finite() && amount >= 0.);
            let automatic = render(&mut session, &settings);
            let before = engine.readback_image(&automatic).unwrap();
            let mut manual = settings.clone();
            manual.sharpen_amount = Some(amount);
            let matched = render(&mut session, &manual);
            assert!(
                engine.readback_image(&matched).unwrap() == before,
                "Auto/manual differ route {route} preset {choice}"
            );
            drop(matched);
            settings.tiff_color_space = Some(TiffColorSpace::ProPhotoRgb);
            let export_choice = render(&mut session, &settings);
            assert!(
                engine.readback_image(&export_choice).unwrap() == before,
                "TIFF setting changed working pixels"
            );
            assert_eq!(
                export_choice.settings().tiff_color_space,
                settings.tiff_color_space
            );
            drop(export_choice);
            let fresh = engine
                .render(&session.source, &settings, &mut Profile::default())
                .unwrap();
            assert!(
                engine.readback_image(&fresh).unwrap() == before,
                "cached/fresh Auto differ"
            );
            drop(fresh);
            manual.sharpen_amount = Some(0.);
            let _off = render(&mut session, &manual);
            assert!(
                engine.readback_image(&automatic).unwrap() == before,
                "retained Auto was overwritten"
            );
        }
        let base = engine
            .render_dng_base(&session.source, &settings, &mut Profile::default())
            .unwrap();
        let before = engine.readback_image(&base).unwrap();
        let mut manual = settings.clone();
        manual.sharpen_amount = Some(
            settings
                .effective_sharpen_amount(&session.source.metadata)
                .unwrap(),
        );
        manual.tiff_color_space = Some(TiffColorSpace::Srgb);
        let matched = engine
            .render_dng_base(&session.source, &manual, &mut Profile::default())
            .unwrap();
        assert!(
            engine.readback_image(&matched).unwrap() == before,
            "DNG Auto/manual or export isolation failed route {route}"
        );
    }
}
