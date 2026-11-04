//! Checks encoded precision, color profiles, compression tags and rejected output requests.

use super::*;
use std::io::Cursor;

fn fixture() -> (EncodeParams, Vec<u8>, Vec<u16>) {
    let params = EncodeParams {
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
