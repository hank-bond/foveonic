//! Seeks directly to X3F capture properties without reading sensor or calibration payloads.

use super::{
    bytes::Bytes, read_properties, BASE_HEADER_BYTES, DIRECTORY_ENTRY_BYTES,
    DIRECTORY_HEADER_BYTES, MAXIMUM_DIRECTORY_ENTRIES, MAXIMUM_FILE_BYTES, MINIMUM_FILE_BYTES,
    SECTION_VERSION, SUPPORTED_FILE_VERSIONS,
};
use crate::source::Result;
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom},
};

// Capture strings are small. A corrupt PROP length must not cause an
// allocation as large as the sensor image.
const MAXIMUM_PROPERTY_BYTES: usize = 1024 * 1024;

pub(crate) fn read(reader: &mut (impl Read + Seek)) -> Result<BTreeMap<String, String>> {
    let length = reader.seek(SeekFrom::End(0))?;
    if !(MINIMUM_FILE_BYTES..=MAXIMUM_FILE_BYTES).contains(&length) {
        return Err("unsupported X3F file size".into());
    }
    reader.seek(SeekFrom::Start(0))?;
    let mut header = [0; BASE_HEADER_BYTES];
    reader.read_exact(&mut header)?;
    let bytes = Bytes(&header);
    if bytes.range(0, 4)? != b"FOVb" || !SUPPORTED_FILE_VERSIONS.contains(&bytes.u32(4)?) {
        return Err("unsupported X3F header".into());
    }
    reader.seek(SeekFrom::End(-4))?;
    let mut pointer = [0; 4];
    reader.read_exact(&mut pointer)?;
    let directory = u64::from(Bytes(&pointer).u32(0)?);
    if directory < BASE_HEADER_BYTES as u64
        || directory + DIRECTORY_HEADER_BYTES as u64 > length - 4
    {
        return Err("X3F directory outside file".into());
    }
    reader.seek(SeekFrom::Start(directory))?;
    let mut header = [0; DIRECTORY_HEADER_BYTES];
    reader.read_exact(&mut header)?;
    let index = Bytes(&header);
    if index.range(0, 4)? != b"SECd" || index.u32(4)? != SECTION_VERSION {
        return Err("unsupported X3F directory".into());
    }
    let count = index.offset(8)?;
    if count == 0
        || count > MAXIMUM_DIRECTORY_ENTRIES
        || directory + (DIRECTORY_HEADER_BYTES + count * DIRECTORY_ENTRY_BYTES) as u64 > length - 4
    {
        return Err("invalid X3F directory count".into());
    }
    let mut entries = vec![0; count * DIRECTORY_ENTRY_BYTES];
    reader.read_exact(&mut entries)?;
    let index = Bytes(&entries);
    let mut properties = None;
    let mut images = Vec::new();
    let mut ranges = Vec::with_capacity(count);
    for i in 0..count {
        let entry = i * DIRECTORY_ENTRY_BYTES;
        let start = u64::from(index.u32(entry)?);
        let size = index.offset(entry + 4)?;
        let stop = start + size as u64;
        if start < BASE_HEADER_BYTES as u64 || size < 8 || stop > directory {
            return Err("X3F section outside data area".into());
        }
        ranges.push((start, stop));
        if matches!(index.range(entry + 8, 4)?, b"IMAG" | b"IMA2") && size >= 28 {
            images.push((start, size));
        }
        if index.range(entry + 8, 4)? == b"PROP" {
            if size > MAXIMUM_PROPERTY_BYTES {
                return Err("X3F capture properties too large".into());
            }
            if properties.replace((start, size)).is_some() {
                return Err("duplicate X3F input section".into());
            }
        }
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err("overlapping X3F sections".into());
    }
    let (start, size) = properties.ok_or("missing X3F properties")?;
    reader.seek(SeekFrom::Start(start))?;
    let mut section = vec![0; size];
    reader.read_exact(&mut section)?;
    let mut properties = read_properties(&section)?;
    if properties.get("CAMMODEL").is_some_and(|m| {
        m.trim().to_ascii_uppercase().starts_with("SIGMA ") && !m.contains("Merrill")
    }) {
        for (start, size) in images {
            reader.seek(SeekFrom::Start(start))?;
            let mut header = [0; 28];
            reader.read_exact(&mut header)?;
            let image = Bytes(&header);
            if image.range(0, 4)? != b"SECi" || image.u32(8)? != 2 || image.u32(12)? != 18 {
                continue;
            }
            reader.seek(SeekFrom::Start(start + 28))?;
            let mut jpeg = vec![0; (size - 28).min(128 * 1024)];
            reader.read_exact(&mut jpeg)?;
            let values = super::exif::properties(&jpeg);
            if !values.is_empty() {
                properties.extend(values);
                break;
            }
        }
    }
    Ok(properties)
}

#[cfg(test)]
#[path = "capture_test.rs"]
mod tests;
