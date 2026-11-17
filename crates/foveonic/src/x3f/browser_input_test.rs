//! Reproduces bounded metadata expansion and rejects invalid originals before payload reads.

use super::*;
use std::io::{Cursor, Read};

fn suffix_properties(count: u32, units: u32) -> Vec<u8> {
    let mut data = b"SECp".to_vec();
    for word in [SECTION_VERSION, count, 0, 0, units + 1] {
        data.extend(word.to_le_bytes());
    }
    for offset in 0..count {
        data.extend(offset.to_le_bytes());
        data.extend(units.to_le_bytes());
    }
    data.extend(std::iter::repeat_n(b'A' as u16, units as usize).flat_map(u16::to_le_bytes));
    data.extend(0_u16.to_le_bytes());
    data
}

#[test]
fn property_budget_bounds_overlapping_expansion_without_banning_shared_text() {
    let small = read_properties(&suffix_properties(3, 8)).unwrap();
    assert_eq!(small.len(), 3);
    assert_eq!(small["AAAAAAAA"], "");
    let error = read_properties(&suffix_properties(512, 65_536)).unwrap_err();
    assert!(error
        .to_string()
        .contains("decoded X3F properties exceed limit"));
    assert!(read_properties(&vec![0; MAXIMUM_PROPERTY_BYTES + 1])
        .unwrap_err()
        .to_string()
        .contains("too large"));
}

#[test]
fn utf16_budget_counts_utf8_bytes_and_shared_references() {
    let data: Vec<_> = "a😀\0".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut remaining = 5;
    assert_eq!(Bytes(&data).utf16(0, &mut remaining).unwrap(), "a😀");
    assert_eq!(remaining, 0);
    assert!(Bytes(&data).utf16(0, &mut remaining).is_err());
    assert!(Bytes(&data).utf16(0, &mut 4).is_err());
}

#[test]
fn invalid_original_header_never_reads_the_claimed_large_payload() {
    struct HeaderOnly(Cursor<[u8; 52]>);
    impl Read for HeaderOnly {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            assert!(self.0.position() < 52, "read beyond fixed header");
            self.0.read(output)
        }
    }
    for case in 0..3 {
        let mut header = [0; 52];
        header[..4].copy_from_slice(b"FOVb");
        header[4..8].copy_from_slice(&0x40001_u32.to_le_bytes());
        header[8..12].copy_from_slice(&1_u32.to_le_bytes());
        match case {
            0 => header[0] = 0,
            1 => header[4..8].copy_from_slice(&0x50000_u32.to_le_bytes()),
            _ => header[8..12].fill(0),
        }
        let mut reader = HeaderOnly(Cursor::new(header));
        assert!(read_original_bytes(&mut reader, MAXIMUM_FILE_BYTES).is_err());
        assert_eq!(reader.0.position(), 52);
    }
}

#[test]
fn original_reader_preserves_bytes_and_rejects_changed_length() {
    for version in SUPPORTED_FILE_VERSIONS {
        let mut bytes = vec![0; if version < 0x40000 { 44 } else { 56 }];
        bytes[..4].copy_from_slice(b"FOVb");
        bytes[4..8].copy_from_slice(&version.to_le_bytes());
        if version >= 0x40000 {
            bytes[8..12].copy_from_slice(&1_u32.to_le_bytes());
        }
        let length = bytes.len() as u64;
        assert_eq!(
            read_original_bytes(&mut Cursor::new(&bytes), length).unwrap(),
            bytes
        );
        assert!(read_original_bytes(&mut Cursor::new(&bytes), length + 1).is_err());
        if length > 52 {
            assert!(read_original_bytes(&mut Cursor::new(&bytes), length - 1).is_err());
        }
    }
}
