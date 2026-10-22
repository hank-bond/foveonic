//! Reads scalar version-4 header adjustments into retained capture properties.

use super::bytes::{product, Bytes};
use crate::source::Result;
use std::collections::BTreeMap;

pub(super) const MAXIMUM_HEADER_BYTES: usize = 1024 * 1024;
const ENTRY_START: usize = 56;
const ENTRY_BYTES: usize = 12;
const MAXIMUM_ENTRIES: usize = 4096;
pub(crate) const PROPERTY_PREFIX: &str = "X3F_ADJUSTMENT_";

fn name(tag: u16) -> Option<&'static str> {
    Some(match tag {
        1 => "WHITE_BALANCE",
        2 => "COLOR_MODE",
        3 => "EXPOSURE",
        4 => "CONTRAST",
        5 => "SHADOWS",
        6 => "HIGHLIGHTS",
        7 => "SATURATION",
        8 => "SHARPNESS",
        9 => "FILL_LIGHT",
        10 => "HIGHLIGHT_CORRECTION",
        14 => "CHROMA_NOISE",
        15 => "LUMINANCE_NOISE",
        16 => "BANDING_NOISE",
        17 => "MONO_EXPOSURE",
        18 => "MONO_CONTRAST",
        19 => "MONO_SHADOWS",
        20 => "MONO_HIGHLIGHTS",
        21 => "MONO_SHARPNESS",
        22 => "MONO_FILL_LIGHT",
        23 => "MONO_LUMINANCE_NOISE",
        24 => "MONO_BANDING_NOISE",
        25 => "MONO_MIX_RED",
        26 => "MONO_MIX_GREEN",
        27 => "MONO_MIX_BLUE",
        63 => "SMOOTHNESS",
        64 => "MONO_SMOOTHNESS",
        _ => return None,
    })
}

/// Offsets in these TIFF-style entries are relative to the original file start.
/// The caller supplies only bytes preceding the first actual file section.
pub(super) fn read(header: &[u8]) -> Result<BTreeMap<String, String>> {
    let bytes = Bytes(header);
    if bytes.u32(4)? < 0x40000 {
        return Ok(BTreeMap::new());
    }
    let count = bytes.offset(52)?;
    if count > MAXIMUM_ENTRIES {
        return Err("too many X3F header adjustments".into());
    }
    let entries_end = ENTRY_START
        .checked_add(product(count, ENTRY_BYTES)?)
        .ok_or("X3F adjustment size overflow")?;
    bytes.range(ENTRY_START, entries_end - ENTRY_START)?;
    let mut properties = BTreeMap::new();
    for i in 0..count {
        let entry = ENTRY_START + i * ENTRY_BYTES;
        let tag = bytes.u16(entry)?;
        let Some(name) = name(tag) else { continue };
        if bytes.u32(entry + 4)? != 1 {
            return Err("X3F scalar adjustment has an invalid count".into());
        }
        let kind = bytes.u16(entry + 2)?;
        let value = if tag <= 2 {
            match kind {
                1 => u16::from(bytes.range(entry + 8, 1)?[0]),
                3 => bytes.u16(entry + 8)?,
                _ => return Err("X3F adjustment preset has an invalid type".into()),
            }
            .to_string()
        } else if (25..=27).contains(&tag) {
            if kind != 3 {
                return Err("X3F monochrome mix is not an unsigned short".into());
            }
            bytes.u16(entry + 8)?.to_string()
        } else {
            if kind != 10 {
                return Err("X3F numeric adjustment is not a signed rational".into());
            }
            let offset = bytes.offset(entry + 8)?;
            if offset < entries_end {
                return Err("X3F adjustment value overlaps header entries".into());
            }
            let numerator = i32::from_le_bytes(bytes.range(offset, 4)?.try_into()?);
            let denominator = i32::from_le_bytes(bytes.range(offset + 4, 4)?.try_into()?);
            if denominator == 0 {
                return Err("X3F adjustment has a zero denominator".into());
            }
            (f64::from(numerator) / f64::from(denominator)).to_string()
        };
        if properties
            .insert(format!("{PROPERTY_PREFIX}{name}"), value)
            .is_some()
        {
            return Err("duplicate X3F header adjustment".into());
        }
    }
    Ok(properties)
}

#[cfg(test)]
#[path = "adjustments_test.rs"]
mod tests;
