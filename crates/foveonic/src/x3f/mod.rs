//! Read owned X3F sections and decode Merrill sensor samples and camera metadata.
//!
//! Imported from Foveonic's X3F reader.

mod bytes;
pub(crate) mod camf;
mod huffman;

use crate::profile::Timing;
use crate::source::{Metadata, Result, Source};
use bytes::{product, Bytes};
use huffman::Table;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

const MINIMUM_FILE_BYTES: u64 = 44; // Forty-byte base header and trailing directory pointer.
const MAXIMUM_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const BASE_HEADER_BYTES: usize = 40;
const SECTION_VERSION: u32 = 0x0002_0000;
// Version words hold major/minor halves. DP1 Merrill review originals use
// 2.3 with the same directory, property, CAMF, and format-30 sensor sections.
const SUPPORTED_FILE_VERSIONS: [u32; 5] = [
    0x0002_0000,
    0x0002_0001,
    0x0002_0002,
    0x0002_0003,
    0x0003_0000,
];
const MAXIMUM_DIRECTORY_ENTRIES: usize = 1024;
const DIRECTORY_HEADER_BYTES: usize = 12;
const DIRECTORY_ENTRY_BYTES: usize = 12;
const SENSOR_IMAGE_TYPE: u32 = 1;
const SENSOR_HUFFMAN_FORMAT: u32 = 30;
const PROPERTY_HEADER_BYTES: usize = 24;
const PROPERTY_ENTRY_BYTES: usize = 8;

pub(crate) fn read(path: &Path) -> Result<Source> {
    let start = Instant::now();
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    if !(MINIMUM_FILE_BYTES..=MAXIMUM_FILE_BYTES).contains(&length) {
        return Err("unsupported X3F file size".into());
    }
    let mut data = Vec::with_capacity(length as usize);
    use std::io::Read;
    (&mut file).take(length + 1).read_to_end(&mut data)?;
    if data.len() as u64 != length {
        return Err("X3F file changes during reading".into());
    }
    let read = Timing::elapsed("file read", start);
    let mut source = decode(&data)?;
    source.timings.insert(0, read);
    Ok(source)
}

fn decode(data: &[u8]) -> Result<Source> {
    let bytes = Bytes(data);
    if bytes.range(0, 4)? != b"FOVb" || !SUPPORTED_FILE_VERSIONS.contains(&bytes.u32(4)?) {
        return Err("unsupported X3F header".into());
    }
    // Header byte offsets: signature 0, version 4, stored orientation 36.
    let rotation = bytes.u32(36)?;
    let end = data
        .len()
        .checked_sub(4)
        .ok_or("truncated X3F directory pointer")?;
    let directory = bytes.offset(end)?;
    if directory < BASE_HEADER_BYTES || directory > end {
        return Err("X3F directory outside file".into());
    }
    let index = Bytes(bytes.range(directory, end - directory)?);
    if index.range(0, 4)? != b"SECd" || index.u32(4)? != SECTION_VERSION {
        return Err("unsupported X3F directory".into());
    }
    let count = index.offset(8)?;
    if count == 0 || count > MAXIMUM_DIRECTORY_ENTRIES {
        return Err("invalid X3F directory count".into());
    }
    // Directory offsets: count 8, then twelve-byte (offset, size, identifier) entries.
    index.range(
        DIRECTORY_HEADER_BYTES,
        product(count, DIRECTORY_ENTRY_BYTES)?,
    )?;
    let mut raw = None;
    let mut properties = None;
    let mut camera = None;
    let mut ranges = Vec::with_capacity(count);
    for i in 0..count {
        let entry = DIRECTORY_HEADER_BYTES + i * DIRECTORY_ENTRY_BYTES;
        let start = index.offset(entry)?;
        let length = index.offset(entry + 4)?;
        let stop = start.checked_add(length).ok_or("section size overflow")?;
        if start < BASE_HEADER_BYTES || length < 8 || stop > directory {
            return Err("X3F section outside data area".into());
        }
        ranges.push((start, stop));
        let section = bytes.range(start, length)?;
        let identifier = index.range(entry + 8, 4)?;
        match identifier {
            b"IMAG" | b"IMA2" => {
                let image = Bytes(section);
                if image.range(0, 4)? != b"SECi" {
                    return Err("invalid image section signature".into());
                }
                if image.u32(8)? == SENSOR_IMAGE_TYPE {
                    insert(&mut raw, section)?;
                }
            }
            b"PROP" => insert(&mut properties, section)?,
            b"CAMF" => insert(&mut camera, section)?,
            _ => (),
        }
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err("overlapping X3F sections".into());
    }
    let properties = read_properties(properties.ok_or("missing X3F properties")?)?;
    let (width, height, pixels, mut timings) = read_pixels(raw.ok_or("missing X3F sensor image")?)?;
    let start = Instant::now();
    let camf = camf::decompress(camera.ok_or("missing X3F camera metadata")?)?;
    timings.push(Timing::elapsed("CAMF decompress", start));
    let start = Instant::now();
    let (matrices, lists) = camf::parse(&camf)?;
    timings.push(Timing::elapsed("CAMF parse", start));
    Ok(Source {
        metadata: std::sync::Arc::new(Metadata {
            width,
            height,
            rotation,
            properties,
            matrices,
            lists,
        }),
        pixels,
        camf,
        timings,
    })
}

fn insert<'a>(slot: &mut Option<&'a [u8]>, section: &'a [u8]) -> Result<()> {
    if slot.replace(section).is_some() {
        return Err("duplicate X3F input section".into());
    }
    Ok(())
}

fn read_properties(section: &[u8]) -> Result<BTreeMap<String, String>> {
    let bytes = Bytes(section);
    if bytes.range(0, 4)? != b"SECp"
        || bytes.u32(4)? != SECTION_VERSION
        || bytes.u32(12)? != 0
        || bytes.u32(16)? != 0
    {
        return Err("unsupported property section".into());
    }
    // Property header: count 8, character format/reserved 12/16, UTF16 length 20.
    // Eight-byte entries store name/value offsets in UTF16 units, not bytes.
    let count = bytes.offset(8)?;
    let table_bytes = product(count, PROPERTY_ENTRY_BYTES)?;
    bytes.range(PROPERTY_HEADER_BYTES, table_bytes)?;
    let units = bytes.offset(20)?;
    let strings = Bytes(bytes.range(PROPERTY_HEADER_BYTES + table_bytes, product(units, 2)?)?);
    let mut properties = BTreeMap::new();
    for i in 0..count {
        let key = strings.utf16(product(
            bytes.offset(PROPERTY_HEADER_BYTES + i * PROPERTY_ENTRY_BYTES)?,
            2,
        )?)?;
        let value = strings.utf16(product(
            bytes.offset(PROPERTY_HEADER_BYTES + 4 + i * PROPERTY_ENTRY_BYTES)?,
            2,
        )?)?;
        if key.is_empty() || properties.insert(key, value).is_some() {
            return Err("invalid or duplicate X3F property".into());
        }
    }
    Ok(properties)
}

type SensorRead = (usize, usize, Vec<f32>, Vec<Timing>);

fn read_pixels(section: &[u8]) -> Result<SensorRead> {
    let bytes = Bytes(section);
    if bytes.range(0, 4)? != b"SECi"
        || bytes.u32(4)? != SECTION_VERSION
        || bytes.u32(8)? != SENSOR_IMAGE_TYPE
        || bytes.u32(12)? != SENSOR_HUFFMAN_FORMAT
    {
        return Err("unsupported sensor compression".into());
    }
    // Image offsets: type/format 8/12, width/height 16/20, predictor seeds 28,
    // thirteen packed code/length words 36..62, terminator 62,
    // three compressed plane lengths 64..76, and plane data 76.
    let width = bytes.offset(16)?;
    let height = bytes.offset(20)?;
    let count = product(width, height)?;
    if width == 0
        || height == 0
        || width > u16::MAX as usize
        || height > u16::MAX as usize
        || count > huffman::MAXIMUM_PLANE_SAMPLES
    {
        return Err("unsupported sensor geometry".into());
    }
    let mut codes = Vec::with_capacity(13);
    for i in 0..13 {
        let packed = bytes.u16(36 + i * 2)?;
        codes.push((packed >> 4, (packed & 0b1111) as u8));
    }
    if bytes.u16(62)? != 0 {
        return Err("unterminated sensor code table".into());
    }
    let table = Table::new(&codes)?;
    // Validate all plane slices before starting the three bounded, independent workers.
    let mut inputs = [(&[][..], 0_u16); 3];
    let mut start: usize = 76;
    for (channel, input) in inputs.iter_mut().enumerate() {
        let length = bytes.offset(64 + channel * 4)?;
        if length == 0 {
            return Err("empty compressed sensor plane".into());
        }
        *input = (
            bytes.range(start, length)?,
            bytes.u16(28 + channel * 2)? & 0b11_1111_1111,
        );
        let aligned = length.checked_add(15).ok_or("plane length overflow")? & !0b1111;
        start = start.checked_add(aligned).ok_or("plane offset overflow")?;
    }
    let start_time = Instant::now();
    let planes = std::thread::scope(|scope| -> Result<Vec<_>> {
        let workers: [_; 3] = std::array::from_fn(|channel| {
            let table = &table;
            let (input, seed) = inputs[channel];
            scope.spawn(move || -> Result<_> {
                let start = Instant::now();
                let pixels = table.decode(input, width, height, seed)?;
                Ok((
                    pixels,
                    Timing::elapsed(
                        ["sensor plane R", "sensor plane G", "sensor plane B"][channel],
                        start,
                    ),
                ))
            })
        });
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| "sensor decoder worker panicked")?
            })
            .collect()
    })?;
    let mut timings = vec![Timing::elapsed("sensor planes wall", start_time)];
    timings.extend(planes.iter().map(|(_, timing)| timing.clone()));
    let start_time = Instant::now();
    let mut pixels = vec![0.; product(count, 3)?];
    // One contiguous pass over disjoint output triplets; iterator bounds permit autovectorization.
    for (((pixel, red), green), blue) in pixels
        .as_chunks_mut::<3>()
        .0
        .iter_mut()
        .zip(&planes[0].0)
        .zip(&planes[1].0)
        .zip(&planes[2].0)
    {
        *pixel = [*red as f32, *green as f32, *blue as f32];
    }
    timings.push(Timing::elapsed("sensor interleave", start_time));
    Ok((width, height, pixels, timings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_headers_and_directory_pointers() {
        for n in 0..44 {
            assert!(decode(&vec![0; n]).is_err());
        }
        let mut data = vec![0; 44];
        data[..4].copy_from_slice(b"FOVb");
        data[4..8].copy_from_slice(&0x30000u32.to_le_bytes());
        for pointer in [0u32, 39, 40, 44, u32::MAX] {
            data[40..].copy_from_slice(&pointer.to_le_bytes());
            assert!(decode(&data).is_err());
        }
    }

    #[test]
    fn merrill_version_23_keeps_the_base_header_directory_contract() {
        let mut data = vec![0; 44];
        data[..4].copy_from_slice(b"FOVb");
        data[4..8].copy_from_slice(&0x0002_0003_u32.to_le_bytes());
        assert_eq!(
            decode(&data).err().unwrap().to_string(),
            "X3F directory outside file"
        );
        data[4..8].copy_from_slice(&0x0002_0004_u32.to_le_bytes());
        assert_eq!(
            decode(&data).err().unwrap().to_string(),
            "unsupported X3F header"
        );
    }

    #[test]
    fn property_offsets_are_utf16_units_and_order_is_stable() {
        let mut data = vec![0; 32];
        data[..4].copy_from_slice(b"SECp");
        data[4..8].copy_from_slice(&0x20000u32.to_le_bytes());
        data[8..12].copy_from_slice(&1u32.to_le_bytes());
        data[20..24].copy_from_slice(&6u32.to_le_bytes());
        data[28..32].copy_from_slice(&2u32.to_le_bytes());
        for unit in "x\0a😀\0".encode_utf16() {
            data.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(read_properties(&data).unwrap()["x"], "a😀");
        data[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_properties(&data).is_err());
    }
}
