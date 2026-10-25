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
        let exif = capture.write_exif(&mut encoder, [2, 1]).unwrap();
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
        decoder
            .get_tag_ascii_string(Tag::Unknown(CAMERA_SERIAL_NUMBER))
            .unwrap(),
        "90406890"
    );
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
