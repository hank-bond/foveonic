//! Reads preferred exposure metadata from an embedded JPEG's optional Exif header.

use std::collections::BTreeMap;

/// Optional preview metadata must not prevent decoding the original sensor data.
pub(super) fn properties(jpeg: &[u8]) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    if !jpeg.starts_with(&[0xff, 0xd8]) {
        return result;
    }
    let mut position = 2;
    while jpeg.get(position) == Some(&0xff) {
        let Some(&marker) = jpeg.get(position + 1) else {
            break;
        };
        if marker == 0xda || marker == 0xd9 {
            break;
        }
        let Some(length) = jpeg.get(position + 2..position + 4) else {
            break;
        };
        let length = usize::from(u16::from_be_bytes(length.try_into().unwrap()));
        if length < 2 {
            break;
        }
        let Some(payload) = jpeg.get(position + 4..position + 2 + length) else {
            break;
        };
        if marker == 0xe1 && payload.starts_with(b"Exif\0\0") {
            if let Some(values) = exposure_values(&payload[6..]) {
                for (name, value) in values {
                    if value.is_finite() {
                        result.insert(name.into(), value.to_string());
                    }
                }
            }
            break;
        }
        position += 2 + length;
    }
    result
}

fn exposure_values(bytes: &[u8]) -> Option<Vec<(&'static str, f64)>> {
    let big = match bytes.get(..2)? {
        b"II" => false,
        b"MM" => true,
        _ => return None,
    };
    let u16_at = |p: usize| -> Option<u16> {
        let data = bytes.get(p..p.checked_add(2)?)?.try_into().ok()?;
        Some(if big {
            u16::from_be_bytes(data)
        } else {
            u16::from_le_bytes(data)
        })
    };
    let u32_at = |p: usize| -> Option<u32> {
        let data = bytes.get(p..p.checked_add(4)?)?.try_into().ok()?;
        Some(if big {
            u32::from_be_bytes(data)
        } else {
            u32::from_le_bytes(data)
        })
    };
    if u16_at(2)? != 42 {
        return None;
    }
    let find = |directory: usize, tag: u16| -> Option<(u16, u32, u32)> {
        let count = usize::from(u16_at(directory)?);
        if count > 4096 {
            return None;
        }
        for i in 0..count {
            let p = directory.checked_add(2 + 12 * i)?;
            if u16_at(p)? == tag {
                return Some((u16_at(p + 2)?, u32_at(p + 4)?, u32_at(p + 8)?));
            }
        }
        None
    };
    let (kind, count, directory) = find(u32_at(4)? as usize, 0x8769)?;
    if kind != 4 || count != 1 {
        return None;
    }
    let mut values = Vec::new();
    for (tag, name) in [(0x829d, "EXIF_FNUMBER"), (0x9204, "EXIF_EXPOSURE_BIAS")] {
        let Some((kind, count, offset)) = find(directory as usize, tag) else {
            continue;
        };
        if count != 1 || ![5, 10].contains(&kind) {
            continue;
        }
        let p = offset as usize;
        let (Some(n), Some(d)) = (u32_at(p), u32_at(p + 4)) else {
            continue;
        };
        let (n, d) = if kind == 10 {
            (f64::from(n as i32), f64::from(d as i32))
        } else {
            (f64::from(n), f64::from(d))
        };
        if d != 0. {
            values.push((name, n / d));
        }
    }
    Some(values)
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
