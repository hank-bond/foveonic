//! Checks capture EXIF through an independent TIFF reader, including sparse and invalid input.

use super::*;
use std::io::Cursor;
use tiff::{
    decoder::{ifd::Value, Decoder, DecodingResult},
    encoder::colortype::RGB16,
    tags::{IfdPointer, Tag},
};

fn rational(value: Value) -> f64 {
    match value {
        Value::Rational(n, d) => f64::from(n) / f64::from(d),
        Value::SRational(n, d) => f64::from(n) / f64::from(d),
        other => panic!("expected EXIF rational, got {other:?}"),
    }
}

fn encoded(properties: &[(&str, &str)]) -> Decoder<Cursor<Vec<u8>>> {
    let metadata = Metadata {
        width: 1,
        height: 2,
        rotation: 90,
        processing_version: None,
        properties: properties
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        matrices: BTreeMap::new(),
        lists: BTreeMap::new(),
    };
    let capture = Capture::new(&metadata);
    let mut output = Cursor::new(Vec::new());
    {
        let mut encoder = TiffEncoder::new(&mut output).unwrap();
        let exif = capture
            .write_exif(
                &mut encoder,
                [2, 1],
                ExifOutput::Embedded(TiffColorSpace::Srgb),
            )
            .unwrap();
        // RGB permits independent decoding of the same directory-writing helpers.
        let mut image = encoder.new_image::<RGB16>(2, 1).unwrap();
        capture.write_primary(image.encoder(), exif).unwrap();
        tag(image.encoder(), 274, 1_u16).unwrap();
        image.write_data(&[1, 2, 3, 4, 5, u16::MAX]).unwrap();
    }
    output.set_position(0);
    Decoder::new(output).unwrap()
}

#[test]
fn capture_exif_roundtrips_without_changing_image_or_orientation() {
    let mut decoder = encoded(&[
        ("TIME", "1790791918"),
        ("SH_DESC", "1/100"),
        ("SHUTTER", "0.01013"),
        ("AP_DESC", "3.2"),
        ("APERTURE", "3.22098"),
        ("ISO", "200"),
        ("FLENGTH", "30"),
        ("FLEQ35MM", "45"),
        ("EXPCOMP", "+0.30"),
        ("PMODE", "A"),
        ("AEMODE", "8"),
        ("CAMSERIAL", "90406890"),
        ("LENSMODEL", "1004"),
        ("ARTIST", "Photographer"),
        ("COPYRIGHT", "Copyright Photographer"),
    ]);
    assert_eq!(decoder.dimensions().unwrap(), (2, 1));
    assert_eq!(decoder.get_tag_u32(Tag::Orientation).unwrap(), 1);
    assert_eq!(
        decoder.get_tag_ascii_string(Tag::Artist).unwrap(),
        "Photographer"
    );
    assert_eq!(
        decoder.get_tag_ascii_string(Tag::Copyright).unwrap(),
        "Copyright Photographer"
    );
    assert_eq!(
        decoder.get_tag_ascii_string(Tag::DateTime).unwrap(),
        "2026:09:30 18:11:58"
    );
    assert!(!decoder.more_images(), "EXIF must not become another image");
    let DecodingResult::U16(samples) = decoder.read_image().unwrap() else {
        panic!("RGB16 expected")
    };
    assert_eq!(samples, [1, 2, 3, 4, 5, u16::MAX]);
    let pointer = decoder.get_tag_u32(Tag::Unknown(EXIF_IFD)).unwrap();
    let directory = decoder.read_directory(IfdPointer(pointer.into())).unwrap();
    let mut exif = decoder.read_directory_tags(&directory);
    assert_eq!(
        exif.get_tag_u8_vec(Tag::Unknown(EXIF_VERSION)).unwrap(),
        b"0230"
    );
    for id in [DATE_TIME_ORIGINAL, DATE_TIME_DIGITIZED] {
        assert_eq!(
            exif.get_tag_ascii_string(Tag::Unknown(id)).unwrap(),
            "2026:09:30 18:11:58"
        );
    }
    for (id, expected) in [
        (EXPOSURE_TIME, 0.01),
        (F_NUMBER, 3.2),
        (FOCAL_LENGTH, 30.),
        (EXPOSURE_BIAS, 0.3),
    ] {
        assert!((rational(exif.get_tag(Tag::Unknown(id)).unwrap()) - expected).abs() < 1e-6);
    }
    for (id, expected) in [
        (ISO_SPEED_RATINGS, 200),
        (ISO_SPEED, 200),
        (SENSITIVITY_TYPE, 3),
        (EXPOSURE_PROGRAM, 3),
        (METERING_MODE, 5),
        (FOCAL_LENGTH_35MM, 45),
        (PIXEL_X_DIMENSION, 2),
        (PIXEL_Y_DIMENSION, 1),
    ] {
        assert_eq!(exif.get_tag_u32(Tag::Unknown(id)).unwrap(), expected);
    }
    assert_eq!(
        exif.get_tag_ascii_string(Tag::Unknown(LENS_MODEL)).unwrap(),
        "1004"
    );
    assert_eq!(
        exif.get_tag_ascii_string(Tag::Unknown(BODY_SERIAL_NUMBER))
            .unwrap(),
        "90406890"
    );
}

#[test]
fn sparse_and_malformed_properties_use_available_fallbacks() {
    for (properties, date, shutter, aperture) in [
        (
            vec![
                ("EXIF_DATETIME", "2018:04:09 11:49:15"),
                ("EXIF_FNUMBER", "2.8"),
                ("EXPTIME", "250"),
            ],
            "2018:04:09 11:49:15",
            0.00025,
            2.8,
        ),
        (
            vec![
                ("TIME", "1265012852"),
                ("SHUTTER", "0.016666"),
                ("APERTURE", "4"),
            ],
            "2010:02:01 08:27:32",
            0.016666,
            4.,
        ),
        (
            vec![
                ("TIME", "broken"),
                ("EXIF_DATETIME", "2000:02:29 12:34:56"),
                ("SH_DESC", "1/0"),
                ("SHUTTER", "-1"),
                ("EXPTIME", "2000000"),
                ("EXIF_FNUMBER", "0"),
                ("AP_DESC", "NaN"),
                ("APERTURE", "8"),
            ],
            "2000:02:29 12:34:56",
            2.,
            8.,
        ),
    ] {
        let mut decoder = encoded(&properties);
        let pointer = decoder.get_tag_u32(Tag::Unknown(EXIF_IFD)).unwrap();
        let directory = decoder.read_directory(IfdPointer(pointer.into())).unwrap();
        let mut exif = decoder.read_directory_tags(&directory);
        assert_eq!(
            exif.get_tag_ascii_string(Tag::Unknown(DATE_TIME_ORIGINAL))
                .unwrap(),
            date
        );
        assert!(
            (rational(exif.get_tag(Tag::Unknown(EXPOSURE_TIME)).unwrap()) - shutter).abs() < 1e-6
        );
        assert!((rational(exif.get_tag(Tag::Unknown(F_NUMBER)).unwrap()) - aperture).abs() < 1e-6);
    }
    let mut decoder = encoded(&[
        ("TIME", "-1"),
        ("EXIF_DATETIME", "2100:02:29 00:00:00"),
        ("ISO", "NaN"),
        ("FLENGTH", "-30"),
        ("CAMSERIAL", "bad\0serial"),
        ("EXPCOMP", "inf"),
    ]);
    let pointer = decoder.get_tag_u32(Tag::Unknown(EXIF_IFD)).unwrap();
    let directory = decoder.read_directory(IfdPointer(pointer.into())).unwrap();
    let mut exif = decoder.read_directory_tags(&directory);
    for id in [
        DATE_TIME_ORIGINAL,
        ISO_SPEED_RATINGS,
        FOCAL_LENGTH,
        BODY_SERIAL_NUMBER,
        EXPOSURE_BIAS,
        LENS_MODEL,
    ] {
        assert!(exif.find_tag(Tag::Unknown(id)).unwrap().is_none());
    }
}

#[test]
fn camera_dates_preserve_wall_clock_and_calendar_boundaries() {
    for (timestamp, expected) in [
        (0, "1970:01:01 00:00:00"),
        (951782400, "2000:02:29 00:00:00"),
        (4107542400, "2100:03:01 00:00:00"),
        (MAX_TIMESTAMP, "9999:12:31 23:59:59"),
    ] {
        assert_eq!(capture_date(timestamp).as_deref(), Some(expected));
        assert!(valid_date(expected));
    }
    assert!(capture_date(MAX_TIMESTAMP + 1).is_none());
    for value in [
        "2026:09:31 00:00:00",
        "2026:02:29 00:00:00",
        "2026:01:01 24:00:00",
        "2026:01:01 00:00:60",
        "0000:01:01 00:00:00",
        "2026:01:01 00:00:00Z",
        "2026-01-01 00:00:00",
    ] {
        assert!(!valid_date(value), "{value}");
    }
}

#[test]
fn rejects_malformed_embedded_capture_fields() {
    let valid = EmbeddedTag {
        ifd: 1,
        id: 0x829a,
        kind: 5,
        count: 1,
        data: [1_u32, 100]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect(),
    };
    assert!(valid.valid());
    let mut invalid = valid.clone();
    invalid.data[4..8].fill(0);
    assert!(!invalid.valid());
    invalid.kind = 2;
    assert!(!invalid.valid());
    let invalid_gps = EmbeddedTag {
        ifd: 2,
        id: 2,
        kind: 3,
        count: 1,
        data: 1_u16.to_ne_bytes().to_vec(),
    };
    assert!(!invalid_gps.valid());
    let invalid_date = EmbeddedTag {
        ifd: 1,
        id: 0x9003,
        kind: 2,
        count: 20,
        data: b"2026:02:30 12:00:00\0".to_vec(),
    };
    assert!(!invalid_date.valid());
    for id in [0x9214, 0xa214, 0xa302] {
        let entry = EmbeddedTag {
            ifd: 1,
            id,
            kind: 3,
            count: 1,
            data: 1_u16.to_ne_bytes().to_vec(),
        };
        assert!(!entry.valid());
    }
}

#[test]
fn embedded_extended_iso_is_not_replaced_by_its_sentinel() {
    let embedded = [
        EmbeddedTag {
            ifd: 1,
            id: 0x8827,
            kind: 3,
            count: 1,
            data: 65535_u16.to_ne_bytes().to_vec(),
        },
        EmbeddedTag {
            ifd: 1,
            id: 0x8830,
            kind: 3,
            count: 1,
            data: 3_u16.to_ne_bytes().to_vec(),
        },
        EmbeddedTag {
            ifd: 1,
            id: 0x8833,
            kind: 4,
            count: 1,
            data: 102400_u32.to_ne_bytes().to_vec(),
        },
    ];
    let encoded_tags = serde_json::to_string(&embedded).unwrap();
    let mut decoder = encoded(&[
        ("ISO", "65535"),
        ("EXIF_PREVIEW_ISO", "1"),
        ("EXIF_CAPTURE_TAGS", &encoded_tags),
    ]);
    let pointer = decoder.get_tag_u32(Tag::Unknown(EXIF_IFD)).unwrap();
    let directory = decoder.read_directory(IfdPointer(pointer.into())).unwrap();
    let mut exif = decoder.read_directory_tags(&directory);
    assert_eq!(
        exif.get_tag_u32(Tag::Unknown(ISO_SPEED_RATINGS)).unwrap(),
        65535
    );
    assert_eq!(exif.get_tag_u32(Tag::Unknown(ISO_SPEED)).unwrap(), 102400);
}

#[test]
fn exif_version_is_validated_and_never_downgraded() {
    for (version, firmware, expected) in [
        (*b"xxxx", "", *b"0230"),
        (*b"0220", "", *b"0230"),
        (*b"0232", "", *b"0232"),
        (*b"0232", "1.2", *b"0300"),
        (*b"0310", "1.2", *b"0310"),
    ] {
        let tags = [EmbeddedTag {
            ifd: 1,
            id: EXIF_VERSION,
            kind: 7,
            count: 4,
            data: version.to_vec(),
        }];
        let json = serde_json::to_string(&tags).unwrap();
        let mut decoder = encoded(&[("EXIF_CAPTURE_TAGS", &json), ("FIRMVERS", firmware)]);
        let pointer = decoder.get_tag_u32(Tag::Unknown(EXIF_IFD)).unwrap();
        let directory = decoder.read_directory(IfdPointer(pointer.into())).unwrap();
        let mut exif = decoder.read_directory_tags(&directory);
        assert_eq!(
            exif.get_tag_u8_vec(Tag::Unknown(EXIF_VERSION)).unwrap(),
            expected
        );
    }
}

#[test]
fn capture_rationals_keep_simple_camera_fractions() {
    for (value, n, d) in [
        (1. / 250., 1, 250),
        (1. / 30., 1, 30),
        (1.3, 13, 10),
        (4., 4, 1),
        (6.3, 63, 10),
        (16.6, 83, 5),
        (0.016666, 8333, 500000),
    ] {
        let value = unsigned_rationals(&[value]).unwrap().remove(0);
        assert_eq!((value.n, value.d), (n, d));
    }
    for (value, n, d) in [(-0.3, -3, 10), (0., 0, 1), (1. / 3., 1, 3)] {
        let value = signed_rationals(&[value]).unwrap().remove(0);
        assert_eq!((value.n, value.d), (n, d));
    }
    for value in [
        0.,
        -1.,
        f64::NAN,
        f64::INFINITY,
        1e-30,
        u32::MAX as f64 + 1.,
    ] {
        assert!(unsigned_rationals(&[value]).is_err());
    }
}

#[test]
fn bounded_fractions_match_an_independent_exhaustive_oracle() {
    for max_n in 1..=12 {
        for max_d in 1..=12 {
            for k in 0..=max_n * 17 {
                let value = f64::from(k) / 17.;
                let (n, d) = fraction(value, max_n, max_d);
                assert!(n <= max_n && (1..=max_d).contains(&d));
                assert_eq!(gcd(n, d), 1);
                let error = (f64::from(n) / f64::from(d) - value).abs();
                for reference_n in 0..=max_n {
                    for reference_d in 1..=max_d {
                        let reference_error =
                            (f64::from(reference_n) / f64::from(reference_d) - value).abs();
                        assert!(error <= reference_error + f64::EPSILON * value.max(1.) * 4., "{value} bounds {max_n}/{max_d}: {n}/{d} versus {reference_n}/{reference_d}");
                    }
                }
            }
        }
    }
}

#[test]
fn standard_capture_values_take_precedence_over_conflicting_properties() {
    let tags = [
        EmbeddedTag {
            ifd: 1,
            id: EXPOSURE_TIME,
            kind: 5,
            count: 1,
            data: [2_u32, 60].into_iter().flat_map(u32::to_ne_bytes).collect(),
        },
        EmbeddedTag {
            ifd: 1,
            id: EXPOSURE_BIAS,
            kind: 10,
            count: 1,
            data: [-2_i32, 6].into_iter().flat_map(i32::to_ne_bytes).collect(),
        },
        EmbeddedTag {
            ifd: 1,
            id: METERING_MODE,
            kind: 3,
            count: 1,
            data: 3_u16.to_ne_bytes().to_vec(),
        },
        EmbeddedTag {
            ifd: 1,
            id: LENS_MODEL,
            kind: 2,
            count: 10,
            data: b"30mm F2.8\0".to_vec(),
        },
    ];
    let json = serde_json::to_string(&tags).unwrap();
    let mut decoder = encoded(&[
        ("EXIF_CAPTURE_TAGS", &json),
        ("SH_DESC", "1/250"),
        ("EXPCOMP", "0.3"),
        ("AEMODE", "C"),
        ("LENSMODEL", "1007"),
    ]);
    let pointer = decoder.get_tag_u32(Tag::Unknown(EXIF_IFD)).unwrap();
    let directory = decoder.read_directory(IfdPointer(pointer.into())).unwrap();
    let mut exif = decoder.read_directory_tags(&directory);
    assert!(matches!(
        exif.get_tag(Tag::Unknown(EXPOSURE_TIME)).unwrap(),
        Value::Rational(1, 30)
    ));
    assert!(matches!(
        exif.get_tag(Tag::Unknown(EXPOSURE_BIAS)).unwrap(),
        Value::SRational(-1, 3)
    ));
    assert_eq!(exif.get_tag_u32(Tag::Unknown(METERING_MODE)).unwrap(), 3);
    assert_eq!(
        exif.get_tag_ascii_string(Tag::Unknown(LENS_MODEL)).unwrap(),
        "30mm F2.8"
    );
}

#[test]
fn source_comments_require_a_supported_encoding_prefix() {
    let valid = |data: Vec<u8>| {
        EmbeddedTag {
            ifd: 1,
            id: 0x9286,
            kind: 7,
            count: data.len(),
            data,
        }
        .valid()
    };
    assert!(valid(b"ASCII\0\0\0A valid comment\0".to_vec()));
    assert!(valid([b"UNICODE\0".as_slice(), &[65, 0, 0, 0]].concat()));
    assert!(valid([&[0; 8][..], b"unspecified"].concat()));
    assert!(!valid(b"ight\0\0\0\0Standard\0".to_vec()));
    assert!(!valid(b"ASCII\0\0\0\xff".to_vec()));
    assert!(!valid(b"UNICODE\0x".to_vec()));
    for body in [&[0xff, 0xfe, 0x00, 0xd8][..], &[0xfe, 0xff, 0xd8, 0x00][..]] {
        assert!(!valid([b"UNICODE\0".as_slice(), body].concat()));
    }
    assert!(valid(
        [b"UNICODE\0".as_slice(), &[0xfe, 0xff, 0, 65]].concat()
    ));
    assert!(valid(
        [b"UNICODE\0".as_slice(), &[0xff, 0xfe, 0x3d, 0xd8, 0, 0xde]].concat()
    ));
}

#[test]
fn malformed_standard_values_do_not_block_camera_fallbacks() {
    let tags = [
        EmbeddedTag {
            ifd: 1,
            id: EXPOSURE_TIME,
            kind: 5,
            count: 1,
            data: [0_u32, 1].into_iter().flat_map(u32::to_ne_bytes).collect(),
        },
        EmbeddedTag {
            ifd: 1,
            id: F_NUMBER,
            kind: 5,
            count: 1,
            data: [0_u32, 1].into_iter().flat_map(u32::to_ne_bytes).collect(),
        },
        EmbeddedTag {
            ifd: 1,
            id: ISO_SPEED_RATINGS,
            kind: 3,
            count: 1,
            data: 0_u16.to_ne_bytes().to_vec(),
        },
        EmbeddedTag {
            ifd: 1,
            id: LENS_MODEL,
            kind: 2,
            count: 1,
            data: vec![0],
        },
    ];
    let json = serde_json::to_string(&tags).unwrap();
    let mut decoder = encoded(&[
        ("EXIF_CAPTURE_TAGS", &json),
        ("SH_DESC", "1/250"),
        ("AP_DESC", "8"),
        ("ISO", "200"),
        ("LENSMODEL", "a known lens"),
    ]);
    let pointer = decoder.get_tag_u32(Tag::Unknown(EXIF_IFD)).unwrap();
    let directory = decoder.read_directory(IfdPointer(pointer.into())).unwrap();
    let mut exif = decoder.read_directory_tags(&directory);
    assert!(matches!(
        exif.get_tag(Tag::Unknown(EXPOSURE_TIME)).unwrap(),
        Value::Rational(1, 250)
    ));
    assert!(matches!(
        exif.get_tag(Tag::Unknown(F_NUMBER)).unwrap(),
        Value::Rational(8, 1)
    ));
    assert_eq!(
        exif.get_tag_u32(Tag::Unknown(ISO_SPEED_RATINGS)).unwrap(),
        200
    );
    assert_eq!(
        exif.get_tag_ascii_string(Tag::Unknown(LENS_MODEL)).unwrap(),
        "a known lens"
    );
}

#[test]
fn missing_iso_rating_uses_standard_speed_before_camera_fallback() {
    let tags = [
        EmbeddedTag {
            ifd: 1,
            id: ISO_SPEED,
            kind: 4,
            count: 1,
            data: 102400_u32.to_ne_bytes().to_vec(),
        },
        EmbeddedTag {
            ifd: 1,
            id: SENSITIVITY_TYPE,
            kind: 3,
            count: 1,
            data: 3_u16.to_ne_bytes().to_vec(),
        },
    ];
    let json = serde_json::to_string(&tags).unwrap();
    let mut decoder = encoded(&[("ISO", "200"), ("EXIF_CAPTURE_TAGS", &json)]);
    let pointer = decoder.get_tag_u32(Tag::Unknown(EXIF_IFD)).unwrap();
    let directory = decoder.read_directory(IfdPointer(pointer.into())).unwrap();
    let mut exif = decoder.read_directory_tags(&directory);
    assert_eq!(
        exif.get_tag_u32(Tag::Unknown(ISO_SPEED_RATINGS)).unwrap(),
        65535
    );
    assert_eq!(exif.get_tag_u32(Tag::Unknown(ISO_SPEED)).unwrap(), 102400);
    assert_eq!(exif.get_tag_u32(Tag::Unknown(SENSITIVITY_TYPE)).unwrap(), 3);
}

#[test]
fn unique_ids_remove_only_valid_trailing_padding() {
    let id = b"30313030313831340FE6DD4B31383132";
    for padding in [1, 2, 4] {
        let data = [id.as_slice(), vec![0; padding].as_slice()].concat();
        let tags = [EmbeddedTag {
            ifd: 1,
            id: 0xa420,
            kind: 2,
            count: data.len(),
            data,
        }];
        let json = serde_json::to_string(&tags).unwrap();
        let mut decoder = encoded(&[("EXIF_CAPTURE_TAGS", &json)]);
        let pointer = decoder.get_tag_u32(Tag::Unknown(EXIF_IFD)).unwrap();
        let directory = decoder.read_directory(IfdPointer(pointer.into())).unwrap();
        let mut exif = decoder.read_directory_tags(&directory);
        assert_eq!(
            exif.get_tag_ascii_string(Tag::Unknown(0xa420))
                .unwrap()
                .as_bytes(),
            id
        );
    }
    for data in [
        b"not a hex identifier\0".to_vec(),
        [id.as_slice(), b"X\0"].concat(),
        vec![b'x'; 33],
    ] {
        let mut tag = EmbeddedTag {
            ifd: 1,
            id: 0xa420,
            kind: 2,
            count: data.len(),
            data,
        };
        tag.normalize_unique_id();
        assert!(!tag.valid());
    }
}
