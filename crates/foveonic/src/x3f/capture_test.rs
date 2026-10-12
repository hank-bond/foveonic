//! Checks property-only reads, shared UTF-16 parsing, and bounded malformed-input rejection.

use super::*;
use std::io::{self, Cursor};

struct Tracked {
    bytes: Cursor<Vec<u8>>,
    reads: Vec<std::ops::Range<u64>>,
}
impl Read for Tracked {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let start = self.bytes.position();
        let count = self.bytes.read(buffer)?;
        self.reads.push(start..start + count as u64);
        Ok(count)
    }
}
impl Seek for Tracked {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.bytes.seek(position)
    }
}
fn put(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn fixture() -> Tracked {
    let mut properties = vec![0; 32];
    properties[..4].copy_from_slice(b"SECp");
    put(&mut properties, 4, SECTION_VERSION);
    put(&mut properties, 8, 1);
    put(&mut properties, 28, 2);
    let strings: Vec<_> = "x\0a😀\0".encode_utf16().collect();
    put(&mut properties, 20, strings.len() as u32);
    properties.extend(strings.into_iter().flat_map(u16::to_le_bytes));
    // Intentionally invalid sensor/CAMF payloads: the capture reader must never parse them.
    let mut bytes = vec![0xaa; 40 + 65536 + 1024];
    bytes[..4].copy_from_slice(b"FOVb");
    put(&mut bytes, 4, 0x20003);
    let prop_start = bytes.len();
    bytes.extend(&properties);
    let directory = bytes.len();
    bytes.extend(b"SECd");
    bytes.extend(SECTION_VERSION.to_le_bytes());
    bytes.extend(3_u32.to_le_bytes());
    for (offset, size, kind) in [
        (40, 65536, *b"IMA2"),
        (40 + 65536, 1024, *b"CAMF"),
        (prop_start, properties.len(), *b"PROP"),
    ] {
        bytes.extend((offset as u32).to_le_bytes());
        bytes.extend((size as u32).to_le_bytes());
        bytes.extend(kind);
    }
    bytes.extend((directory as u32).to_le_bytes());
    Tracked {
        bytes: Cursor::new(bytes),
        reads: vec![],
    }
}
fn directory(reader: &Tracked) -> usize {
    let bytes = reader.bytes.get_ref();
    u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap()) as usize
}

#[test]
fn reads_only_header_directory_and_properties_and_supports_original_versions() {
    for version in SUPPORTED_FILE_VERSIONS {
        let mut reader = fixture();
        put(reader.bytes.get_mut(), 4, version);
        assert_eq!(read(&mut reader).unwrap()["x"], "a😀");
        assert!(reader
            .reads
            .iter()
            .all(|range| range.end <= 40 || range.start >= 40 + 65536 + 1024));
        assert!(
            reader
                .reads
                .iter()
                .map(|range| range.end - range.start)
                .sum::<u64>()
                < 256
        );
    }
}

#[test]
fn rejects_missing_duplicate_overlapping_and_out_of_bounds_sections() {
    for case in 0..4 {
        let mut reader = fixture();
        let d = directory(&reader);
        let entries = d + 12;
        let bytes = reader.bytes.get_mut();
        match case {
            0 => bytes[entries + 24 + 8..entries + 24 + 12].copy_from_slice(b"NONE"),
            1 => bytes[entries + 8..entries + 12].copy_from_slice(b"PROP"),
            2 => put(bytes, entries + 24, 40),
            _ => put(bytes, entries + 24 + 4, u32::MAX),
        }
        assert!(read(&mut reader).is_err(), "case {case}");
    }
}

#[test]
fn rejects_oversized_properties_before_reading_the_payload() {
    let mut bytes = vec![0; 40 + MAXIMUM_PROPERTY_BYTES + 1];
    bytes[..4].copy_from_slice(b"FOVb");
    put(&mut bytes, 4, 0x20003);
    let directory = bytes.len();
    bytes.extend(b"SECd");
    bytes.extend(SECTION_VERSION.to_le_bytes());
    bytes.extend(1_u32.to_le_bytes());
    bytes.extend(40_u32.to_le_bytes());
    bytes.extend((MAXIMUM_PROPERTY_BYTES as u32 + 1).to_le_bytes());
    bytes.extend(b"PROP");
    bytes.extend((directory as u32).to_le_bytes());
    let mut reader = Tracked {
        bytes: Cursor::new(bytes),
        reads: vec![],
    };
    assert!(read(&mut reader)
        .unwrap_err()
        .to_string()
        .contains("too large"));
    assert!(reader
        .reads
        .iter()
        .all(|range| range.end <= 40 || range.start >= directory as u64));
}

#[test]
fn rejects_truncated_headers_invalid_directories_and_bad_properties() {
    for size in 0..44 {
        assert!(read(&mut Cursor::new(vec![0; size])).is_err());
    }
    for case in 0..5 {
        let mut reader = fixture();
        let d = directory(&reader);
        let bytes = reader.bytes.get_mut();
        match case {
            0 => bytes[0] = 0,
            1 => {
                let end = bytes.len() - 4;
                put(bytes, end, u32::MAX);
            }
            2 => put(bytes, d + 8, u32::MAX),
            3 => put(bytes, 40 + 65536 + 1024 + 28, u32::MAX),
            _ => put(bytes, d + 4, 0),
        }
        assert!(read(&mut reader).is_err(), "case {case}");
    }
}
