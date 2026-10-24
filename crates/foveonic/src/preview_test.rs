//! Exercises embedded-preview selection, bitmap layout, and bounded decoding.

use super::*;

fn embedded(format: u32, extent: [u32; 2], stride: usize) -> Embedded {
    Embedded {
        offset: 0,
        size: 0,
        format,
        extent,
        stride,
    }
}

#[test]
fn preview_reduction_averages_linear_light_without_brightening_floats() {
    let preview = Preview {
        rgb: vec![0, 0, 0, 65535, 65535, 65535],
        extent: [2, 1],
        native_extent: [2, 1],
        rotation: 0,
        color_space: crate::color_space::TiffColorSpace::Srgb,
    };
    let reduced = preview.resized(1).unwrap();
    for value in reduced.linear_image().unwrap().as_raw() {
        assert!((value - 0.5).abs() < 0.0001, "{value}");
    }
}

#[test]
fn bitmap_preserves_rgb_and_omits_row_padding() {
    let bitmap = embedded(3, [1, 2], 4);
    assert_eq!(
        bitmap
            .decode(&[1, 2, 3, 99, 4, 5, 6, 99], &color::Encoding::Srgb)
            .unwrap()
            .0,
        [1, 2, 3, 4, 5, 6].map(|v| v * 257)
    );
    assert!(embedded(3, [1, 2], 2)
        .decode(&[0; 8], &color::Encoding::Srgb)
        .is_err());
    assert!(bitmap.decode(&[0; 7], &color::Encoding::Srgb).is_err());
}

fn fixture(images: &[(u32, [u32; 2], u32, Vec<u8>)]) -> Vec<u8> {
    fixture_with_rotation(images, 0x20000, 90)
}

fn fixture_with_rotation(
    images: &[(u32, [u32; 2], u32, Vec<u8>)],
    version: u32,
    rotation: u32,
) -> Vec<u8> {
    let mut data = vec![0; if version >= 0x40000 { 52 } else { 40 }];
    data[..4].copy_from_slice(b"FOVb");
    data[4..8].copy_from_slice(&version.to_le_bytes());
    if version >= 0x40000 {
        data[8..12].copy_from_slice(&1_u32.to_le_bytes());
    }
    let offset = if version >= 0x40000 { 48 } else { 36 };
    data[offset..offset + 4].copy_from_slice(&rotation.to_le_bytes());
    let mut entries = Vec::new();
    for (format, extent, stride, payload) in images {
        let offset = data.len() as u32;
        data.extend_from_slice(b"SECi");
        for value in [0x20000, 2, *format, extent[0], extent[1], *stride] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        data.extend_from_slice(payload);
        entries.extend_from_slice(&offset.to_le_bytes());
        entries.extend_from_slice(&(28 + payload.len() as u32).to_le_bytes());
        entries.extend_from_slice(b"IMAG");
    }
    let directory = data.len() as u32;
    data.extend_from_slice(b"SECd");
    data.extend_from_slice(&0x20000_u32.to_le_bytes());
    data.extend_from_slice(&(images.len() as u32).to_le_bytes());
    data.extend(entries);
    data.extend_from_slice(&directory.to_le_bytes());
    data
}

fn with_camera_properties(mut data: Vec<u8>, model: &str, space: &str) -> Vec<u8> {
    let directory = u32_at(&data, data.len() - 4) as usize;
    let count = u32_at(&data, directory + 8);
    let mut entries = data[directory + 12..data.len() - 4].to_vec();
    data.truncate(directory);
    let offset = data.len() as u32;
    let mut strings = Vec::new();
    let mut table = Vec::new();
    for (key, value) in [("CAMMODEL", model), ("COLORSPACE", space)] {
        for text in [key, value] {
            table.extend_from_slice(&(strings.len() as u32).to_le_bytes());
            strings.extend(text.encode_utf16().chain([0]));
        }
    }
    data.extend(b"SECp");
    for value in [0x20000, 2, 0, 0, strings.len() as u32] {
        data.extend(value.to_le_bytes());
    }
    data.extend(table);
    for unit in strings {
        data.extend(unit.to_le_bytes());
    }
    let size = data.len() as u32 - offset;
    entries.extend(offset.to_le_bytes());
    entries.extend(size.to_le_bytes());
    entries.extend(b"PROP");
    let directory = data.len() as u32;
    data.extend(b"SECd");
    data.extend(0x20000_u32.to_le_bytes());
    data.extend((count + 1).to_le_bytes());
    data.extend(entries);
    data.extend(directory.to_le_bytes());
    data
}

fn jpeg_pixel(pixel: [u8; 3], extent: [u32; 2]) -> Vec<u8> {
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
        .encode(
            &pixel.repeat((extent[0] * extent[1]) as usize),
            extent[0],
            extent[1],
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    jpeg
}

#[test]
fn camera_space_is_retained_for_small_and_large_previews_across_architectures() {
    for (model, version, small_format) in [
        ("SIGMA SD15", 0x20000, 3),
        ("SIGMA DP2 Merrill", 0x30000, 18),
        ("SIGMA dp0 Quattro", 0x40001, 25),
        ("SIGMA sd Quattro H", 0x40002, 25),
    ] {
        let small = if small_format == 3 {
            vec![128, 64, 32]
        } else {
            jpeg_pixel([128, 64, 32], [1, 1])
        };
        let large = jpeg_pixel([80, 120, 160], [2, 1]);
        let expected_small = embedded(small_format, [1, 1], 3)
            .decode(&small, &color::Encoding::Srgb)
            .unwrap()
            .0;
        let expected_large = embedded(18, [2, 1], 0)
            .decode(&large, &color::Encoding::Srgb)
            .unwrap()
            .0;
        let bytes = with_camera_properties(
            fixture_with_rotation(
                &[(small_format, [1, 1], 3, small), (18, [2, 1], 0, large)],
                version,
                90,
            ),
            model,
            "AdobeRGB",
        );
        for (full, expected) in [(false, expected_small), (true, expected_large)] {
            let preview = from_original(&bytes, full).unwrap();
            assert_eq!(preview.rgb, expected, "{model}, full={full}");
            assert_eq!(
                preview.color_space,
                crate::color_space::TiffColorSpace::AdobeRgb
            );
        }
    }
}

#[test]
fn version_four_without_properties_inherits_color_space_from_jpeg_makernotes() {
    let mut tiff = vec![0_u8; 256];
    tiff[..2].copy_from_slice(b"II");
    tiff[2..4].copy_from_slice(&42_u16.to_le_bytes());
    tiff[4..8].copy_from_slice(&8_u32.to_le_bytes());
    let entry = |bytes: &mut [u8], offset, tag: u16, kind: u16, count: u32, value: u32| {
        bytes[offset..offset + 2].copy_from_slice(&tag.to_le_bytes());
        bytes[offset + 2..offset + 4].copy_from_slice(&kind.to_le_bytes());
        bytes[offset + 4..offset + 8].copy_from_slice(&count.to_le_bytes());
        bytes[offset + 8..offset + 12].copy_from_slice(&value.to_le_bytes());
    };
    tiff[8..10].copy_from_slice(&2_u16.to_le_bytes());
    entry(&mut tiff, 10, 0x110, 2, 13, 200);
    entry(&mut tiff, 22, 0x8769, 4, 1, 40);
    tiff[40..42].copy_from_slice(&1_u16.to_le_bytes());
    entry(&mut tiff, 42, 0x927c, 7, 110, 80);
    tiff[80..88].copy_from_slice(b"SIGMA\0\0\0");
    tiff[90..92].copy_from_slice(&1_u16.to_le_bytes());
    entry(&mut tiff, 92, 0xb, 2, 9, 220);
    tiff[200..213].copy_from_slice(b"sd Quattro H\0");
    tiff[220..229].copy_from_slice(b"AdobeRGB\0");
    let jpeg = jpeg_pixel([128, 64, 32], [2, 1]);
    let mut tagged = vec![0xff, 0xd8, 0xff, 0xe1];
    tagged.extend(((tiff.len() + 8) as u16).to_be_bytes());
    tagged.extend(b"Exif\0\0");
    tagged.extend(tiff);
    tagged.extend(&jpeg[2..]);
    let small = jpeg_pixel([128, 64, 32], [1, 1]);
    let expected = embedded(18, [1, 1], 0)
        .decode(&small, &color::Encoding::AdobeRgb)
        .unwrap()
        .0;
    let bytes = fixture_with_rotation(
        &[(18, [2, 1], 0, tagged), (25, [1, 1], 0, small)],
        0x40002,
        0,
    );
    assert_eq!(from_original(&bytes, false).unwrap().rgb, expected);
    assert_eq!(from_original(&bytes, true).unwrap().rgb, expected.repeat(2));
}

#[test]
fn jpeg_icc_overrides_camera_space_and_srgb_bitmaps_keep_their_samples() {
    let pixel = [80, 120, 160];
    let jpeg = jpeg_pixel(pixel, [1, 1]);
    let mut tagged = vec![0xff, 0xd8, 0xff, 0xe2];
    let profile = include_bytes!("../assets/srgb-v4.icc");
    tagged.extend(((profile.len() + 16) as u16).to_be_bytes());
    tagged.extend(b"ICC_PROFILE\0\x01\x01");
    tagged.extend(profile);
    tagged.extend(&jpeg[2..]);
    let decoded = embedded(18, [1, 1], 0)
        .decode(&jpeg, &color::Encoding::Srgb)
        .unwrap()
        .0;
    let managed = embedded(18, [1, 1], 0)
        .decode(&tagged, &color::Encoding::AdobeRgb)
        .unwrap()
        .0;
    let managed = as_srgb16(&managed, crate::color_space::TiffColorSpace::ProPhotoRgb);
    assert!(decoded
        .iter()
        .zip(managed)
        .all(|(a, b)| a.abs_diff(b) <= 128));
    let inherited = color::Encoding::AdobeRgb.jpeg(&tagged);
    assert_eq!(
        embedded(18, [1, 1], 0).decode(&jpeg, &inherited).unwrap().0,
        embedded(18, [1, 1], 0)
            .decode(&tagged, &color::Encoding::AdobeRgb)
            .unwrap()
            .0,
        "an unlabelled nested thumbnail inherits its enclosing JPEG's profile"
    );
    tagged[20..20 + profile.len()].fill(0);
    assert_eq!(
        embedded(18, [1, 1], 0)
            .decode(&tagged, &color::Encoding::AdobeRgb)
            .unwrap()
            .0,
        embedded(18, [1, 1], 0)
            .decode(&jpeg, &color::Encoding::AdobeRgb)
            .unwrap()
            .0,
        "a malformed optional profile falls back to camera metadata"
    );
    let bytes = with_camera_properties(
        fixture(&[(3, [1, 1], 3, pixel.to_vec())]),
        "SIGMA SD15",
        "sRGB",
    );
    assert_eq!(
        from_original(&bytes, false).unwrap().rgb,
        pixel.map(|v| u16::from(v) * 257)
    );
}

#[test]
fn an_embedded_adobe_icc_profile_retains_colors_outside_srgb() {
    let profile =
        moxcms::ColorProfile::new_from_slice(include_bytes!("../assets/adobe-rgb-1998-compat.icc"))
            .unwrap();
    let encoding = color::Encoding::Profile {
        profile: std::sync::Arc::new(profile),
        fallback: Box::new(color::Encoding::Srgb),
    };
    let (rgb, space) = encoding.decode(&[0, 255, 0]).unwrap();
    assert_eq!(space, crate::color_space::TiffColorSpace::ProPhotoRgb);
    let matrix = crate::color_space::TiffColorSpace::Srgb.transform(space.into());
    let linear: [f32; 3] =
        std::array::from_fn(|c| decode_sample(space, f32::from(rgb[c]) / 65535.));
    let output: [f32; 3] = std::array::from_fn(|r| (0..3).map(|c| matrix[r][c] * linear[c]).sum());
    assert!(
        output[0] < -0.3 && output[1] > 0.99 && output[2] < -0.02,
        "{output:?}"
    );
}

#[test]
fn an_unusable_rgb_profile_falls_back_before_converting_any_pixels() {
    use moxcms::{ColorProfile, Layout, TransformOptions};
    let mut profile = ColorProfile::new_srgb();
    profile.red_trc = None;
    assert!(profile
        .create_transform_8bit(
            Layout::Rgb,
            &ColorProfile::new_srgb(),
            Layout::Rgb,
            TransformOptions::default(),
        )
        .is_err());
    let encoding = color::Encoding::Profile {
        profile: std::sync::Arc::new(profile),
        fallback: Box::new(color::Encoding::AdobeRgb),
    };
    let pixels = [80, 120, 160].repeat(5000);
    let expected = color::Encoding::AdobeRgb.decode(&pixels).unwrap();
    assert_eq!(encoding.decode(&pixels).unwrap(), expected);
}

fn as_srgb16(samples: &[u16], space: crate::color_space::TiffColorSpace) -> Vec<u16> {
    let matrix = crate::color_space::TiffColorSpace::Srgb.transform(space.into());
    samples
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|pixel| {
            let linear = pixel.map(|v| decode_sample(space, f32::from(v) / 65535.));
            (0..3).map(move |r| {
                encode_sample(
                    crate::color_space::TiffColorSpace::Srgb,
                    (0..3).map(|c| matrix[r][c] * linear[c]).sum(),
                )
            })
        })
        .collect()
}

#[test]
fn adobe_preview_retains_saturated_camera_codes_before_presentation() {
    let input = [0, 255, 0];
    let (rgb, space) = color::Encoding::AdobeRgb.decode(&input).unwrap();
    assert_eq!(space, crate::color_space::TiffColorSpace::AdobeRgb);
    assert_eq!(rgb, [0, 65535, 0]);
    let matrix = crate::color_space::TiffColorSpace::Srgb.transform(space.into());
    assert!(
        matrix[0][1] < 0. && matrix[2][1] < 0.,
        "the retained green lies outside sRGB"
    );
}

#[test]
fn selection_uses_pixel_area_and_keeps_largest_preview_geometry() {
    let bytes = fixture(&[
        (3, [2, 1], 6, vec![0; 6]),
        (3, [1, 1], 12, vec![1; 12]),
        (11, [20, 10], 0, vec![]),
    ]);
    let path = std::env::temp_dir().join(format!("foveonic-preview-{}.x3f", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let preview = read(&path).unwrap();
    let full = read_full(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(full.extent, [2, 1]);
    assert_eq!(full.rotation, 90);
    assert_eq!(preview.extent, [1, 1]);
    assert_eq!(preview.native_extent, [20, 10]);
    assert_eq!(preview.rotation, 90);
    assert_eq!(preview.rgb, [257, 257, 257]);
}

#[test]
fn jpeg_dimensions_must_match_the_container() {
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode(&[30, 90, 150], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    assert_eq!(
        embedded(18, [1, 1], 0)
            .decode(&jpeg, &color::Encoding::Srgb)
            .unwrap()
            .0
            .len(),
        3
    );
    assert!(embedded(18, [2, 1], 0)
        .decode(&jpeg, &color::Encoding::Srgb)
        .is_err());
    let mut larger = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut larger)
        .encode(&[30; 6], 2, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    assert!(embedded(18, [1, 1], 0)
        .decode(&larger, &color::Encoding::Srgb)
        .is_err());
    assert!(embedded(18, [1, 1], 0)
        .decode(&jpeg[..jpeg.len() / 2], &color::Encoding::Srgb)
        .is_err());
}

#[test]
#[ignore = "requires CLASSIC_SAMPLE_DIR"]
fn original_classic_previews_decode_without_sensor_processing() {
    let directory = std::path::PathBuf::from(std::env::var_os("CLASSIC_SAMPLE_DIR").unwrap());
    for name in ["sd9", "sd10", "sd14", "sd15", "dp1", "dp1s", "dp2-dpreview"] {
        let preview = read(&directory.join(format!("{name}.x3f"))).unwrap();
        assert_eq!(
            preview.rgb.len(),
            preview.extent[0] as usize * preview.extent[1] as usize * 3
        );
        assert!(preview.rgb.iter().any(|&v| v != 0));
        if ["sd9", "sd10"].contains(&name) {
            assert_eq!(preview.extent, [189, 126]);
            assert_eq!(preview.native_extent, [567, 378]);
        }
    }
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR"]
fn original_merrill_previews_decode_at_thumbnail_size() {
    let directory = std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap();
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("x3f"))
        {
            let preview = read(&path).unwrap();
            assert_eq!(preview.native_extent, [4704, 3136]);
            assert!(preview.extent[0] < preview.native_extent[0]);
            assert_eq!(
                preview.rgb.len(),
                preview.extent[0] as usize * preview.extent[1] as usize * 3
            );
        }
    }
}

#[test]
#[ignore = "requires CLASSIC_SAMPLE_DIR"]
fn original_bitmap_channels_match_same_file_jpeg() {
    let directory = std::path::PathBuf::from(std::env::var_os("CLASSIC_SAMPLE_DIR").unwrap());
    for name in ["sd14", "sd15", "dp1", "dp1s", "dp2-dpreview"] {
        let path = directory.join(format!("{name}.x3f"));
        let bytes = std::fs::read(&path).unwrap();
        let directory_offset = u32_at(&bytes, bytes.len() - 4) as usize;
        let count = u32_at(&bytes, directory_offset + 8) as usize;
        let entries = &bytes[directory_offset + 12..directory_offset + 12 + count * 12];
        let jpeg = entries
            .as_chunks::<12>()
            .0
            .iter()
            .find_map(|entry| {
                if &entry[8..] != b"IMAG" && &entry[8..] != b"IMA2" {
                    return None;
                }
                let offset = u32_at(entry, 0) as usize;
                let size = u32_at(entry, 4) as usize;
                (u32_at(&bytes, offset + 8) == 2 && u32_at(&bytes, offset + 12) == 18)
                    .then_some(&bytes[offset + 28..offset + size])
            })
            .expect("same original must contain a JPEG for independent color comparison");
        let preview = read(&path).unwrap();
        let reference = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
            .unwrap()
            .into_rgb8();
        let (rgb, space) =
            color::Encoding::camera(&crate::source::read_capture_properties(&path).unwrap())
                .jpeg(jpeg)
                .decode(reference.as_raw())
                .unwrap();
        let reference = image::ImageBuffer::<image::Rgb<u16>, _>::from_raw(
            reference.width(),
            reference.height(),
            as_srgb16(&rgb, space),
        )
        .unwrap();
        let reference = image::imageops::resize(
            &reference,
            preview.extent[0],
            preview.extent[1],
            image::imageops::FilterType::Triangle,
        );
        let mut direct = 0_u64;
        let mut swapped = 0_u64;
        for (thumbnail, expected) in as_srgb16(&preview.rgb, preview.color_space)
            .as_chunks::<3>()
            .0
            .iter()
            .zip(reference.as_raw().as_chunks::<3>().0.iter())
        {
            for channel in 0..3 {
                direct += u64::from(thumbnail[channel].abs_diff(expected[channel]));
                swapped += u64::from(thumbnail[2 - channel].abs_diff(expected[channel]));
            }
        }
        let samples = preview.rgb.len() as f64;
        eprintln!(
            "{name}: RGB MAE {:.3}, swapped MAE {:.3}",
            direct as f64 / samples,
            swapped as f64 / samples
        );
        assert!(
            direct * 10 < swapped * 9,
            "{name}: embedded RGB should match its JPEG substantially better than swapped channels"
        );
    }
}

#[test]
fn previews_share_versioned_rotation_with_sensor_metadata() {
    let path = std::env::temp_dir().join(format!(
        "foveonic-preview-orientation-{}.x3f",
        std::process::id()
    ));
    for version in [0x20000, 0x30000, 0x40001, 0x40002] {
        for degrees in [0, 90, 180, 270] {
            let bytes = fixture_with_rotation(&[(3, [2, 1], 6, vec![1; 6])], version, degrees);
            std::fs::write(&path, bytes).unwrap();
            let preview = read(&path).unwrap();
            assert_eq!(preview.rotation, degrees);
            assert_eq!(preview.extent, [2, 1]);
            assert_eq!(preview.rgb, [257; 6]);
        }
    }
    let mut bytes = fixture_with_rotation(&[(3, [2, 1], 6, vec![1; 6])], 0x40001, 270);
    bytes[8..12].copy_from_slice(&2_u32.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    assert!(read(&path)
        .err()
        .unwrap()
        .to_string()
        .contains("byte order"));
    std::fs::remove_file(path).unwrap();
}

#[test]
#[ignore = "requires QUATTRO_SAMPLE_DIR"]
fn original_quattro_preview_rotation_matches_camera_orientation() {
    let directory = std::path::PathBuf::from(std::env::var_os("QUATTRO_SAMPLE_DIR").unwrap());
    for (name, expected) in [
        ("dp0-quattro", 0),
        ("dp1-quattro-pblog", 0),
        ("dp2-quattro", 0),
        ("dp3-quattro", 270),
        ("dp3-quattro-iso800", 270),
        ("sd-quattro", 0),
    ] {
        let preview = read(&directory.join(format!("{name}.x3f"))).unwrap();
        assert_eq!(preview.rotation, expected, "{name}");
    }
}

#[test]
fn version_four_thumbnail_uses_raw_header_geometry_before_rendering() {
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode(&[70; 18], 3, 2, image::ExtendedColorType::Rgb8)
        .unwrap();
    let mut bytes = fixture_with_rotation(
        &[(25, [3, 2], 0, jpeg), (11, [4, 3], 0, vec![])],
        0x40001,
        270,
    );
    bytes[40..44].copy_from_slice(&5424_u32.to_le_bytes());
    bytes[44..48].copy_from_slice(&3616_u32.to_le_bytes());
    let path = std::env::temp_dir().join(format!(
        "foveonic-quattro-thumbnail-{}.x3f",
        std::process::id()
    ));
    std::fs::write(&path, bytes).unwrap();
    let preview = read(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(preview.extent, [3, 2]);
    assert_eq!(preview.native_extent, [5424, 3616]);
    assert_eq!(preview.rotation, 270);
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR with Quattro originals"]
fn original_quattro_thumbnails_preserve_raw_framing_and_orientation() {
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    for name in [
        "dp0-quattro",
        "dp1-quattro-pblog",
        "dp2-quattro",
        "dp3-quattro",
        "sd-quattro",
        "sd-quattro-h",
    ] {
        let preview = read(&root.join(format!("{name}.x3f"))).unwrap();
        assert!(
            preview.extent[0] <= 320 && preview.extent[1] <= 320,
            "{name}: {:?}",
            preview.extent
        );
        let expected = if name == "sd-quattro-h" {
            [6192, 4128]
        } else {
            [5424, 3616]
        };
        assert_eq!(preview.native_extent, expected, "{name}");
        let aspect = preview.extent[0] as f64 / preview.extent[1] as f64;
        assert!(
            (aspect - 1.5).abs() < 0.02,
            "{name}: thumbnail framing changed"
        );
    }
}

#[test]
fn exif_padding_is_removed_only_when_it_matches_the_photo_frame() {
    let mut pixels = image::RgbImage::from_pixel(160, 120, image::Rgb([0, 0, 0]));
    for y in 7..113 {
        for x in 0..160 {
            pixels.put_pixel(x, y, image::Rgb([80, 120, 160]));
        }
    }
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode_image(&pixels)
        .unwrap();
    let (rgb, extent, space) =
        decode_exif_thumbnail(&jpeg, [5424, 3616], &color::Encoding::Srgb).unwrap();
    assert_eq!(extent, [160, 106]);
    assert_eq!(rgb.len(), 160 * 106 * 3);
    let (adobe, extent, adobe_space) =
        decode_exif_thumbnail(&jpeg, [5424, 3616], &color::Encoding::AdobeRgb).unwrap();
    assert_eq!(extent, [160, 106]);
    assert_eq!(adobe, rgb);
    assert_eq!(space, crate::color_space::TiffColorSpace::Srgb);
    assert_eq!(adobe_space, crate::color_space::TiffColorSpace::AdobeRgb);
    let pixels = image::RgbImage::from_pixel(160, 120, image::Rgb([80, 120, 160]));
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode_image(&pixels)
        .unwrap();
    assert!(decode_exif_thumbnail(&jpeg, [5424, 3616], &color::Encoding::Srgb).is_err());
}
