//! Checks encoded precision, color profiles, compression tags and rejected output requests.

use super::*;
use std::io::Cursor;

fn metadata() -> crate::source::Metadata {
    crate::source::Metadata {
        width: 17,
        height: 9,
        rotation: 0,
        processing_version: None,
        properties: [
            ("CAMMODEL".into(), "SIGMA DP2 Merrill".into()),
            ("ISO".into(), "200".into()),
            ("SHUTTER".into(), "0.01".into()),
        ]
        .into(),
        matrices: Default::default(),
        lists: Default::default(),
    }
}
fn fixture() -> (EncodeParams, Vec<u8>, Vec<u16>) {
    let params = EncodeParams {
        mapping: [[0.; 4]; 2],
        source: [17, 9, 0, 0],
        crop: [0, 0, 17, 9],
        output: [17, 9, 0, 0],
    };
    let rgb: Vec<u16> = (0..17 * 9 * 3)
        .map(|i| [0, 1, 128, 257, 32768, 64000, 65535][i % 7])
        .collect();
    let packed = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2], 0].into_iter().flat_map(u16::to_le_bytes))
        .collect();
    (params, packed, rgb)
}
fn encoded(format: Format, options: EncodingOptions, space: TiffColorSpace) -> Vec<u8> {
    let (params, bytes, _) = fixture();
    let mut writer = Cursor::new(Vec::new());
    write(
        &mut writer,
        &bytes,
        &params,
        space,
        format,
        options,
        &metadata(),
        &mut Default::default(),
    )
    .unwrap();
    writer.into_inner()
}

#[test]
fn tiff_compression_preserves_rgb16_and_destination_profile() {
    for space in TiffColorSpace::ALL {
        for compression in TiffCompression::ALL {
            let data = encoded(
                Format::Tiff,
                EncodingOptions {
                    tiff_compression: compression,
                    ..Default::default()
                },
                space,
            );
            let mut decoder = tiff::decoder::Decoder::new(Cursor::new(data)).unwrap();
            assert_eq!(decoder.dimensions().unwrap(), (17, 9));
            assert_eq!(
                decoder.get_tag_u32(tiff::tags::Tag::Compression).unwrap(),
                match compression {
                    TiffCompression::None => 1,
                    TiffCompression::Lzw => 5,
                    TiffCompression::Zip => 8,
                }
            );
            assert_eq!(
                decoder
                    .get_tag_u8_vec(tiff::tags::Tag::Unknown(34675))
                    .unwrap(),
                space.profile()
            );
            let tiff::decoder::DecodingResult::U16(pixels) = decoder.read_image().unwrap() else {
                panic!("expected RGB16")
            };
            assert_eq!(pixels, fixture().2);
        }
    }
}

#[test]
fn uncompressed_tiff_strips_match_whole_image_encoding() {
    let [width, height] = [1024, 200];
    let rgb: Vec<u16> = (0..width * height * 3).map(|i| (i * 37) as u16).collect();
    let packed: Vec<u8> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|p| {
            [p[0], p[1], p[2], 65535]
                .into_iter()
                .flat_map(u16::to_le_bytes)
        })
        .collect();
    let params = EncodeParams {
        mapping: [[0.; 4]; 2],
        source: [width, height, 0, 0],
        crop: [0, 0, width, height],
        output: [width, height, 0, 0],
    };
    let space = TiffColorSpace::Srgb;
    let mut actual = Cursor::new(Vec::new());
    write(
        &mut actual,
        &packed,
        &params,
        space,
        Format::Tiff,
        Default::default(),
        &metadata(),
        &mut Default::default(),
    )
    .unwrap();
    let mut expected = Cursor::new(Vec::new());
    {
        let mut encoder = tiff::encoder::TiffEncoder::new(&mut expected).unwrap();
        let metadata = metadata();
        let capture = crate::capture_metadata::Capture::new(&metadata);
        let exif = capture.write_exif(&mut encoder, [width, height]).unwrap();
        let mut image = encoder
            .new_image::<tiff::encoder::colortype::RGB16>(width, height)
            .unwrap();
        capture.write_primary(image.encoder(), exif).unwrap();
        image
            .encoder()
            .write_tag(tiff::tags::Tag::Unknown(34675), space.profile())
            .unwrap();
        image
            .encoder()
            .write_tag(tiff::tags::Tag::Orientation, 1_u16)
            .unwrap();
        image
            .encoder()
            .write_tag(tiff::tags::Tag::Software, "Foveonic GPU")
            .unwrap();
        image.write_data(&rgb).unwrap();
    }
    assert_eq!(actual.into_inner(), expected.into_inner());
}

#[test]
fn png_depth_and_profiles_preserve_the_selected_samples() {
    use image::ImageDecoder;
    for space in TiffColorSpace::ALL {
        for depth in [BitDepth::Eight, BitDepth::Sixteen] {
            let data = encoded(
                Format::Png,
                EncodingOptions {
                    png_depth: depth,
                    ..Default::default()
                },
                space,
            );
            let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(&data)).unwrap();
            assert_eq!(decoder.icc_profile().unwrap().unwrap(), space.profile());
            let decoded = image::load_from_memory(&data).unwrap();
            match depth {
                BitDepth::Sixteen => assert_eq!(decoded.into_rgb16().as_raw(), &fixture().2),
                BitDepth::Eight => assert_eq!(
                    decoded.into_rgb8().as_raw(),
                    &fixture()
                        .2
                        .iter()
                        .map(|&v| ((u32::from(v) + 128) / 257) as u8)
                        .collect::<Vec<_>>()
                ),
            }
        }
    }
}

#[test]
fn png_rgb16_stream_preserves_rows_and_propagates_writer_errors() {
    use image::ImageDecoder;
    use std::io::{Seek, SeekFrom, Write};
    let [width, height] = [257, 33];
    let rgb: Vec<u16> = (0..width * height * 3)
        .map(|i| (i * 37 + i / 73) as u16)
        .collect();
    let packed: Vec<u8> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|p| {
            [p[0], p[1], p[2], 65535]
                .into_iter()
                .flat_map(u16::to_le_bytes)
        })
        .collect();
    let params = EncodeParams {
        mapping: [[0.; 4]; 2],
        source: [width, height, 0, 0],
        crop: [0, 0, width, height],
        output: [width, height, 0, 0],
    };
    let mut output = Cursor::new(Vec::new());
    write(
        &mut output,
        &packed,
        &params,
        TiffColorSpace::AdobeRgb,
        Format::Png,
        Default::default(),
        &metadata(),
        &mut Default::default(),
    )
    .unwrap();
    let data = output.into_inner();
    let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(&data)).unwrap();
    assert_eq!(
        decoder.icc_profile().unwrap().unwrap(),
        TiffColorSpace::AdobeRgb.profile()
    );
    assert_eq!(
        image::load_from_memory(&data)
            .unwrap()
            .into_rgb16()
            .as_raw(),
        &rgb
    );

    struct FailingWriter {
        inner: Cursor<Vec<u8>>,
        remaining: usize,
        fail_flush: bool,
    }
    impl Write for FailingWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::other("injected failure"));
            }
            let n = bytes.len().min(self.remaining);
            self.remaining -= n;
            self.inner.write(&bytes[..n])
        }
        fn flush(&mut self) -> std::io::Result<()> {
            if self.fail_flush {
                Err(std::io::Error::other("injected flush failure"))
            } else {
                self.inner.flush()
            }
        }
    }
    impl Seek for FailingWriter {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(pos)
        }
    }
    let mut pos = 8;
    let idat_middle = loop {
        let len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        if &data[pos + 4..pos + 8] == b"IDAT" {
            break pos + 8 + len / 2;
        }
        pos += len + 12;
    };
    for remaining in [16, idat_middle, data.len() - 4] {
        let mut writer = FailingWriter {
            inner: Cursor::new(Vec::new()),
            remaining,
            fail_flush: false,
        };
        let error = write(
            &mut writer,
            &packed,
            &params,
            TiffColorSpace::AdobeRgb,
            Format::Png,
            Default::default(),
            &metadata(),
            &mut Default::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("injected failure"));
    }
    let mut writer = FailingWriter {
        inner: Cursor::new(Vec::new()),
        remaining: usize::MAX,
        fail_flush: true,
    };
    let error = write(
        &mut writer,
        &packed,
        &params,
        TiffColorSpace::AdobeRgb,
        Format::Png,
        Default::default(),
        &metadata(),
        &mut Default::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("injected flush failure"));
}

#[test]
fn png_rgb8_quantizes_every_u16_value() {
    let params = EncodeParams {
        mapping: [[0.; 4]; 2],
        source: [256, 256, 0, 0],
        crop: [0, 0, 256, 256],
        output: [256, 256, 0, 0],
    };
    let packed: Vec<u8> = (0..=u16::MAX)
        .flat_map(|v| [v, v, v, 0].into_iter().flat_map(u16::to_le_bytes))
        .collect();
    let mut writer = Cursor::new(Vec::new());
    write(
        &mut writer,
        &packed,
        &params,
        TiffColorSpace::Srgb,
        Format::Png,
        EncodingOptions {
            png_depth: BitDepth::Eight,
            ..Default::default()
        },
        &metadata(),
        &mut Default::default(),
    )
    .unwrap();
    let decoded = image::load_from_memory(&writer.into_inner())
        .unwrap()
        .into_rgb8();
    for (value, pixel) in (0..=u16::MAX).zip(decoded.pixels()) {
        let expected = ((u32::from(value) + 128) / 257) as u8;
        assert_eq!(pixel.0, [expected; 3]);
    }
}

#[test]
fn jpeg_is_readable_rgb8_with_the_requested_profile() {
    use image::ImageDecoder;
    for space in TiffColorSpace::ALL {
        let data = encoded(Format::Jpeg, Default::default(), space);
        let mut decoder = image::codecs::jpeg::JpegDecoder::new(Cursor::new(&data)).unwrap();
        assert_eq!(decoder.dimensions(), (17, 9));
        assert_eq!(decoder.color_type(), image::ColorType::Rgb8);
        assert_eq!(decoder.icc_profile().unwrap().unwrap(), space.profile());
        assert_eq!(
            image::load_from_memory(&data).unwrap().into_rgb8().len(),
            17 * 9 * 3
        );
    }
}

struct Decoder(*mut jxl_sys::JxlDecoder);
impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { jxl_sys::JxlDecoderDestroy(self.0) }
    }
}
pub(crate) struct Decoded {
    pub pixels: Vec<u16>,
    pub profile: Vec<u8>,
    pub color: Option<jxl_sys::JxlColorEncoding>,
}

pub(crate) fn decode_jxl(bytes: &[u8]) -> Decoded {
    use jxl_sys::*;
    let decoder = Decoder(unsafe { JxlDecoderCreate(std::ptr::null()) });
    assert!(!decoder.0.is_null());
    let dec = decoder.0;
    let mut pixels = Vec::<u16>::new();
    let mut original_profile = true;
    let mut color = None;
    let mut icc = Vec::new();
    unsafe {
        assert!(matches!(
            JxlDecoderSubscribeEvents(
                dec,
                JxlDecoderStatus::JXL_DEC_BASIC_INFO as i32
                    | JxlDecoderStatus::JXL_DEC_COLOR_ENCODING as i32
                    | JxlDecoderStatus::JXL_DEC_FULL_IMAGE as i32
            ),
            JxlDecoderStatus::JXL_DEC_SUCCESS
        ));
        assert!(matches!(
            JxlDecoderSetInput(dec, bytes.as_ptr(), bytes.len()),
            JxlDecoderStatus::JXL_DEC_SUCCESS
        ));
        JxlDecoderCloseInput(dec);
        loop {
            match JxlDecoderProcessInput(dec) {
                JxlDecoderStatus::JXL_DEC_BASIC_INFO => {
                    let mut info = JxlBasicInfo::default();
                    assert!(matches!(
                        JxlDecoderGetBasicInfo(dec, &mut info),
                        JxlDecoderStatus::JXL_DEC_SUCCESS
                    ));
                    original_profile = info.uses_original_profile != 0;
                    pixels.resize(info.xsize as usize * info.ysize as usize * 3, 0);
                }
                JxlDecoderStatus::JXL_DEC_COLOR_ENCODING => {
                    let target = JxlColorProfileTarget::JXL_COLOR_PROFILE_TARGET_DATA;
                    if !original_profile {
                        let mut native = JxlColorEncoding::default();
                        assert!(matches!(
                            JxlDecoderGetColorAsEncodedProfile(dec, target, &mut native),
                            JxlDecoderStatus::JXL_DEC_SUCCESS
                        ));
                        color = Some(native);
                    }
                    let mut size = 0;
                    assert!(matches!(
                        JxlDecoderGetICCProfileSize(dec, target, &mut size),
                        JxlDecoderStatus::JXL_DEC_SUCCESS
                    ));
                    icc.resize(size, 0);
                    assert!(matches!(
                        JxlDecoderGetColorAsICCProfile(dec, target, icc.as_mut_ptr(), size),
                        JxlDecoderStatus::JXL_DEC_SUCCESS
                    ));
                }
                JxlDecoderStatus::JXL_DEC_NEED_IMAGE_OUT_BUFFER => {
                    let format = JxlPixelFormat {
                        num_channels: 3,
                        data_type: JxlDataType::JXL_TYPE_UINT16,
                        endianness: JxlEndianness::JXL_NATIVE_ENDIAN,
                        align: 0,
                    };
                    assert!(matches!(
                        JxlDecoderSetImageOutBuffer(
                            dec,
                            &format,
                            pixels.as_mut_ptr().cast(),
                            pixels.len() * 2
                        ),
                        JxlDecoderStatus::JXL_DEC_SUCCESS
                    ));
                }
                JxlDecoderStatus::JXL_DEC_FULL_IMAGE => (),
                JxlDecoderStatus::JXL_DEC_SUCCESS => break,
                status => panic!("unexpected JPEG XL decoder state: {status:?}"),
            }
        }
    }
    Decoded {
        pixels,
        profile: icc,
        color,
    }
}

#[test]
fn jxl_lossless_preserves_every_rgb16_sample_at_fast_and_archive_efforts() {
    for space in TiffColorSpace::ALL {
        for effort in [1, 7, 11] {
            let data = encoded(
                Format::Jxl,
                EncodingOptions {
                    jxl_effort: effort,
                    ..Default::default()
                },
                space,
            );
            let Decoded {
                pixels,
                profile: icc,
                ..
            } = decode_jxl(&data);
            assert_eq!(pixels, fixture().2);
            assert_eq!(icc, space.profile());
        }
    }
}

#[test]
fn jxl_lossy_is_readable_and_retains_the_destination_profile() {
    for space in TiffColorSpace::ALL {
        let data = encoded(
            Format::Jxl,
            EncodingOptions {
                jxl_lossless: false,
                jxl_effort: 1,
                ..Default::default()
            },
            space,
        );
        let Decoded {
            pixels,
            profile: icc,
            ..
        } = decode_jxl(&data);
        assert_eq!(pixels.len(), fixture().2.len());
        let original = moxcms::ColorProfile::new_from_slice(&icc).unwrap();
        let expected = moxcms::ColorProfile::new_from_slice(space.profile()).unwrap();
        let transform = original
            .create_transform_16bit(
                moxcms::Layout::Rgb,
                &expected,
                moxcms::Layout::Rgb,
                Default::default(),
            )
            .unwrap();
        let samples = [
            0, 0, 0, 65535, 65535, 65535, 10000, 20000, 30000, 55000, 15000, 5000,
        ];
        let mut converted = [0; 12];
        transform.transform(&samples, &mut converted).unwrap();
        for (&actual, expected) in converted.iter().zip(samples) {
            assert!(
                actual.abs_diff(expected) <= 128,
                "{space:?}: profile transform changed {expected} to {actual}"
            );
        }
    }
}

#[test]
fn invalid_options_and_dimensions_do_not_write_output() {
    let (params, bytes, _) = fixture();
    for (format, options, packed) in [
        (Format::Dng, EncodingOptions::default(), bytes.as_slice()),
        (
            Format::Jpeg,
            EncodingOptions {
                jpeg_quality: 0,
                ..Default::default()
            },
            bytes.as_slice(),
        ),
        (
            Format::Jxl,
            EncodingOptions {
                jxl_effort: 12,
                ..Default::default()
            },
            bytes.as_slice(),
        ),
        (
            Format::Png,
            EncodingOptions::default(),
            &bytes[..bytes.len() - 1],
        ),
    ] {
        let mut writer = Cursor::new(Vec::new());
        assert!(write(
            &mut writer,
            packed,
            &params,
            TiffColorSpace::Srgb,
            format,
            options,
            &metadata(),
            &mut Default::default()
        )
        .is_err());
        assert!(writer.into_inner().is_empty());
    }
}

#[test]
fn lossy_jxl_preserves_wide_rgb_values_and_native_destination_metadata() {
    for space in TiffColorSpace::ALL {
        let rgb = [6000_u16, 40000, 55000];
        let bytes: Vec<_> = [rgb[0], rgb[1], rgb[2], 0]
            .into_iter()
            .cycle()
            .take(64 * 64 * 4)
            .flat_map(u16::to_le_bytes)
            .collect();
        let params = EncodeParams {
            mapping: [[0.; 4]; 2],
            source: [64, 64, 0, 0],
            crop: [0, 0, 64, 64],
            output: [64, 64, space.encoding_code(true), 0],
        };
        let mut writer = Cursor::new(Vec::new());
        write(
            &mut writer,
            &bytes,
            &params,
            space,
            Format::Jxl,
            EncodingOptions {
                jxl_lossless: false,
                jxl_quality: 99,
                jxl_effort: 1,
                ..Default::default()
            },
            &metadata(),
            &mut Default::default(),
        )
        .unwrap();
        let decoded = decode_jxl(writer.get_ref());
        let color = decoded.color.unwrap();
        match space {
            TiffColorSpace::Srgb => assert!(matches!(
                color.transfer_function,
                jxl_sys::JxlTransferFunction::JXL_TRANSFER_FUNCTION_SRGB
            )),
            TiffColorSpace::AdobeRgb | TiffColorSpace::ProPhotoRgb => {
                assert!(matches!(
                    color.primaries,
                    jxl_sys::JxlPrimaries::JXL_PRIMARIES_CUSTOM
                ));
                assert!(matches!(
                    color.transfer_function,
                    jxl_sys::JxlTransferFunction::JXL_TRANSFER_FUNCTION_GAMMA
                ));
                let gamma = if space == TiffColorSpace::AdobeRgb {
                    2.2
                } else {
                    1.8
                };
                assert!((color.gamma - 1. / gamma).abs() < 1e-6);
            }
        }
        for (i, value) in decoded.pixels.iter().enumerate() {
            assert!(
                value.abs_diff(rgb[i % 3]) < 128,
                "{space:?} component {i}: {value} instead of {}",
                rgb[i % 3]
            );
        }
    }
}

#[test]
fn maximum_jxl_quality_uses_the_exact_lossless_profile_and_samples() {
    for space in TiffColorSpace::ALL {
        let bytes = encoded(
            Format::Jxl,
            EncodingOptions {
                jxl_lossless: false,
                jxl_quality: 100,
                jxl_effort: 1,
                ..Default::default()
            },
            space,
        );
        let decoded = decode_jxl(&bytes);
        assert!(decoded.color.is_none());
        assert_eq!(decoded.pixels, fixture().2);
        assert_eq!(decoded.profile, space.profile());
    }
}

// These tiny, repository-generated captures exercise the real encoders and a
// separate TIFF directory reader without depending on camera files or tools.
fn metadata_with_embedded_tags() -> crate::source::Metadata {
    use crate::capture_metadata::EmbeddedTag;
    let tags = vec![
        EmbeddedTag {
            ifd: 1,
            id: 0x9209,
            kind: 3,
            count: 1,
            data: 1_u16.to_ne_bytes().to_vec(),
        },
        EmbeddedTag {
            ifd: 1,
            id: 0x9004,
            kind: 2,
            count: 20,
            data: b"2020:01:02 03:04:05\0".to_vec(),
        },
        EmbeddedTag {
            ifd: 2,
            id: 1,
            kind: 2,
            count: 2,
            data: b"N\0".to_vec(),
        },
        EmbeddedTag {
            ifd: 2,
            id: 2,
            kind: 5,
            count: 3,
            data: [51_u32, 1, 30, 1, 0, 1]
                .into_iter()
                .flat_map(u32::to_ne_bytes)
                .collect(),
        },
    ];
    let mut m = metadata();
    m.properties.insert(
        "EXIF_CAPTURE_TAGS".into(),
        serde_json::to_string(&tags).unwrap(),
    );
    m.properties.insert("TIME".into(), "1790791918".into());
    m.properties
        .insert("ARTIST".into(), "A Photographer".into());
    m
}

fn tiff_tag(bytes: &[u8], ifd: usize, tag: u16) -> Option<Vec<u8>> {
    let short =
        |p| -> Option<u16> { Some(u16::from_le_bytes(bytes.get(p..p + 2)?.try_into().ok()?)) };
    let long =
        |p| -> Option<u32> { Some(u32::from_le_bytes(bytes.get(p..p + 4)?.try_into().ok()?)) };
    let count: u16 = short(ifd)?;
    for i in 0..usize::from(count) {
        let p = ifd + 2 + i * 12;
        if short(p)? != tag {
            continue;
        }
        let kind = short(p + 2)?;
        let count: u32 = long(p + 4)?;
        let unit = match kind {
            1 | 2 | 7 => 1,
            3 => 2,
            4 | 9 => 4,
            5 | 10 => 8,
            _ => return None,
        };
        let size = usize::try_from(count).ok()?.checked_mul(unit)?;
        let start = if size <= 4 {
            p + 8
        } else {
            long(p + 8)? as usize
        };
        return Some(bytes.get(start..start.checked_add(size)?)?.to_vec());
    }
    None
}

fn check_capture_tiff(bytes: &[u8]) {
    assert_eq!(&bytes[..4], b"II\x2a\0");
    let u32v = |v: Vec<u8>| u32::from_le_bytes(v.try_into().unwrap());
    let root = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    assert_eq!(tiff_tag(bytes, root, 272).unwrap(), b"SIGMA DP2 Merrill\0");
    assert_eq!(tiff_tag(bytes, root, 315).unwrap(), b"A Photographer\0");
    assert_eq!(tiff_tag(bytes, root, 274).unwrap(), 1_u16.to_le_bytes());
    let exif = u32v(tiff_tag(bytes, root, 0x8769).unwrap()) as usize;
    assert_eq!(
        tiff_tag(bytes, exif, 0x8827).unwrap(),
        200_u16.to_le_bytes()
    );
    assert_eq!(tiff_tag(bytes, exif, 0x9209).unwrap(), 1_u16.to_le_bytes());
    assert_eq!(
        tiff_tag(bytes, exif, 0x9004).unwrap(),
        b"2020:01:02 03:04:05\0"
    );
    assert_eq!(u32v(tiff_tag(bytes, exif, 0xa002).unwrap()), 17);
    assert_eq!(u32v(tiff_tag(bytes, exif, 0xa003).unwrap()), 9);
    assert!(tiff_tag(bytes, exif, 0x927c).is_none());
    let gps = u32v(tiff_tag(bytes, root, 0x8825).unwrap()) as usize;
    assert_eq!(tiff_tag(bytes, gps, 1).unwrap(), b"N\0");
    assert_eq!(tiff_tag(bytes, gps, 2).unwrap().len(), 24);
}

fn encode_capture(format: Format) -> Vec<u8> {
    let (params, bytes, _) = fixture();
    let mut writer = Cursor::new(Vec::new());
    write(
        &mut writer,
        &bytes,
        &params,
        TiffColorSpace::Srgb,
        format,
        EncodingOptions::default(),
        &metadata_with_embedded_tags(),
        &mut Default::default(),
    )
    .unwrap();
    writer.into_inner()
}

#[test]
fn tiff_exports_capture_metadata() {
    check_capture_tiff(&encode_capture(Format::Tiff));
}

#[test]
fn jpeg_exports_capture_metadata() {
    use image::ImageDecoder;
    let bytes = encode_capture(Format::Jpeg);
    let mut decoder = image::codecs::jpeg::JpegDecoder::new(Cursor::new(bytes)).unwrap();
    check_capture_tiff(&decoder.exif_metadata().unwrap().unwrap());
}

#[test]
fn png_exports_capture_metadata() {
    use image::ImageDecoder;
    let bytes = encode_capture(Format::Png);
    let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes)).unwrap();
    check_capture_tiff(&decoder.exif_metadata().unwrap().unwrap());
}

#[test]
fn jxl_exports_capture_metadata() {
    let bytes = encode_capture(Format::Jxl);
    let mut offset = 0;
    while offset + 8 <= bytes.len() {
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        assert!(size >= 8 && offset + size <= bytes.len());
        if &bytes[offset + 4..offset + 8] == b"Exif" {
            assert_eq!(&bytes[offset + 8..offset + 12], &[0; 4]);
            check_capture_tiff(&bytes[offset + 12..offset + size]);
            return;
        }
        offset += size;
    }
    panic!("missing JPEG XL Exif box");
}

#[test]
fn jpeg_rejects_oversized_exif_without_writing_a_corrupt_segment() {
    let mut metadata = metadata();
    let entry = crate::capture_metadata::EmbeddedTag {
        ifd: 1,
        id: 0x9286,
        kind: 7,
        count: 65_500,
        data: vec![42; 65_500],
    };
    metadata.properties.insert(
        "EXIF_CAPTURE_TAGS".into(),
        serde_json::to_string(&[entry]).unwrap(),
    );
    let (params, pixels, _) = fixture();
    let mut output = Cursor::new(Vec::new());
    assert!(write(
        &mut output,
        &pixels,
        &params,
        TiffColorSpace::Srgb,
        Format::Jpeg,
        EncodingOptions::default(),
        &metadata,
        &mut Default::default()
    )
    .is_err());
    assert!(output.into_inner().is_empty());
}

#[test]
fn x3f_embedded_capture_reaches_every_photographic_encoder() {
    use image::ImageDecoder;
    let original = crate::decode_cache::tests::small_x3f_with_exif();
    let properties = crate::x3f::capture::read_for_export(&mut Cursor::new(original)).unwrap();
    assert!(properties.contains_key("EXIF_CAPTURE_TAGS"));
    let mut metadata = metadata();
    metadata.properties = properties;
    for format in [Format::Tiff, Format::Jpeg, Format::Png, Format::Jxl] {
        let (params, packed, _) = fixture();
        let mut file = Cursor::new(Vec::new());
        write(
            &mut file,
            &packed,
            &params,
            TiffColorSpace::Srgb,
            format,
            EncodingOptions::default(),
            &metadata,
            &mut Default::default(),
        )
        .unwrap();
        let file = file.into_inner();
        let bytes = match format {
            Format::Tiff => file,
            Format::Jpeg => image::codecs::jpeg::JpegDecoder::new(Cursor::new(file))
                .unwrap()
                .exif_metadata()
                .unwrap()
                .unwrap(),
            Format::Png => image::codecs::png::PngDecoder::new(Cursor::new(file))
                .unwrap()
                .exif_metadata()
                .unwrap()
                .unwrap(),
            Format::Jxl => {
                let mut offset = 0;
                loop {
                    let size =
                        u32::from_be_bytes(file[offset..offset + 4].try_into().unwrap()) as usize;
                    if &file[offset + 4..offset + 8] == b"Exif" {
                        break file[offset + 12..offset + size].to_vec();
                    }
                    assert!(size >= 8);
                    offset += size;
                }
            }
            Format::Dng => unreachable!(),
        };
        let root = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(tiff_tag(&bytes, root, 272).unwrap(), b"SIGMA dp0 Quattro\0");
        let exif = u32::from_le_bytes(tiff_tag(&bytes, root, 0x8769).unwrap().try_into().unwrap())
            as usize;
        assert_eq!(
            tiff_tag(&bytes, exif, 0x8827).unwrap(),
            400_u16.to_le_bytes()
        );
        assert_eq!(
            tiff_tag(&bytes, exif, 0xa002).unwrap(),
            17_u32.to_le_bytes()
        );
    }
}

#[test]
fn jxl_streams_multiple_chunks_and_propagates_writer_errors() {
    struct Sink {
        bytes: Vec<u8>,
        writes: usize,
        fail: bool,
    }
    impl std::io::Write for Sink {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            if self.fail {
                return Err(std::io::Error::other("injected failure"));
            }
            self.writes += 1;
            self.bytes.extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut state = 1_u32;
    let pixels: Vec<u16> = (0..256 * 256 * 3)
        .map(|_| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state >> 16) as u16
        })
        .collect();
    let options = EncodingOptions {
        jxl_effort: 1,
        ..Default::default()
    };
    let mut sink = Sink {
        bytes: Vec::new(),
        writes: 0,
        fail: false,
    };
    jxl::encode(
        &mut sink,
        &pixels,
        256,
        256,
        TiffColorSpace::Srgb,
        options,
        &[],
    )
    .unwrap();
    assert!(sink.writes > 1);
    assert_eq!(decode_jxl(&sink.bytes).pixels, pixels);
    let mut failing = Sink {
        bytes: Vec::new(),
        writes: 0,
        fail: true,
    };
    let error = jxl::encode(
        &mut failing,
        &pixels,
        256,
        256,
        TiffColorSpace::Srgb,
        options,
        &[],
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "injected failure");
}
