//! Decode camera metadata while preserving matrix encodings and property-list order.
//!
//! Imported from Foveonic's X3F reader.

use super::bytes::{product, Bytes};
use super::huffman::Table;
use crate::source::{Matrix, MatrixEncoding, PropertyList, Result};
use std::collections::BTreeMap;

pub(super) type Lists = BTreeMap<String, PropertyList>;

const PACKED_HUFFMAN_COMPRESSION: u32 = 4;
const MATRIX_BLOCK_VERSION: u32 = 0x0002_0000;
const LIST_BLOCK_VERSION: u32 = 0x0001_0000;
const BLOCK_HEADER_BYTES: usize = 20;
const MAXIMUM_DECODED_BYTES: usize = 64 * 1024 * 1024;

pub(super) fn decompress(section: &[u8]) -> Result<Vec<u8>> {
    let bytes = Bytes(section);
    if bytes.range(0, 4)? != b"SECc" || bytes.u32(8)? != PACKED_HUFFMAN_COMPRESSION {
        return Err("unsupported camera metadata compression".into());
    }
    // Type-4 header byte offsets: decoded length 12, predictor seed 16,
    // plane width/height 20/24, thirteen (length, code) pairs 28..54,
    // zero terminator 54, compressed length 56, and bitstream 60.
    let declared = bytes.offset(12)?;
    let seed = bytes.u32(16)?;
    let width = bytes.offset(20)?;
    let height = bytes.offset(24)?;
    if seed > super::huffman::SAMPLE_MAX
        || !(BLOCK_HEADER_BYTES..=MAXIMUM_DECODED_BYTES).contains(&declared)
    {
        return Err("invalid camera metadata size or seed".into());
    }
    let count = product(width, height)?;
    if !count.is_multiple_of(2) || declared > product(count / 2, 3)? {
        return Err("invalid packed camera metadata geometry".into());
    }
    let mut codes = Vec::new();
    for i in 0..13 {
        let pair = bytes.range(28 + i * 2, 2)?;
        codes.push(((pair[1] as u16) << 4, pair[0]));
    }
    if bytes.u16(54)? != 0 {
        return Err("unterminated camera metadata code table".into());
    }
    let input = bytes.range(60, bytes.offset(56)?)?;
    let plane = Table::new(&codes)?.decode(input, width, height, seed as u16)?;
    let mut output = Vec::with_capacity(product(count / 2, 3)?);
    // Two twelve-bit samples occupy three bytes: aaaaaaaa aaaabbbb bbbbbbbb.
    for pair in plane.as_chunks::<2>().0 {
        output.push((pair[0] >> 4) as u8);
        output.push(((pair[0] << 4) | (pair[1] >> 8)) as u8);
        output.push(pair[1] as u8);
    }
    // The declared byte prefix contains the blocks. Remaining packed samples are padding.
    output.truncate(declared);
    Ok(output)
}

pub(crate) fn parse(data: &[u8]) -> Result<(BTreeMap<String, Matrix>, Lists)> {
    let mut matrices = BTreeMap::new();
    let mut lists = BTreeMap::new();
    let mut offset = 0;
    while offset < data.len() {
        let remaining = Bytes(&data[offset..]);
        // Block header offsets: kind 0, version 4, size 8, name 12, payload 16.
        let kind = remaining.range(0, 4)?;
        let size = remaining.offset(8)?;
        if size < BLOCK_HEADER_BYTES {
            return Err("invalid camera metadata block size".into());
        }
        let block = Bytes(remaining.range(0, size)?);
        let name = block.text(block.offset(12)?)?;
        let payload = block.offset(16)?;
        if payload < BLOCK_HEADER_BYTES || payload > size || name.is_empty() {
            return Err("invalid camera metadata block header".into());
        }
        match kind {
            b"CMbM" => {
                if block.u32(4)? != MATRIX_BLOCK_VERSION {
                    return Err("unsupported camera matrix version".into());
                }
                let matrix = matrix(block, payload)?;
                if matrices
                    .keys()
                    .any(|key: &String| key.eq_ignore_ascii_case(&name))
                {
                    return Err("duplicate camera matrix name".into());
                }
                matrices.insert(name, matrix);
            }
            b"CMbP" => {
                if block.u32(4)? != LIST_BLOCK_VERSION {
                    return Err("unsupported camera list version".into());
                }
                // List payload: count, string-base offset, then pairs of relative
                // name/value offsets. Header and each pair occupy eight bytes.
                let count = block.offset(payload)?;
                let strings = block.offset(payload + 4)?;
                block.range(payload + 8, product(count, 8)?)?;
                let mut entries = Vec::with_capacity(count);
                for i in 0..count {
                    let entry = payload + 8 + i * 8;
                    let key = block.text(
                        strings
                            .checked_add(block.offset(entry)?)
                            .ok_or("list offset overflow")?,
                    )?;
                    let value = block.text(
                        strings
                            .checked_add(block.offset(entry + 4)?)
                            .ok_or("list offset overflow")?,
                    )?;
                    entries.push((key, value));
                }
                if lists.insert(name, PropertyList::new(entries)?).is_some() {
                    return Err("duplicate camera list name".into());
                }
            }
            b"CMbT" | b"CMbB" => (),
            _ => return Err("unsupported camera metadata block".into()),
        }
        offset += size;
    }
    Ok((matrices, lists))
}

fn matrix(block: Bytes<'_>, payload: usize) -> Result<Matrix> {
    // Matrix payload: encoding/rank/data-offset (twelve bytes), followed by
    // twelve-byte dimensions (extent, name offset, storage metadata).
    let encoding = MatrixEncoding::try_from(block.u32(payload)?)?;
    let rank = block.offset(payload + 4)?;
    let offset = block.offset(payload + 8)?;
    if rank == 0 || rank > 8 {
        return Err("unsupported camera matrix rank".into());
    }
    block.range(payload + 12, product(rank, 12)?)?;
    let mut dimensions = Vec::with_capacity(rank);
    let mut count = 1;
    for i in 0..rank {
        let length = block.offset(payload + 12 + i * 12)?;
        count = product(count, length)?;
        dimensions.push(length);
    }
    let (size, representation) = match encoding {
        MatrixEncoding::Int16 => (2, "i32"),
        MatrixEncoding::Int32 => (4, "i32"),
        MatrixEncoding::Uint32 => (4, "u32"),
        MatrixEncoding::Float32 => (4, "f64"),
        MatrixEncoding::Uint8 => (1, "u32"),
        MatrixEncoding::Uint16 => (2, "u32"),
    };
    let raw = Bytes(block.range(offset, product(count, size)?)?);
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        let position = i * size;
        let value = match encoding {
            MatrixEncoding::Int16 => raw.u16(position)? as i16 as f64,
            MatrixEncoding::Int32 => raw.u32(position)? as i32 as f64,
            MatrixEncoding::Uint32 => raw.u32(position)? as f64,
            MatrixEncoding::Float32 => f32::from_bits(raw.u32(position)?) as f64,
            MatrixEncoding::Uint8 => raw.0[position] as f64,
            MatrixEncoding::Uint16 => raw.u16(position)? as f64,
        };
        if !value.is_finite() {
            return Err("nonfinite camera matrix".into());
        }
        values.push(value);
    }
    Ok(Matrix {
        encoding,
        representation,
        dimensions,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(encoding: u32, values: &[u8]) -> Vec<u8> {
        let mut block = vec![0; 52];
        block[..4].copy_from_slice(b"CMbM");
        for (at, value) in [
            (4, 0x20000),
            (8, 52 + values.len() as u32),
            (12, 20),
            (16, 24),
            (24, encoding),
            (28, 1),
            (32, 52),
            (36, 2),
        ] {
            block[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        block[20] = b'x';
        block.extend_from_slice(values);
        block
    }

    #[test]
    fn matrix_encoding_is_distinct_from_widened_values() {
        for (encoding, raw, expected) in [
            (0, vec![255, 255, 0, 128], vec![-1., -32768.]),
            (6, vec![255, 255, 0, 128], vec![65535., 32768.]),
            (
                1,
                vec![255, 255, 255, 255, 0, 0, 0, 128],
                vec![-1., -2147483648.],
            ),
            (
                2,
                vec![255, 255, 255, 255, 0, 0, 0, 128],
                vec![4294967295., 2147483648.],
            ),
            (3, vec![0, 0, 128, 63, 0, 0, 0, 128], vec![1., -0.]),
            (5, vec![1, 255], vec![1., 255.]),
        ] {
            let (m, _) = parse(&block(encoding, &raw)).unwrap();
            assert_eq!(u32::from(m["x"].encoding), encoding);
            assert_eq!(m["x"].dimensions, [2]);
            assert_eq!(
                m["x"]
                    .values
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|v: &f64| v.to_bits())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn matrix_names_are_unambiguous_under_native_case_ignored_lookup() {
        let first = block(6, &[1, 0, 2, 0]);
        let mut second = first.clone();
        second[20] = b'X';
        assert!(parse(&[first, second].concat()).is_err());
    }

    #[test]
    fn invalid_sizes_duplicates_and_nonfinite_values_fail() {
        let valid = block(6, &[1, 0, 2, 0]);
        for end in 0..valid.len() {
            assert!(parse(&valid[..end]).is_err() || end == 0);
        }
        let mut duplicate = valid.clone();
        duplicate.extend_from_slice(&valid);
        assert!(parse(&duplicate).is_err());
        assert!(parse(&block(4, &[0; 16])).is_err());
        assert!(parse(&block(3, &[0, 0, 192, 127, 0, 0, 0, 0])).is_err());
        let mut bad = valid;
        bad[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&bad).is_err());
    }
}
