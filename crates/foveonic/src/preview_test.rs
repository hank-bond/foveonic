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
fn bitmap_preserves_rgb_and_omits_row_padding() {
    let bitmap = embedded(3, [1, 2], 4);
    assert_eq!(
        bitmap.decode(&[1, 2, 3, 99, 4, 5, 6, 99]).unwrap(),
        [1, 2, 3, 4, 5, 6]
    );
    assert!(embedded(3, [1, 2], 2).decode(&[0; 8]).is_err());
    assert!(bitmap.decode(&[0; 7]).is_err());
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
    std::fs::remove_file(path).unwrap();
    assert_eq!(preview.extent, [1, 1]);
    assert_eq!(preview.native_extent, [20, 10]);
    assert_eq!(preview.rotation, 90);
    assert_eq!(preview.rgb, [1, 1, 1]);
}

#[test]
fn jpeg_dimensions_must_match_the_container() {
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode(&[30, 90, 150], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    assert_eq!(embedded(18, [1, 1], 0).decode(&jpeg).unwrap().len(), 3);
    assert!(embedded(18, [2, 1], 0).decode(&jpeg).is_err());
    let mut larger = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut larger)
        .encode(&[30; 6], 2, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    assert!(embedded(18, [1, 1], 0).decode(&larger).is_err());
    assert!(embedded(18, [1, 1], 0)
        .decode(&jpeg[..jpeg.len() / 2])
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
        let reference = image::imageops::resize(
            &reference,
            preview.extent[0],
            preview.extent[1],
            image::imageops::FilterType::Triangle,
        );
        let mut direct = 0_u64;
        let mut swapped = 0_u64;
        for (thumbnail, expected) in preview
            .rgb
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
            assert_eq!(preview.rgb, [1; 6]);
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
