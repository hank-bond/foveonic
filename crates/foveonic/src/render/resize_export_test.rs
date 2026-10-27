//! Checks resized TIFF exports against reference samples without changing retained working images.

use super::*;
use crate::image::{ColorSpace, View};

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "foveonic-resize-{}-{}",
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

fn encode(value: f64, destination: TiffColorSpace) -> u16 {
    let v = value.clamp(0., 1.);
    let v = match destination {
        TiffColorSpace::Srgb if v <= 0.0031308 => v * 12.92,
        TiffColorSpace::Srgb => 1.055 * v.powf(1. / 2.4) - 0.055,
        TiffColorSpace::AdobeRgb => v.powf(1. / 2.2),
        TiffColorSpace::ProPhotoRgb if v < 1. / 512. => v * 16.,
        TiffColorSpace::ProPhotoRgb => v.powf(1. / 1.8),
    };
    (v * 65535. + 0.5).floor() as u16
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn resized_tiff_defaults_to_reference_sampling_before_destination_conversion() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/data/resize-reference.json")).unwrap();
    let floats = |v: &serde_json::Value| -> Vec<f32> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect()
    };
    let pixels = floats(&fixture["input"]);
    let engine = Engine::new().unwrap();
    let directory = Directory::new();
    let metadata = Arc::new(crate::source::Metadata {
        processing_version: None,
        width: 12,
        height: 10,
        rotation: 0,
        properties: Default::default(),
        matrices: Default::default(),
        lists: Default::default(),
    });
    for space in [ColorSpace::Srgb, ColorSpace::AdobeRgb] {
        let mut image = RenderedImage {
            buffer: engine
                .gpu
                .upload("TIFF reference resize fixture", &pixels)
                .unwrap(),
            description: Description {
                linear_white: 4095,
                view_baked: false,
                extent: [12, 10],
                layout: Layout::Rgb32Float,
                space: Space::ToneMappedRgb(space),
            },
            view: View {
                crop: [3, 3, 9, 8],
                rotation: 0,
                extent: [6, 5],
            },
            metadata: metadata.clone(),
            settings: Settings::new("Sunlight", "Standard"),
            owner: engine.owner.clone(),
        };
        for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            image.view.rotation = case["rotation"].as_u64().unwrap() as u32;
            image.view.extent = std::array::from_fn(|i| case["target"][i].as_u64().unwrap() as u32);
            let expected = floats(&case["pixels"]);
            for destination in TiffColorSpace::ALL {
                image.settings.tiff_color_space = Some(destination);
                let path = directory
                    .0
                    .join(format!("{space:?}-{index}-{destination:?}.tiff"));
                let activity = engine.gpu_activity();
                let size = engine
                    .write_tiff(&image, &path, &mut Profile::default())
                    .unwrap();
                let activity = engine.gpu_activity().since(activity);
                assert_eq!(activity.readbacks, 1);
                assert_eq!(size, image.view.extent);
                let mut decoder =
                    tiff::decoder::Decoder::new(std::fs::File::open(&path).unwrap()).unwrap();
                assert_eq!(decoder.dimensions().unwrap(), (size[0], size[1]));
                assert_eq!(
                    decoder
                        .get_tag_u8_vec(tiff::tags::Tag::Unknown(34675))
                        .unwrap(),
                    destination.profile()
                );
                let tiff::decoder::DecodingResult::U16(actual) = decoder.read_image().unwrap()
                else {
                    panic!("expected RGB16");
                };
                let transform = destination.transform(space);
                for (p, rgb) in expected.as_chunks::<3>().0.iter().enumerate() {
                    for c in 0..3 {
                        let value = (0..3)
                            .map(|k| f64::from(transform[c][k]) * f64::from(rgb[k]))
                            .sum();
                        let code = encode(value, destination);
                        assert!(actual[p*3+c].abs_diff(code) <= 3, "{space:?} {destination:?} case {index} pixel {p} channel {c}: {} vs {code}",actual[p*3+c]);
                    }
                }
                assert_eq!(
                    engine.readback_image(&image).unwrap(),
                    bytemuck::cast_slice::<f32, u8>(&pixels)
                );
                assert_eq!(image.description.space, Space::ToneMappedRgb(space));
            }
        }
        image.view = View {
            crop: [0, 0, 12, 10],
            rotation: 0,
            extent: [3, 2],
        };
        image.settings.rendering = Rendering::Monochrome;
        image.buffer = engine
            .gpu
            .upload("Neutral mono resize", &vec![0.25_f32; 12 * 10 * 3])
            .unwrap();
        for destination in TiffColorSpace::ALL {
            image.settings.tiff_color_space = Some(destination);
            let path = directory
                .0
                .join(format!("mono-{space:?}-{destination:?}.tiff"));
            engine
                .write_tiff(&image, &path, &mut Profile::default())
                .unwrap();
            let mut decoder =
                tiff::decoder::Decoder::new(std::fs::File::open(path).unwrap()).unwrap();
            let tiff::decoder::DecodingResult::U16(actual) = decoder.read_image().unwrap() else {
                panic!("expected RGB16");
            };
            for rgb in actual.as_chunks::<3>().0 {
                assert_eq!(rgb[0], rgb[1]);
                assert_eq!(rgb[1], rgb[2]);
                assert!(rgb[0].abs_diff(encode(0.25, destination)) <= 2);
            }
        }
        // Reject invalid geometry before creating a destination file.
        image.view.extent = [0, 2];
        let path = directory.0.join(format!("invalid-{space:?}.tiff"));
        assert!(engine
            .write_tiff(&image, &path, &mut Profile::default())
            .is_err());
        assert!(!path.exists());
    }
}
