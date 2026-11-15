//! Reads standard capture fields, color declarations and bounded thumbnails from JPEG EXIF.

use std::collections::BTreeMap;

const TIFF_MAGIC: u16 = 42;
const TIFF_LONG: u16 = 4;
const MAX_IFD_ENTRIES: usize = 4096;
const IFD_COUNT_BYTES: usize = 2;
const IFD_ENTRY_BYTES: usize = 12;
const JPEG_OFFSET_TAG: u16 = 0x0201;
const JPEG_LENGTH_TAG: u16 = 0x0202;
const JPEG_MARKER_PREFIX: u8 = 0xff;
const JPEG_START: u8 = 0xd8;
const JPEG_SCAN: u8 = 0xda;
const JPEG_END: u8 = 0xd9;
const JPEG_APP1: u8 = 0xe1;
const EXIF_SIGNATURE: &[u8; 6] = b"Exif\0\0";
const EXIF_IFD_TAG: u16 = 0x8769;
const COLOR_SPACE_TAG: u16 = 0xa001;
const INTEROPERABILITY_IFD_TAG: u16 = 0xa005;
const INTEROPERABILITY_INDEX_TAG: u16 = 0x0001;

/// Standard JPEG color tags apply to preview pixels independently of RAW settings.
pub(crate) fn color_space(jpeg: &[u8]) -> Option<crate::color_space::ColorSpace> {
    use crate::color_space::ColorSpace;
    let bytes = exif_payload(jpeg)?;
    let big = match bytes.get(..2)? {
        b"II" => false,
        b"MM" => true,
        _ => return None,
    };
    let t = Tiff { bytes, big };
    if t.u16(2)? != TIFF_MAGIC {
        return None;
    }
    let first = t.u32(4)? as usize;
    let exif = t.number(first, EXIF_IFD_TAG)? as usize;
    if t.number(exif, COLOR_SPACE_TAG) == Some(1.) {
        return Some(ColorSpace::Srgb);
    }
    let interoperability = t.number(exif, INTEROPERABILITY_IFD_TAG)? as usize;
    match t
        .text(interoperability, INTEROPERABILITY_INDEX_TAG)?
        .as_str()
    {
        "R03" => Some(ColorSpace::AdobeRgb),
        "R98" => Some(ColorSpace::Srgb),
        _ => None,
    }
}

/// Optional preview metadata must not prevent decoding the original sensor data.
pub(super) fn properties(jpeg: &[u8]) -> BTreeMap<String, String> {
    exif_payload(jpeg)
        .and_then(tiff_properties)
        .unwrap_or_default()
}

/// Seek over unrelated JPEG segments and read only the bounded EXIF payload.
pub(super) fn properties_at(
    reader: &mut (impl std::io::Read + std::io::Seek),
    start: u64,
    size: usize,
) -> crate::source::Result<BTreeMap<String, String>> {
    use std::io::SeekFrom;
    let Some(end) = start.checked_add(size as u64) else {
        return Ok(BTreeMap::new());
    };
    if size < 2 {
        return Ok(BTreeMap::new());
    }
    reader.seek(SeekFrom::Start(start))?;
    let mut soi = [0; 2];
    reader.read_exact(&mut soi)?;
    if soi != [JPEG_MARKER_PREFIX, JPEG_START] {
        return Ok(BTreeMap::new());
    }
    let mut position = start + 2;
    for _ in 0..MAX_IFD_ENTRIES {
        if position >= end {
            break;
        }
        reader.seek(SeekFrom::Start(position))?;
        let mut byte = [0; 1];
        reader.read_exact(&mut byte)?;
        if byte[0] != JPEG_MARKER_PREFIX {
            break;
        }
        position += 1;
        let mut fill = 0;
        let marker = loop {
            if position >= end || fill == MAX_IFD_ENTRIES {
                return Ok(BTreeMap::new());
            }
            reader.read_exact(&mut byte)?;
            position += 1;
            if byte[0] != JPEG_MARKER_PREFIX {
                break byte[0];
            }
            fill += 1;
        };
        if matches!(marker, JPEG_SCAN | JPEG_END | 0) {
            break;
        }
        if marker == 1 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if position.checked_add(2).is_none_or(|p| p > end) {
            break;
        }
        let mut word = [0; 2];
        reader.read_exact(&mut word)?;
        let length = usize::from(u16::from_be_bytes(word));
        if length < 2 {
            break;
        }
        let Some(next) = position.checked_add(length as u64).filter(|&v| v <= end) else {
            break;
        };
        if marker == JPEG_APP1 && length >= 8 {
            let mut signature = [0; 6];
            reader.read_exact(&mut signature)?;
            if &signature == EXIF_SIGNATURE {
                let mut tiff = vec![0; length - 8];
                reader.read_exact(&mut tiff)?;
                if let Some(properties) = tiff_properties(&tiff) {
                    return Ok(properties);
                }
            }
        }
        position = next;
    }
    Ok(BTreeMap::new())
}

/// Only IFD1's declared JPEG range is accepted. Bad optional metadata falls
/// back to the ordinary preview without touching sensor decoding.
pub(crate) fn thumbnail(jpeg: &[u8]) -> Option<&[u8]> {
    let bytes = exif_payload(jpeg)?;
    let big = match bytes.get(..2)? {
        b"II" => false,
        b"MM" => true,
        _ => return None,
    };
    let t = Tiff { bytes, big };
    if t.u16(2)? != TIFF_MAGIC {
        return None;
    }
    let first = t.u32(4)? as usize;
    let count = usize::from(t.u16(first)?);
    if count > MAX_IFD_ENTRIES {
        return None;
    }
    let second = t.u32(first.checked_add(IFD_COUNT_BYTES + count * IFD_ENTRY_BYTES)?)? as usize;
    if second == 0 {
        return None;
    }
    let (TIFF_LONG, 1, offset) = t.find(second, JPEG_OFFSET_TAG)? else {
        return None;
    };
    let (TIFF_LONG, 1, size) = t.find(second, JPEG_LENGTH_TAG)? else {
        return None;
    };
    let start = t.u32(offset)? as usize;
    let length = t.u32(size)? as usize;
    bytes.get(start..start.checked_add(length)?)
}

fn exif_payload(jpeg: &[u8]) -> Option<&[u8]> {
    if !jpeg.starts_with(&[JPEG_MARKER_PREFIX, JPEG_START]) {
        return None;
    }
    let mut position = 2;
    for _ in 0..MAX_IFD_ENTRIES {
        if *jpeg.get(position)? != JPEG_MARKER_PREFIX {
            return None;
        }
        position += 1;
        let mut fill = 0;
        let marker = loop {
            let marker = *jpeg.get(position)?;
            position += 1;
            if marker != JPEG_MARKER_PREFIX {
                break marker;
            }
            fill += 1;
            if fill == MAX_IFD_ENTRIES {
                return None;
            }
        };
        if matches!(marker, JPEG_SCAN | JPEG_END | 0) {
            return None;
        }
        if marker == 1 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length = usize::from(u16::from_be_bytes(
            jpeg.get(position..position + 2)?.try_into().ok()?,
        ));
        if length < 2 {
            return None;
        }
        let next = position.checked_add(length)?;
        let payload = jpeg.get(position + 2..next)?;
        if marker == JPEG_APP1 && payload.starts_with(EXIF_SIGNATURE) {
            return payload.get(EXIF_SIGNATURE.len()..);
        }
        position = next;
    }
    None
}

struct Tiff<'a> {
    bytes: &'a [u8],
    big: bool,
}

impl<'a> Tiff<'a> {
    fn u16(&self, p: usize) -> Option<u16> {
        let v = self.bytes.get(p..p.checked_add(2)?)?.try_into().ok()?;
        Some(if self.big {
            u16::from_be_bytes(v)
        } else {
            u16::from_le_bytes(v)
        })
    }
    fn u32(&self, p: usize) -> Option<u32> {
        let v = self.bytes.get(p..p.checked_add(4)?)?.try_into().ok()?;
        Some(if self.big {
            u32::from_be_bytes(v)
        } else {
            u32::from_le_bytes(v)
        })
    }
    fn find(&self, dir: usize, tag: u16) -> Option<(u16, usize, usize)> {
        let count = usize::from(self.u16(dir)?);
        if count > MAX_IFD_ENTRIES {
            return None;
        }
        self.bytes
            .get(dir..dir.checked_add(IFD_COUNT_BYTES + count * IFD_ENTRY_BYTES)?)?;
        for i in 0..count {
            let p = dir + IFD_COUNT_BYTES + i * IFD_ENTRY_BYTES;
            if self.u16(p)? == tag {
                let kind = self.u16(p + 2)?;
                let count = self.u32(p + 4)? as usize;
                let unit: usize = match kind {
                    1 | 2 | 7 => 1,
                    3 => 2,
                    4 | 9 => 4,
                    5 | 10 => 8,
                    _ => return None,
                };
                let size = unit.checked_mul(count)?;
                let offset = if size <= 4 {
                    p + 8
                } else {
                    self.u32(p + 8)? as usize
                };
                self.bytes.get(offset..offset.checked_add(size)?)?;
                return Some((kind, count, offset));
            }
        }
        None
    }
    fn text(&self, dir: usize, tag: u16) -> Option<String> {
        let (kind, count, p) = self.find(dir, tag)?;
        if kind != 2 || count > 65536 {
            return None;
        }
        let text = std::str::from_utf8(self.bytes.get(p..p + count)?)
            .ok()?
            .trim_end_matches('\0')
            .trim();
        (!text.is_empty()).then(|| text.to_owned())
    }
    fn number(&self, dir: usize, tag: u16) -> Option<f64> {
        let (kind, count, p) = self.find(dir, tag)?;
        if count != 1 {
            return None;
        }
        let v = match kind {
            3 => f64::from(self.u16(p)?),
            4 => f64::from(self.u32(p)?),
            5 | 10 => {
                let (n, d) = (self.u32(p)?, self.u32(p + 4)?);
                let (n, d) = if kind == 10 {
                    (f64::from(n as i32), f64::from(d as i32))
                } else {
                    (f64::from(n), f64::from(d))
                };
                if d == 0. {
                    return None;
                }
                n / d
            }
            _ => return None,
        };
        v.is_finite().then_some(v)
    }
}

// Keep capture facts and rights. Pixel layout, previews, MakerNotes, color and
// pointers belong to the source image and are rebuilt for the export.
pub(crate) fn retained(ifd: u8, tag: u16) -> bool {
    match ifd {
        0 => matches!(
            tag,
            0x010e | 0x010f | 0x0110 | 0x0132 | 0x013b | 0x013c | 0x8298 | 0x9c9b..=0x9c9f
        ),
        1 => matches!(tag,
            0x829a | 0x829d | 0x8822 | 0x8824 | 0x8827 | 0x8828 | 0x8830..=0x8835 |
            0x9000 | 0x9003 | 0x9004 | 0x9010..=0x9012 |
            0x9201..=0x920a | 0x9286 | 0x9290..=0x9292 | 0x9400..=0x9405 | 0xa004 |
            0xa20b | 0xa20c | 0xa20e..=0xa210 | 0xa215 | 0xa217 |
            0xa300..=0xa301 | 0xa402..=0xa40c | 0xa420 | 0xa430..=0xa435 |
            0xa436..=0xa439 | 0xa460..=0xa462),
        2 => tag <= 0x1f,
        _ => false,
    }
}

fn embedded_tags(t: &Tiff<'_>, root: usize) -> Vec<crate::capture_metadata::EmbeddedTag> {
    let mut result = Vec::new();
    let exif = t.number(root, 0x8769).map(|x| x as usize);
    let gps = t.number(root, 0x8825).map(|x| x as usize);
    for (ifd, dir) in [(0, Some(root)), (1, exif), (2, gps)] {
        let Some(dir) = dir else { continue };
        let Some(count) = t
            .u16(dir)
            .map(usize::from)
            .filter(|&n| n <= MAX_IFD_ENTRIES)
        else {
            continue;
        };
        for i in 0..count {
            let Some(id) = t.u16(dir + IFD_COUNT_BYTES + i * IFD_ENTRY_BYTES) else {
                break;
            };
            if !retained(ifd, id)
                || result
                    .iter()
                    .any(|e: &crate::capture_metadata::EmbeddedTag| e.ifd == ifd && e.id == id)
            {
                continue;
            }
            let Some((kind, count, offset)) = t.find(dir, id) else {
                continue;
            };
            let unit = match kind {
                1 | 2 | 7 => 1,
                3 => 2,
                4 | 9 => 4,
                5 | 10 => 8,
                _ => continue,
            };
            let Some(size) = count.checked_mul(unit).filter(|&n| n > 0 && n <= 65536) else {
                continue;
            };
            let Some(bytes) = t.bytes.get(offset..offset + size) else {
                continue;
            };
            if kind == 2 && (bytes.last() != Some(&0) || !bytes.is_ascii()) {
                continue;
            }
            if matches!(kind, 5 | 10) && bytes.as_chunks::<8>().0.iter().any(|x| x[4..] == [0; 4]) {
                continue;
            }
            let mut data = bytes.to_vec();
            let word = match kind {
                3 => 2,
                4 | 5 | 9 | 10 => 4,
                _ => 1,
            };
            if t.big == cfg!(target_endian = "little") && word > 1 {
                for part in data.chunks_exact_mut(word) {
                    part.reverse();
                }
            }
            let entry = crate::capture_metadata::EmbeddedTag {
                ifd,
                id,
                kind,
                count,
                data,
            };
            if entry.valid() {
                result.push(entry);
            }
        }
    }
    result
}

fn tiff_properties(bytes: &[u8]) -> Option<BTreeMap<String, String>> {
    let big = match bytes.get(..2)? {
        b"II" => false,
        b"MM" => true,
        _ => return None,
    };
    let t = Tiff { bytes, big };
    if t.u16(2)? != TIFF_MAGIC {
        return None;
    }
    let first = t.u32(4)? as usize;
    let mut out = BTreeMap::new();
    let tags = embedded_tags(&t, first);
    if !tags.is_empty() {
        out.insert(
            "EXIF_CAPTURE_TAGS".into(),
            serde_json::to_string(&tags).ok()?,
        );
    }
    for (tag, name) in [
        (0x10f, "CAMMANUF"),
        (0x110, "CAMMODEL"),
        (0x132, "EXIF_DATETIME"),
        (0x131, "FIRMVERS"),
    ] {
        if let Some(v) = t.text(first, tag) {
            out.insert(name.into(), v);
        }
    }
    let Some(exif) = t.number(first, 0x8769).map(|v| v as usize) else {
        return Some(out);
    };
    for (tag, name) in [
        (0x829d, "EXIF_FNUMBER"),
        (0x9204, "EXIF_EXPOSURE_BIAS"),
        (0x829a, "SHUTTER"),
        (0x8827, "ISO"),
        (0x920a, "FLENGTH"),
    ] {
        if let Some(v) = t.number(exif, tag) {
            out.insert(name.into(), v.to_string());
            if name == "ISO" {
                out.insert("EXIF_PREVIEW_ISO".into(), "1".into());
            }
        }
    }
    if let Some(v) = t.number(exif, 0x9204) {
        out.insert("EXPCOMP".into(), v.to_string());
    }
    if let Some(v) = t.text(exif, 0xa434) {
        out.insert("LENSMODEL".into(), v);
    }
    if let Some(v) = t.number(exif, 0x829d) {
        out.insert("APERTURE".into(), v.to_string());
    }
    if let Some(v) = t.number(exif, 0x829a) {
        out.insert("EXPTIME".into(), (v * 1_000_000.).round().to_string());
    }
    if let Some((7, count, p)) = t.find(exif, 0x927c) {
        if count >= 12 && bytes.get(p..p + 8) == Some(b"SIGMA\0\0\0") {
            let dir = p + 10;
            for (tag, name) in [
                (0x2, "CAMSERIAL"),
                (0x27, "LENSMODEL"),
                (0x7, "WB_DESC"),
                (0xb, "COLORSPACE"),
                (0x3d, "CM_DESC"),
            ] {
                if let Some(v) = t.text(dir, tag) {
                    out.insert(name.into(), v);
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_color_tags_identify_srgb_and_adobe_in_both_byte_orders() {
        use crate::color_space::ColorSpace;
        for big in [false, true] {
            let word = |v: u16| {
                if big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                }
            };
            let long = |v: u32| {
                if big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                }
            };
            for (space, interoperability, expected) in [
                (1, *b"R98\0", Some(ColorSpace::Srgb)),
                (65535, *b"R03\0", Some(ColorSpace::AdobeRgb)),
                (65535, *b"R98\0", Some(ColorSpace::Srgb)),
                (65535, *b"???\0", None),
            ] {
                let mut tiff = Vec::new();
                tiff.extend(if big { b"MM" } else { b"II" });
                tiff.extend(word(TIFF_MAGIC));
                tiff.extend(long(8));
                tiff.extend(word(1));
                tiff.extend(word(EXIF_IFD_TAG));
                tiff.extend(word(4));
                tiff.extend(long(1));
                tiff.extend(long(26));
                tiff.extend(long(0));
                tiff.extend(word(2));
                tiff.extend(word(COLOR_SPACE_TAG));
                tiff.extend(word(3));
                tiff.extend(long(1));
                tiff.extend(word(space));
                tiff.extend(word(0));
                tiff.extend(word(INTEROPERABILITY_IFD_TAG));
                tiff.extend(word(4));
                tiff.extend(long(1));
                tiff.extend(long(56));
                tiff.extend(long(0));
                tiff.extend(word(1));
                tiff.extend(word(INTEROPERABILITY_INDEX_TAG));
                tiff.extend(word(2));
                tiff.extend(long(4));
                tiff.extend(interoperability);
                tiff.extend(long(0));
                let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe1];
                jpeg.extend(((tiff.len() + 8) as u16).to_be_bytes());
                jpeg.extend(EXIF_SIGNATURE);
                jpeg.extend(tiff);
                assert_eq!(color_space(&jpeg), expected);
                for end in 0..jpeg.len() {
                    assert_eq!(color_space(&jpeg[..end]), None);
                }
            }
        }
    }

    #[test]
    fn capture_scan_skips_large_unrelated_jpeg_segments() {
        use std::io::{Cursor, Read, Seek, SeekFrom};
        struct Counting {
            inner: Cursor<Vec<u8>>,
            read: usize,
        }
        impl Read for Counting {
            fn read(&mut self, data: &mut [u8]) -> std::io::Result<usize> {
                let n = self.inner.read(data)?;
                self.read += n;
                Ok(n)
            }
        }
        impl Seek for Counting {
            fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
                self.inner.seek(pos)
            }
        }
        let metadata = crate::source::Metadata {
            width: 1,
            height: 1,
            rotation: 0,
            processing_version: None,
            properties: [
                ("CAMMODEL".into(), "SIGMA DP2 Merrill".into()),
                ("ISO".into(), "200".into()),
            ]
            .into(),
            matrices: Default::default(),
            lists: Default::default(),
        };
        let tiff = crate::capture_metadata::Capture::new(&metadata)
            .payload([1, 1])
            .unwrap();
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xff, 0x01, 0xff, 0xd0];
        for _ in 0..3 {
            jpeg.extend([0xff, 0xe2, 0xff, 0xff]);
            jpeg.extend(vec![0; 65_533]);
        }
        jpeg.extend([0xff, 0xe1]);
        jpeg.extend(((tiff.len() + 8) as u16).to_be_bytes());
        jpeg.extend(b"Exif\0\0");
        jpeg.extend(&tiff);
        assert_eq!(properties(&jpeg)["ISO"], "200");
        let size = jpeg.len();
        let mut reader = Counting {
            inner: Cursor::new(jpeg),
            read: 0,
        };
        let result = properties_at(&mut reader, 0, size).unwrap();
        assert_eq!(result["ISO"], "200");
        assert!(
            reader.read < tiff.len() + 100,
            "only marker headers and EXIF should be read"
        );
    }

    #[test]
    fn reads_signed_and_unsigned_rationals_with_either_byte_order() {
        for big in [false, true] {
            let mut data = vec![0; 80];
            data[..2].copy_from_slice(if big { b"MM" } else { b"II" });
            let put16 = |data: &mut [u8], p, v: u16| {
                data[p..p + 2].copy_from_slice(&if big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                })
            };
            let put32 = |data: &mut [u8], p, v: u32| {
                data[p..p + 4].copy_from_slice(&if big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                })
            };
            put16(&mut data, 2, 42);
            put32(&mut data, 4, 8);
            put16(&mut data, 8, 1);
            put16(&mut data, 10, 0x8769);
            put16(&mut data, 12, 4);
            put32(&mut data, 14, 1);
            put32(&mut data, 18, 26);
            put16(&mut data, 26, 2);
            for (i, tag, kind, offset) in [(0, 0x829d, 5, 56), (1, 0x9204, 10, 64)] {
                let p = 28 + i * 12;
                put16(&mut data, p, tag);
                put16(&mut data, p + 2, kind);
                put32(&mut data, p + 4, 1);
                put32(&mut data, p + 8, offset);
            }
            put32(&mut data, 56, 63);
            put32(&mut data, 60, 10);
            put32(&mut data, 64, (-1_i32) as u32);
            put32(&mut data, 68, 3);
            let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe1];
            jpeg.extend(((data.len() + 8) as u16).to_be_bytes());
            jpeg.extend(b"Exif\0\0");
            jpeg.extend(data);
            let values = properties(&jpeg);
            let tags: Vec<crate::capture_metadata::EmbeddedTag> =
                serde_json::from_str(&values["EXIF_CAPTURE_TAGS"]).unwrap();
            let aperture = tags
                .iter()
                .find(|entry| entry.ifd == 1 && entry.id == 0x829d)
                .unwrap();
            assert_eq!(
                aperture.data,
                [63_u32, 10]
                    .into_iter()
                    .flat_map(u32::to_ne_bytes)
                    .collect::<Vec<_>>()
            );
            let exposure = tags
                .iter()
                .find(|entry| entry.ifd == 1 && entry.id == 0x9204)
                .unwrap();
            assert_eq!(
                exposure.data,
                [(-1_i32).to_ne_bytes(), 3_i32.to_ne_bytes()].concat()
            );
            assert_eq!(values["EXIF_FNUMBER"], "6.3");
            assert_eq!(
                values["EXIF_EXPOSURE_BIAS"].parse::<f64>().unwrap(),
                -1. / 3.
            );
            for length in 0..jpeg.len() {
                assert!(properties(&jpeg[..length]).is_empty());
            }
        }
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    #[test]
    fn capture_identity_and_camera_choices_use_tiff_relative_makernote_offsets() {
        let mut bytes = vec![0_u8; 256];
        bytes[..2].copy_from_slice(b"II");
        let put16 = |b: &mut [u8], p: usize, v: u16| b[p..p + 2].copy_from_slice(&v.to_le_bytes());
        let put32 = |b: &mut [u8], p: usize, v: u32| b[p..p + 4].copy_from_slice(&v.to_le_bytes());
        put16(&mut bytes, 2, 42);
        put32(&mut bytes, 4, 8);
        let entry = |b: &mut [u8], p, tag, kind, count, value| {
            put16(b, p, tag);
            put16(b, p + 2, kind);
            put32(b, p + 4, count);
            put32(b, p + 8, value);
        };
        put16(&mut bytes, 8, 2);
        entry(&mut bytes, 10, 0x110, 2, 11, 200);
        entry(&mut bytes, 22, 0x8769, 4, 1, 40);
        bytes[200..211].copy_from_slice(b"sd Quattro\0");
        put16(&mut bytes, 40, 1);
        entry(&mut bytes, 42, 0x927c, 7, 90, 80);
        bytes[80..88].copy_from_slice(b"SIGMA\0\0\0");
        put16(&mut bytes, 90, 2);
        entry(&mut bytes, 92, 7, 2, 5, 212);
        entry(&mut bytes, 104, 0x3d, 2, 9, 220);
        bytes[212..217].copy_from_slice(b"Auto\0");
        bytes[220..229].copy_from_slice(b"Standard\0");
        let values = tiff_properties(&bytes).unwrap();
        assert_eq!(values["CAMMODEL"], "sd Quattro");
        assert_eq!(values["WB_DESC"], "Auto");
        assert_eq!(values["CM_DESC"], "Standard");
        put32(&mut bytes, 112, u32::MAX);
        let values = tiff_properties(&bytes).unwrap();
        assert!(!values.contains_key("CM_DESC"));
    }
}

#[cfg(test)]
mod thumbnail_tests {
    use super::*;
    #[test]
    fn thumbnail_offsets_are_bounded_in_both_byte_orders() {
        for big in [false, true] {
            let word = |v: u16| {
                if big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                }
            };
            let long = |v: u32| {
                if big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                }
            };
            let mut tiff = Vec::new();
            tiff.extend(if big { b"MM" } else { b"II" });
            tiff.extend(word(42));
            tiff.extend(long(8));
            tiff.extend(word(0));
            tiff.extend(long(14));
            tiff.extend(word(2));
            for (tag, value) in [(0x201, 44), (0x202, 4)] {
                tiff.extend(word(tag));
                tiff.extend(word(4));
                tiff.extend(long(1));
                tiff.extend(long(value));
            }
            tiff.extend(long(0));
            tiff.extend([0xff, 0xd8, 0xff, 0xd9]);
            let wrap = |tiff: &[u8]| {
                let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe1];
                jpeg.extend(((tiff.len() + 8) as u16).to_be_bytes());
                jpeg.extend(b"Exif\0\0");
                jpeg.extend(tiff);
                jpeg
            };
            let jpeg = wrap(&tiff);
            assert_eq!(thumbnail(&jpeg), Some(&[0xff, 0xd8, 0xff, 0xd9][..]));
            for end in 0..jpeg.len() {
                assert!(thumbnail(&jpeg[..end]).is_none());
            }
            tiff[24..28].copy_from_slice(&long(u32::MAX));
            assert!(thumbnail(&wrap(&tiff)).is_none());
        }
    }
}
