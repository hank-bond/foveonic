//! Checks EXIF storage and records reviewed validator warnings beside their exact acceptance rules.

use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path, process::Command};

// Read actual IFD types and stored fractions independently of Capture's writer.
fn find_entry(bytes: &[u8], directory: usize, wanted: u16) -> Option<(u16, u32, &[u8])> {
    assert_eq!(&bytes[..4], b"II\x2a\0");
    let u16_at = |p| u16::from_le_bytes(bytes[p..p + 2].try_into().unwrap());
    let u32_at = |p| u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
    assert!(
        directory.is_multiple_of(2),
        "TIFF directories must be word-aligned"
    );
    let count = u16_at(directory) as usize;
    assert!(directory + 2 + count * 12 + 4 <= bytes.len());
    for i in 0..count {
        let p = directory + 2 + i * 12;
        if u16_at(p) != wanted {
            continue;
        }
        let kind = u16_at(p + 2);
        let count = u32_at(p + 4);
        let unit = match kind {
            1 | 2 | 7 => 1,
            3 => 2,
            4 | 9 | 11 => 4,
            5 | 10 | 12 => 8,
            _ => panic!("unexpected TIFF type {kind}"),
        };
        let size = (count as usize).checked_mul(unit).unwrap();
        let start = if size <= 4 {
            p + 8
        } else {
            u32_at(p + 8) as usize
        };
        assert!(
            size <= 4 || start.is_multiple_of(2),
            "TIFF tag values must be word-aligned"
        );
        return Some((
            kind,
            count,
            bytes.get(start..start.checked_add(size).unwrap()).unwrap(),
        ));
    }
    None
}

fn entry(bytes: &[u8], directory: usize, wanted: u16) -> (u16, u32, &[u8]) {
    find_entry(bytes, directory, wanted).unwrap_or_else(|| panic!("missing TIFF tag {wanted:#x}"))
}

fn exif_bytes(path: &Path) -> Vec<u8> {
    use image::ImageDecoder;
    let bytes = fs::read(path).unwrap();
    match path.extension().unwrap().to_str().unwrap() {
        "tiff" | "dng" => bytes,
        "jpg" => image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes))
            .unwrap()
            .exif_metadata()
            .unwrap()
            .unwrap(),
        "png" => image::codecs::png::PngDecoder::new(std::io::Cursor::new(bytes))
            .unwrap()
            .exif_metadata()
            .unwrap()
            .unwrap(),
        "jxl" => {
            let mut p = 0;
            while p + 12 <= bytes.len() {
                let size = u32::from_be_bytes(bytes[p..p + 4].try_into().unwrap()) as usize;
                assert!(size >= 8 && p + size <= bytes.len());
                if &bytes[p + 4..p + 8] == b"Exif" {
                    assert_eq!(&bytes[p + 8..p + 12], &[0; 4]);
                    return bytes[p + 12..p + size].to_vec();
                }
                p += size;
            }
            panic!("missing JPEG XL EXIF box");
        }
        other => panic!("unsupported test container: {other}"),
    }
}

struct ReviewedWarning {
    id: &'static str,
    reason: &'static str,
}

// Keep the decision beside the exact warning so a search for the message finds
// its justification. DNG array types, counts and bounds are checked before this
// classifier is called. Any layout change requires those checks to pass again.
fn reviewed_warning(
    format: &str,
    compression: Option<u16>,
    warning: &str,
) -> Option<ReviewedWarning> {
    match (format, warning) {
        ("tiff", "Invalid value for IFD0 tag 0x0103 Compression") if compression == Some(8) => {
            // Adobe's Photoshop TIFF Technical Notes specify SHORT[1] Compression=8.
            // ExifTool's EXIF validation allows a narrower set of compression codes.
            Some(ReviewedWarning {
                id: "TIFF-DEFLATE-8",
                reason: "Adobe defines Compression=8 as valid TIFF Deflate. With ExifTool 13.55 and Adobe SDK 1.7.1 (2802), all 21 reviewed ZIP exports decoded without SDK warnings and with exact RGB16 samples. This exception requires a SHORT[1] compression tag equal to 8.",
            })
        }
        ("dng", "[Minor] Not decoding some large array(s). Ignore minor errors to decode") => {
            // The preserved ProfileToneCurve exceeds ExifTool's 500-value decoding guard.
            Some(ReviewedWarning {
                id: "DNG-TONE-CURVE-LIMIT",
                reason: "The preserved FLOAT[1026] tone curve exceeds ExifTool 13.55's optional array-decoding limit. Its count, finite values, input endpoints and bounds are checked independently. Adobe SDK 1.7.1 (2802) parsed all eight reviewed DNGs without warnings and decoded exact stage-1 samples. This is parser coverage, not visual appearance approval.",
            })
        }
        ("dng", "[Minor] Ignoring IFD0 ProfileLookTableData with excessive count") => {
            // Adobe DNG permits Hue x Saturation x Value x 3 FLOAT values.
            // This table exceeds ExifTool's independent 100,000-value parsing guard.
            Some(ReviewedWarning {
                id: "DNG-LOOK-TABLE-LIMIT",
                reason: "The valid 72x33x65 look table has 463320 FLOAT values, exceeding ExifTool 13.55's numeric-array count limit. Dimensions, count, bounds and finite values are checked independently. Adobe SDK 1.7.1 (2802) parsed all eight reviewed DNGs without warnings and decoded exact stage-1 samples. Profile values remain unchanged.",
            })
        }
        _ => None,
    }
}

pub fn check_export(path: &Path, shutter: (u32, u32)) {
    let format = path.extension().unwrap().to_str().unwrap();
    let bytes = exif_bytes(path);
    let root = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let (_, _, pointer) = entry(&bytes, root, 0x8769);
    let exif = u32::from_le_bytes(pointer.try_into().unwrap()) as usize;
    if let Some((kind, count, value)) = find_entry(&bytes, exif, 0xa420) {
        assert_eq!((kind, count), (2, 33));
        assert!(value[..32].iter().all(u8::is_ascii_hexdigit));
        assert_eq!(value[32], 0);
    }
    let (kind, count, value) = entry(&bytes, exif, 0x829a);
    assert_eq!((kind, count), (5, 1));
    assert_eq!(
        (
            u32::from_le_bytes(value[..4].try_into().unwrap()),
            u32::from_le_bytes(value[4..].try_into().unwrap())
        ),
        shutter
    );
    let compression = if format == "tiff" {
        let (kind, count, value) = entry(&bytes, root, 259);
        assert_eq!((kind, count), (3, 1));
        Some(u16::from_le_bytes(value.try_into().unwrap()))
    } else {
        None
    };
    if format == "tiff" {
        assert_eq!(
            entry(&bytes, root, 34675).0,
            7,
            "ICC data must be UNDEFINED"
        );
    }
    if format == "dng" {
        let (kind, count, data) = entry(&bytes, root, 50981);
        assert_eq!((kind, count), (4, 3));
        let dims: Vec<_> = data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_le_bytes(*b))
            .collect();
        assert_eq!(dims, [72, 33, 65]);
        let (kind, count, data) = entry(&bytes, root, 50982);
        assert_eq!((kind, count), (11, dims.iter().product::<u32>() * 3));
        assert!(data
            .as_chunks::<4>()
            .0
            .iter()
            .all(|b| f32::from_le_bytes(*b).is_finite()));
        let (kind, count, data) = entry(&bytes, root, 50940);
        assert_eq!((kind, count), (11, 1026));
        let curve: Vec<_> = data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        assert!(curve.iter().all(|v| v.is_finite()));
        assert_eq!(curve[0], 0.);
        assert_eq!(curve[curve.len() - 2], 1.);
        assert!(curve.iter().all(|v| (0. ..=1.).contains(v)));
        assert!(curve
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| v[0])
            .collect::<Vec<_>>()
            .windows(2)
            .all(|v| v[0] < v[1]));
    }
    let output = Command::new("exiftool")
        .args([
            "-config",
            "",
            "-j",
            "-G1:4",
            "-a",
            "-validate",
            "-warning",
            "-error",
        ])
        .arg(path)
        .output()
        .expect("ExifTool is a declared test dependency");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let record = records[0].as_object().unwrap();
    let mut accepted = BTreeSet::new();
    for (key, value) in record {
        if key.ends_with(":Error") {
            panic!("{} {key}: {value}", path.display());
        }
        if key.ends_with(":Warning") {
            let decision = value.as_str()
                .and_then(|warning| reviewed_warning(format, compression, warning))
                .unwrap_or_else(|| panic!("{}: unreviewed {key}: {value}. Review crates/foveonic/tests/common/capture_exif.rs before changing acceptance rules.",path.display()));
            assert!(
                accepted.insert(decision.id),
                "duplicate validator warning: {}",
                decision.id
            );
            eprintln!("{}: {key}: {value}\n  REVIEWED EXCEPTION {}: {}\n  Decision: crates/foveonic/tests/common/capture_exif.rs (reviewed_warning).",path.display(),decision.id,decision.reason);
        }
    }
    let expected = match (format, accepted.len()) {
        (_, 0) => "OK",
        ("tiff", 1) => "1 Warning",
        ("dng", 1) => "1 Warning (minor)",
        ("dng", 2) => "2 Warnings (all minor)",
        _ => panic!("unexpected warning count: {record:?}"),
    };
    assert_eq!(
        record["ExifTool:Validate"],
        expected,
        "{}: {record:?}",
        path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_warning_requires_the_reviewed_container_and_code() {
        let warning = "Invalid value for IFD0 tag 0x0103 Compression";
        assert!(reviewed_warning("tiff", Some(8), warning).is_some());
        for format in ["dng", "jpg", "png", "jxl"] {
            assert!(reviewed_warning(format, Some(8), warning).is_none());
        }
        for compression in [None, Some(1), Some(5), Some(32946)] {
            assert!(reviewed_warning("tiff", compression, warning).is_none());
        }
        assert!(reviewed_warning("tiff", Some(8), &format!("{warning} changed")).is_none());
    }

    #[test]
    fn unreviewed_notices_and_wrong_dng_context_fail_closed() {
        assert!(reviewed_warning("dng", None, "Invalid ProfileToneCurve").is_none());
        assert!(reviewed_warning("dng", None, "[Minor] Unknown warning").is_none());
        for warning in [
            "[Minor] Not decoding some large array(s). Ignore minor errors to decode",
            "[Minor] Ignoring IFD0 ProfileLookTableData with excessive count",
        ] {
            assert!(reviewed_warning("dng", None, warning).is_some());
            for format in ["tiff", "jpg", "png", "jxl"] {
                assert!(reviewed_warning(format, None, warning).is_none());
            }
        }
    }
}
