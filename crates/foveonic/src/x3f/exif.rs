//! Reads capture properties and bounded thumbnails from optional JPEG Exif headers.

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

/// Optional preview metadata must not prevent decoding the original sensor data.
pub(super) fn properties(jpeg: &[u8]) -> BTreeMap<String, String> {
    exif_payload(jpeg)
        .and_then(tiff_properties)
        .unwrap_or_default()
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
    while jpeg.get(position) == Some(&JPEG_MARKER_PREFIX) {
        let marker = *jpeg.get(position + 1)?;
        if marker == JPEG_SCAN || marker == JPEG_END {
            return None;
        }
        let length = usize::from(u16::from_be_bytes(
            jpeg.get(position + 2..position + 4)?.try_into().ok()?,
        ));
        if length < 2 {
            return None;
        }
        let payload = jpeg.get(position + 4..position + 2 + length)?;
        if marker == JPEG_APP1 && payload.starts_with(EXIF_SIGNATURE) {
            return payload.get(EXIF_SIGNATURE.len()..);
        }
        position += 2 + length;
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
    for (tag, name) in [
        (0x110, "CAMMODEL"),
        (0x132, "EXIF_DATETIME"),
        (0x131, "FIRMVERS"),
    ] {
        if let Some(v) = t.text(first, tag) {
            out.insert(name.into(), v);
        }
    }
    let exif = t.number(first, 0x8769)? as usize;
    for (tag, name) in [
        (0x829d, "EXIF_FNUMBER"),
        (0x9204, "EXIF_EXPOSURE_BIAS"),
        (0x829a, "SHUTTER"),
        (0x8827, "ISO"),
        (0x920a, "FLENGTH"),
    ] {
        if let Some(v) = t.number(exif, tag) {
            out.insert(name.into(), v.to_string());
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
