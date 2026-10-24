//! Reads owned X3F sections and decodes sensor samples and camera metadata.
//!
//! Imported from Foveonic's X3F reader.

pub(crate) mod adjustments;
mod bytes;
pub(crate) mod camf;
pub(crate) mod capture;
mod exif;
mod format31;
mod format6;
mod huffman;

use crate::profile::Timing;
use crate::source::{Metadata, Result, Source, SourceStorage};
use bytes::{product, Bytes};
use huffman::Table;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

// Active photos have independent plane workers. Speculative photos share a bounded
// FIFO layer pool across their nearest-neighbor queue.
fn decode_planes<T: Send>(decode: impl Fn(usize) -> Result<T> + Sync) -> Result<Vec<T>> {
    const SENSOR_PLANES: usize = 3;
    if let Some(task) = crate::background::decoder_context() {
        return task.pool.planes(&task.turn, decode);
    }
    std::thread::scope(|scope| {
        let workers: [_; SENSOR_PLANES] = std::array::from_fn(|channel| {
            let decode = &decode;
            let scheduling = crate::background::context();
            scope.spawn(move || crate::background::with_context(scheduling, || decode(channel)))
        });
        // Join every child, including after a failure, before leaving the scope.
        workers
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| "sensor decoder worker panicked".into())
                    .and_then(|result| result)
            })
            .into_iter()
            .collect()
    })
}

const MINIMUM_FILE_BYTES: u64 = 44; // Forty-byte base header and trailing directory pointer.
const MAXIMUM_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const BASE_HEADER_BYTES: usize = 40;
const SECTION_VERSION: u32 = 0x0002_0000;
// Version words hold major/minor halves. DP1 Merrill review originals use
// 2.3 with the same directory, property, CAMF, and format-30 sensor sections.
const SUPPORTED_FILE_VERSIONS: [u32; 7] = [
    0x0002_0000,
    0x0002_0001,
    0x0002_0002,
    0x0002_0003,
    0x0003_0000,
    0x0004_0001,
    0x0004_0002,
];
const MAXIMUM_DIRECTORY_ENTRIES: usize = 1024;
const DIRECTORY_HEADER_BYTES: usize = 12;
const DIRECTORY_ENTRY_BYTES: usize = 12;
const SENSOR_IMAGE_TYPES: [u32; 2] = [1, 3];
const SENSOR_HUFFMAN_FORMAT: u32 = 30;
const PROPERTY_HEADER_BYTES: usize = 24;
const PROPERTY_ENTRY_BYTES: usize = 8;

pub(crate) use camf::parse as cached_camera_metadata;
pub(crate) use exif::color_space as jpeg_color_space;
pub(crate) use exif::thumbnail as jpeg_thumbnail;

pub(crate) fn read(path: &Path, read_complete: impl FnOnce()) -> Result<Source> {
    let start = Instant::now();
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    if !(MINIMUM_FILE_BYTES..=MAXIMUM_FILE_BYTES).contains(&length) {
        return Err("unsupported X3F file size".into());
    }
    let mut data = Vec::with_capacity(length as usize);
    use std::io::Read;
    // Yield background I/O between bounded reads instead of issuing the whole
    // original at once when the next selected photo needs the storage device.
    let mut limited = (&mut file).take(length + 1);
    loop {
        crate::background::checkpoint_or_cancel()?;
        let before = data.len();
        limited
            .by_ref()
            .take(crate::background::IO_CHUNK_BYTES as u64)
            .read_to_end(&mut data)?;
        if data.len() == before {
            break;
        }
    }
    if data.len() as u64 != length {
        return Err("X3F file changes during reading".into());
    }
    let read = Timing::elapsed("file read", start);
    read_complete();
    let start = Instant::now();
    let mut source = decode(&data)?;
    crate::background::checkpoint_or_cancel()?;
    // Parallel-plane timings overlap this total, so adding them would count
    // part of the same decode twice. Metadata parsing is included here too.
    let decode = Timing::elapsed("X3F decode wall", start);
    source.timings.splice(0..0, [read, decode]);
    Ok(source)
}

fn decode(data: &[u8]) -> Result<Source> {
    let bytes = Bytes(data);
    if bytes.range(0, 4)? != b"FOVb" || !SUPPORTED_FILE_VERSIONS.contains(&bytes.u32(4)?) {
        return Err("unsupported X3F header".into());
    }
    let rotation = rotation(data)?;
    let processing_version = processing_version(data)?;
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
    let mut preview_properties = BTreeMap::new();
    let mut preview_area = 0_u64;
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
                if image.u32(8)? == 2 && image.u32(12)? == 18 {
                    let area = u64::from(image.u32(16)?) * u64::from(image.u32(20)?);
                    if area > preview_area {
                        let values =
                            exif::properties(image.range(28, section.len().saturating_sub(28))?);
                        if !values.is_empty() {
                            preview_properties = values;
                            preview_area = area;
                        }
                    }
                }
                if SENSOR_IMAGE_TYPES.contains(&image.u32(8)?) {
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
    if properties.is_none() && bytes.u32(4)? < 0x40000 {
        return Err("missing X3F properties".into());
    }
    let mut properties = properties
        .map(read_properties)
        .transpose()?
        .unwrap_or_default();
    if properties.is_empty()
        || properties.get("CAMMODEL").is_some_and(|m| {
            m.trim().to_ascii_uppercase().starts_with("SIGMA ") && !m.contains("Merrill")
        })
    {
        merge_preview_properties(&mut properties, preview_properties);
    }
    properties.extend(adjustments::read(&data[..ranges[0].0])?);
    properties.insert("X3F_MARK_BITS".into(), mark_bits(data)?.to_string());
    let raw = raw.ok_or("missing X3F sensor image")?;
    let storage = if [35, 37, 39].contains(&Bytes(raw).u32(12)?) {
        SourceStorage::PackedRgbBluePhases
    } else {
        SourceStorage::InterleavedRgb
    };
    let (width, height, pixels, mut timings) = read_pixels(raw)?;
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
            processing_version,
            properties,
            matrices,
            lists,
        }),
        pixels,
        storage,
        camf,
        timings,
    })
}

/// Older PROP records keep their authoritative camera choices and exposure units.
/// Files without PROP receive the complete preview capture fallback.
fn merge_preview_properties(
    properties: &mut BTreeMap<String, String>,
    preview: BTreeMap<String, String>,
) {
    let fallback = properties.is_empty();
    properties.extend(
        preview
            .into_iter()
            .filter(|(name, _)| fallback || name.starts_with("EXIF_")),
    );
}

/// Clockwise presentation degrees. Version 4 moves image geometry twelve bytes
/// later to make room for processing-version and camera identity fields.
pub(crate) fn rotation(header: &[u8]) -> Result<u32> {
    let bytes = Bytes(header);
    bytes.u32(if bytes.u32(4)? >= 0x40000 { 48 } else { 36 })
}

/// File marks include the saved color/monochrome presentation choice.
pub(super) fn mark_bits(header: &[u8]) -> Result<u32> {
    let bytes = Bytes(header);
    bytes.u32(if bytes.u32(4)? >= 0x40000 { 36 } else { 24 })
}

pub(super) fn processing_version(header: &[u8]) -> Result<Option<[u16; 4]>> {
    let bytes = Bytes(header);
    if bytes.u32(4)? < 0x40000 {
        return Ok(None);
    }
    if bytes.u32(8)? != 1 {
        return Err("unsupported X3F byte order".into());
    }
    Ok(Some([
        bytes.u16(12)?,
        bytes.u16(14)?,
        bytes.u16(16)?,
        bytes.u16(18)?,
    ]))
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
        || !SENSOR_IMAGE_TYPES.contains(&bytes.u32(8)?)
        || ![6, SENSOR_HUFFMAN_FORMAT, 35, 37, 39].contains(&bytes.u32(12)?)
    {
        return Err("unsupported sensor compression".into());
    }
    // Both encodings store image dimensions at offsets 16 and 20.
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
    if [35, 37, 39].contains(&bytes.u32(12)?) {
        let start = Instant::now();
        let pixels = format31::decode(section, width, height, bytes.u32(12)?)?;
        return Ok((
            width,
            height,
            pixels,
            vec![Timing::elapsed("sensor packed planes", start)],
        ));
    }
    if bytes.u32(12)? == 6 {
        let start = Instant::now();
        let pixels = crate::decode_pool::one(|| format6::decode(section, width, height))?;
        return Ok((
            width,
            height,
            pixels,
            vec![Timing::elapsed("sensor interleaved Huffman", start)],
        ));
    }
    // Format 30 stores predictor seeds at 28, thirteen packed code/length words
    // at 36..62, a terminator at 62, and three plane lengths at 64..76.
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
    let planes = decode_planes(|channel| {
        let (input, seed) = inputs[channel];
        let start = Instant::now();
        let pixels = table.decode(input, width, height, seed)?;
        Ok((
            pixels,
            Timing::elapsed(
                ["sensor plane R", "sensor plane G", "sensor plane B"][channel],
                start,
            ),
        ))
    })?;
    let mut timings = vec![Timing::elapsed("sensor planes wall", start_time)];
    timings.extend(planes.iter().map(|(_, timing)| timing.clone()));
    let start_time = Instant::now();
    let mut pixels = vec![0.; product(count, 3)?];
    // Disjoint contiguous triplets let the compiler vectorize this pass
    // without changing the channel-interleaving order.
    // Keep the existing contiguous triplets while bounding speculative CPU work.
    let samples_per_chunk = crate::background::IO_CHUNK_BYTES / std::mem::size_of::<f32>() / 3;
    for (((chunk, red), green), blue) in pixels
        .chunks_mut(samples_per_chunk * 3)
        .zip(planes[0].0.chunks(samples_per_chunk))
        .zip(planes[1].0.chunks(samples_per_chunk))
        .zip(planes[2].0.chunks(samples_per_chunk))
    {
        crate::background::checkpoint_or_cancel()?;
        for (((pixel, red), green), blue) in chunk
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .zip(red)
            .zip(green)
            .zip(blue)
        {
            *pixel = [*red as f32, *green as f32, *blue as f32];
        }
    }
    timings.push(Timing::elapsed("sensor interleave", start_time));
    Ok((width, height, pixels, timings))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_uses_the_file_version_geometry_offset() {
        for version in SUPPORTED_FILE_VERSIONS {
            for degrees in [0_u32, 90, 180, 270] {
                let mut header = [0_u8; 52];
                header[4..8].copy_from_slice(&version.to_le_bytes());
                let offset = if version >= 0x40000 { 48 } else { 36 };
                let other_offset = if offset == 48 { 36 } else { 48 };
                header[other_offset..other_offset + 4].copy_from_slice(&123_u32.to_le_bytes());
                header[offset..offset + 4].copy_from_slice(&degrees.to_le_bytes());
                assert_eq!(rotation(&header).unwrap(), degrees);
                assert!(rotation(&header[..offset + 3]).is_err());
            }
        }
    }

    #[test]
    fn processing_version_is_header_version_gated_and_keeps_word_order() {
        let mut header = [0_u8; 40];
        header[4..8].copy_from_slice(&0x30000_u32.to_le_bytes());
        header[12..20].fill(255);
        assert_eq!(processing_version(&header).unwrap(), None);
        header[4..8].copy_from_slice(&0x40001_u32.to_le_bytes());
        header[8..12].copy_from_slice(&1_u32.to_le_bytes());
        for (i, word) in [3_u16, 1, 2, 0].into_iter().enumerate() {
            header[12 + i * 2..14 + i * 2].copy_from_slice(&word.to_le_bytes());
        }
        assert_eq!(processing_version(&header).unwrap(), Some([3, 1, 2, 0]));
        header[8..12].copy_from_slice(&2_u32.to_le_bytes());
        assert!(processing_version(&header).is_err());
    }

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
