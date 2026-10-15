//! Writes editable camera samples and one selected look into an uncompressed DNG.

use super::appearance::{Prepared, ProfileData, DIMS};
use crate::{
    render::Settings,
    source::{Metadata, Result},
};
use std::io::{Seek, Write};
use tiff::{
    encoder::{DirectoryEncoder, Rational, SRational, TiffEncoder, TiffKindStandard, TiffValue},
    tags::Tag,
};

const ROWS_PER_STRIP: u32 = 32;
const DENOMINATOR: u32 = 1_000_000;

/// The TIFF directory API leaves alignment to us. Odd-length text or history
/// would put later values and the directory on an invalid byte boundary.
/// Add padding outside the declared count so it does not become tag data.
fn tag<W: Write + Seek, T: TiffValue>(
    directory: &mut DirectoryEncoder<'_, W, TiffKindStandard>,
    number: u16,
    value: T,
) -> Result<()> {
    let bytes = value.bytes();
    directory.write_tag(Tag::Unknown(number), value)?;
    if bytes > 4 && !bytes.is_multiple_of(2) {
        directory.write_data(&[0_u8][..])?;
    }
    Ok(())
}

fn unsigned_rationals(values: &[f64]) -> Result<Vec<Rational>> {
    values
        .iter()
        .map(|v| {
            let numerator = (v * f64::from(DENOMINATOR)).round();
            if !numerator.is_finite() || !(0. ..=u32::MAX as f64).contains(&numerator) {
                return Err("unrepresentable DNG rational".into());
            }
            Ok(Rational {
                n: numerator as u32,
                d: DENOMINATOR,
            })
        })
        .collect()
}

fn signed_rationals(values: &[f64]) -> Result<Vec<SRational>> {
    values
        .iter()
        .map(|v| {
            let numerator = (v * f64::from(DENOMINATOR)).round();
            if !numerator.is_finite() || !(i32::MIN as f64..=i32::MAX as f64).contains(&numerator) {
                return Err("unrepresentable DNG rational".into());
            }
            Ok(SRational {
                n: numerator as i32,
                d: DENOMINATOR as i32,
            })
        })
        .collect()
}

/// The GPU pack has a spare fourth component that is not part of the photo.
/// Drop it without changing the RGB values. Work one strip at a time to avoid
/// allocating another full-image copy.
fn samples<W: Write + Seek>(
    directory: &mut DirectoryEncoder<'_, W, TiffKindStandard>,
    packed: &[u8],
    size: [u32; 2],
) -> Result<()> {
    let [width, height] = size;
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(8));
    if width == 0 || height == 0 || expected != Some(packed.len()) {
        return Err("invalid DNG packed geometry".into());
    }
    let strip_bytes = (width as usize)
        .checked_mul(ROWS_PER_STRIP as usize * 8)
        .ok_or("invalid DNG packed geometry")?;
    let mut offsets = Vec::new();
    let mut counts = Vec::new();
    for strip in packed.chunks(strip_bytes) {
        let rgb: Vec<u16> = strip
            .as_chunks::<8>()
            .0
            .iter()
            .flat_map(|pixel| [0, 2, 4].map(|i| u16::from_le_bytes([pixel[i], pixel[i + 1]])))
            .collect();
        let bytes = u32::try_from(rgb.len() * 2)?;
        let offset = u32::try_from(directory.write_data(rgb.as_slice())?)?;
        // A valid start offset is not enough if the strip's end exceeds 4 GiB.
        offset
            .checked_add(bytes)
            .ok_or("DNG exceeds classic TIFF size")?;
        offsets.push(offset);
        counts.push(bytes);
    }
    tag(directory, 254, 0_u32)?; // NewSubfileType: main image.
    tag(directory, 256, width)?;
    tag(directory, 257, height)?;
    tag(directory, 258, &[16_u16; 3][..])?;
    tag(directory, 259, 1_u16)?; // Compression: uncompressed.
    tag(directory, 262, 34892_u16)?; // PhotometricInterpretation: LinearRaw.
    tag(directory, 273, offsets.as_slice())?;
    tag(directory, 274, 1_u16)?; // GPU has already applied orientation.
    tag(directory, 277, 3_u16)?;
    tag(directory, 278, ROWS_PER_STRIP)?;
    tag(directory, 279, counts.as_slice())?;
    tag(directory, 284, 1_u16)?; // PlanarConfiguration: interleaved.
    tag(directory, 339, &[1_u16; 3][..])?; // SampleFormat: unsigned integer.
    Ok(())
}

pub(super) fn write<W: Write + Seek>(
    mut out: W,
    packed: &[u8],
    size: [u32; 2],
    prepared: &Prepared,
    presentation: &ProfileData,
    metadata: &Metadata,
    history_context: (&Settings, f32),
) -> Result<()> {
    let (settings, scale) = history_context;
    let [width, height] = size;
    let mut encoder = TiffEncoder::new(&mut out)?;
    let mut directory = encoder.image_directory()?;
    samples(&mut directory, packed, size)?;

    let model = metadata
        .properties
        .get("CAMMODEL")
        .ok_or("missing DNG camera model")?;
    tag(&mut directory, 271, "SIGMA")?;
    tag(&mut directory, 272, model.as_str())?;
    tag(&mut directory, 305, "Foveonic GPU")?;
    tag(&mut directory, 50706, &[1_u8, 4, 0, 0][..])?; // DNGVersion.
    tag(&mut directory, 50707, &[1_u8, 4, 0, 0][..])?; // DNGBackwardVersion.
    tag(&mut directory, 50708, format!("Foveonic {model}").as_str())?;
    tag(&mut directory, 50713, &[1_u16, 1][..])?; // BlackLevelRepeatDim.
    tag(
        &mut directory,
        50714,
        unsigned_rationals(&[0.; 3])?.as_slice(),
    )?;
    tag(&mut directory, 50717, &[65535_u32; 3][..])?;
    tag(
        &mut directory,
        50718,
        unsigned_rationals(&[1., 1.])?.as_slice(),
    )?;
    tag(&mut directory, 50719, &[0_u32, 0][..])?;
    tag(&mut directory, 50720, &[width, height][..])?;
    tag(
        &mut directory,
        50721,
        signed_rationals(&prepared.color.0)?.as_slice(),
    )?;
    tag(
        &mut directory,
        50728,
        unsigned_rationals(&prepared.neutral)?.as_slice(),
    )?;
    tag(
        &mut directory,
        50730,
        signed_rationals(&[presentation.baseline])?.as_slice(),
    )?;
    tag(&mut directory, 50731, unsigned_rationals(&[1.])?.as_slice())?;
    tag(&mut directory, 50732, unsigned_rationals(&[1.])?.as_slice())?;
    tag(&mut directory, 50739, unsigned_rationals(&[1.])?.as_slice())?;
    tag(&mut directory, 50778, 23_u16)?; // CalibrationIlluminant1: D50.
    tag(
        &mut directory,
        50964,
        signed_rationals(&prepared.forward.0)?.as_slice(),
    )?;
    let name = format!("Foveonic {}", settings.color_mode);
    tag(&mut directory, 50936, name.as_str())?; // ProfileName.
    tag(&mut directory, 50934, name.as_str())?; // AsShotProfileName.
    tag(&mut directory, 50940, presentation.curve.as_slice())?;
    tag(&mut directory, 50981, DIMS.as_slice())?;
    tag(&mut directory, 50982, presentation.look.as_slice())?;
    tag(&mut directory, 51108, 1_u32)?; // ProfileLookTableEncoding: sRGB.
    tag(&mut directory, 51110, 1_u32)?; // DefaultBlackRender: None.

    // These samples already include technical corrections. To resume edits,
    // use the X3F and sidecar rather than treating this history as a recipe to replay.
    let history = serde_json::json!({
        "format": "foveonic-dng-history", "version": 1, "settings": settings,
        "baked_controls": ["calibration", "repair", "highlight_restoration", "despeckle", "row_correction", "noise_and_detail", "color_dq", "sharpening"],
        "metadata_controls": ["color_mode", "exposure", "tone", "saturation"],
        "levels": {"black": 0, "white": 65535, "negative_samples": "clamped", "common_scale": scale},
        "calibration_exposure": prepared.calibration_exposure
    });
    let mut private = b"Foveonic\0".to_vec();
    private.extend(serde_json::to_vec(&history)?);
    tag(&mut directory, 50740, private.as_slice())?;
    // Drop hides final write errors, which could make a broken export look successful.
    directory.finish()?;
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // The TIFF decoder can read our value types but not DNG LinearRaw images.
    // Inspect tags here and use the SDK for independent sample decoding.
    fn entries(bytes: &[u8]) -> Vec<(u16, u16, u32, Vec<u8>)> {
        assert_eq!(&bytes[..4], b"II\x2a\0");
        let u16_at = |i| u16::from_le_bytes(bytes[i..i + 2].try_into().unwrap());
        let u32_at = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        let root = u32_at(4) as usize;
        assert_eq!(root % 2, 0);
        let mut previous = 0;
        (0..u16_at(root) as usize)
            .map(|i| {
                let start = root + 2 + i * 12;
                let (number, kind, count) = (u16_at(start), u16_at(start + 2), u32_at(start + 4));
                assert!(number > previous);
                previous = number;
                let unit = match kind {
                    1 | 2 => 1,
                    3 => 2,
                    4 | 11 => 4,
                    5 | 10 => 8,
                    _ => panic!("unexpected tag type"),
                };
                let length = count as usize * unit;
                let offset = if length <= 4 {
                    start + 8
                } else {
                    let offset = u32_at(start + 8) as usize;
                    assert_eq!(offset % 2, 0, "unaligned tag {number}");
                    offset
                };
                (number, kind, count, bytes[offset..offset + length].to_vec())
            })
            .collect()
    }

    fn packed(values: &[u16]) -> Vec<u8> {
        values
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| {
                [p[0], p[1], p[2], 0xdead]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
            })
            .collect()
    }

    #[test]
    fn uncompressed_strips_preserve_rgb16_and_exclude_gpu_padding() {
        for size in [[1, 1], [3, 33], [257, 65]] {
            let values: Vec<u16> = (0..size[0] * size[1] * 3)
                .map(|i| (i * 101) as u16)
                .collect();
            let mut out = Cursor::new(Vec::new());
            let mut encoder = TiffEncoder::new(&mut out).unwrap();
            let mut directory = encoder.image_directory().unwrap();
            samples(&mut directory, &packed(&values), size).unwrap();
            directory.finish().unwrap();
            let bytes = out.into_inner();
            let tags = entries(&bytes);
            let entry = |number| &tags.iter().find(|e| e.0 == number).unwrap().3;
            assert_eq!(entry(259), &1_u16.to_le_bytes());
            let offsets = entry(273)
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| u32::from_le_bytes(*v));
            let counts = entry(279)
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| u32::from_le_bytes(*v));
            let actual: Vec<u16> = offsets
                .zip(counts)
                .flat_map(|(offset, count)| {
                    bytes[offset as usize..(offset + count) as usize]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|v| u16::from_le_bytes(*v))
                })
                .collect();
            assert_eq!(actual, values);
            assert_eq!(
                entry(279).len() / 4,
                size[1].div_ceil(ROWS_PER_STRIP) as usize
            );
        }
    }

    #[test]
    fn directory_preserves_dng_types_and_word_alignment() {
        let mut out = Cursor::new(Vec::new());
        let mut encoder = TiffEncoder::new(&mut out).unwrap();
        let mut directory = encoder.image_directory().unwrap();
        tag(&mut directory, 305, "ABCD").unwrap(); // An odd-length value catches missing alignment padding.
        tag(&mut directory, 50740, &[1_u8, 2, 3, 4, 5, 6, 7][..]).unwrap();
        tag(
            &mut directory,
            50728,
            unsigned_rationals(&[0.5, 1., 0.25]).unwrap().as_slice(),
        )
        .unwrap();
        tag(
            &mut directory,
            50721,
            signed_rationals(&[-0.125, 1., 2.]).unwrap().as_slice(),
        )
        .unwrap();
        tag(&mut directory, 50940, &[0_f32, 0., 1., 1.][..]).unwrap();
        directory.finish().unwrap();
        let tags = entries(out.get_ref());
        assert_eq!(
            tags.iter().map(|e| (e.0, e.1, e.2)).collect::<Vec<_>>(),
            [
                (305, 2, 5),
                (50721, 10, 3),
                (50728, 5, 3),
                (50740, 1, 7),
                (50940, 11, 4)
            ]
        );
        let matrix = &tags[1].3;
        assert_eq!(
            i32::from_le_bytes(matrix[..4].try_into().unwrap()),
            -125_000
        );
        assert_eq!(
            u32::from_le_bytes(matrix[4..8].try_into().unwrap()),
            DENOMINATOR
        );
        assert_eq!(tags[0].3, b"ABCD\0");
        assert_eq!(tags[3].3, [1, 2, 3, 4, 5, 6, 7]);
        for invalid in [f64::NAN, f64::INFINITY, f64::MAX] {
            assert!(unsigned_rationals(&[invalid]).is_err());
            assert!(signed_rationals(&[invalid]).is_err());
        }
        assert!(unsigned_rationals(&[-1.]).is_err());
    }

    #[test]
    fn invalid_packed_geometry_is_rejected() {
        for (bytes, size) in [
            (vec![], [0, 1]),
            (vec![0; 7], [1, 1]),
            (vec![0; 8], [2, 1]),
            (vec![], [u32::MAX, u32::MAX]),
        ] {
            let mut out = Cursor::new(Vec::new());
            let mut encoder = TiffEncoder::new(&mut out).unwrap();
            assert!(samples(&mut encoder.image_directory().unwrap(), &bytes, size).is_err());
        }
    }

    #[test]
    #[ignore = "requires FOVEONIC_FIXTURE_DIR, no GPU"]
    fn uncompressed_file_preserves_samples_and_selected_profile_metadata() {
        let source = crate::source::Source::open(
            std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
                .join("blue-parking-sign.x3f"),
        )
        .unwrap();
        let settings = Settings::new("Sunlight", "Vivid");
        let prepared = Prepared::new(&source.metadata, &settings).unwrap();
        let profile = prepared.finish(2.).unwrap();
        assert_eq!(&profile.curve[..2], &[0., 0.]);
        assert_eq!(&profile.curve[profile.curve.len() - 2..], &[1., 1.]);
        assert_eq!(
            profile.look.len(),
            DIMS.into_iter().product::<u32>() as usize * 3
        );
        for entry in profile
            .look
            .as_chunks::<3>()
            .0
            .iter()
            .step_by(DIMS[1] as usize)
        {
            assert_eq!(entry, &[0., 1., 1.]);
        }
        let values: Vec<u16> = (0..256 * 33 * 3).map(|i| (i * 101) as u16).collect();
        let mut out = Cursor::new(Vec::new());
        write(
            &mut out,
            &packed(&values),
            [256, 33],
            &prepared,
            &profile,
            &source.metadata,
            (&settings, 2.),
        )
        .unwrap();
        let bytes = out.into_inner();
        let tags = entries(&bytes);
        let entry = |number| tags.iter().find(|e| e.0 == number).unwrap();
        assert_eq!(entry(259).3, 1_u16.to_le_bytes());
        assert_eq!(entry(50717).2, 3);
        assert_eq!(entry(50940).2, 1026);
        assert_eq!(entry(50936).3, b"Foveonic Vivid\0");
        assert_eq!(
            entry(50981).3,
            DIMS.into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            entry(50721).3,
            signed_rationals(&prepared.color.0).unwrap().data().as_ref()
        );
        assert_eq!(
            entry(50964).3,
            signed_rationals(&prepared.forward.0)
                .unwrap()
                .data()
                .as_ref()
        );
        let private = &entry(50740).3;
        assert!(private.starts_with(b"Foveonic\0"));
        let history: serde_json::Value = serde_json::from_slice(&private[9..]).unwrap();
        assert_eq!(
            history["settings"],
            serde_json::to_value(&settings).unwrap()
        );
        assert_eq!(history["levels"]["common_scale"], 2.);
        if let Some(path) = std::env::var_os("FOVEONIC_DNG_STAGE1") {
            let mut decoder = tiff::decoder::Decoder::new(std::io::BufReader::new(
                std::fs::File::open(path).unwrap(),
            ))
            .unwrap();
            assert_eq!(decoder.dimensions().unwrap(), (256, 33));
            let tiff::decoder::DecodingResult::U16(independent) = decoder.read_image().unwrap()
            else {
                panic!("SDK stage 1 did not decode uint16")
            };
            assert_eq!(independent, values);
        }
        if let Some(path) = std::env::var_os("FOVEONIC_DNG_SYNTHETIC") {
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .unwrap()
                .write_all(&bytes)
                .unwrap();
        }
    }
}
