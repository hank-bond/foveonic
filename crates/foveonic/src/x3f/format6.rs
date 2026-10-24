//! Decode row-local Huffman differences into signed interleaved sensor codes.
//!
//! Equations and layout follow the reference implementation's independently
//! verified format-6 decoder. Each channel accumulates signed differences with
//! sixteen-bit wrapping, independently of the other channels and rows.

use super::bytes::{product, Bytes};
use crate::source::Result;

const SYMBOLS: usize = 1024;
const DATA_OFFSET: usize = 6172;
const PREFIX_BITS: usize = 12;

#[derive(Default)]
struct Node {
    children: [Option<usize>; 2],
    delta: Option<i16>,
}

struct Table {
    nodes: Vec<Node>,
    prefixes: [(Option<usize>, usize); 1 << PREFIX_BITS],
}

impl Table {
    fn new(codes: &[u32], deltas: &[i16]) -> Result<Self> {
        let mut nodes = vec![Node::default()];
        for (&word, &delta) in codes.iter().zip(deltas) {
            let length = word >> 27;
            let bits = word & 0x07ff_ffff;
            if length == 0 {
                continue;
            }
            if length > 27 || bits >= 1 << length {
                return Err("invalid format-6 Huffman code".into());
            }
            let mut index = 0;
            for bit in (0..length).rev() {
                if nodes[index].delta.is_some() {
                    return Err("overlapping format-6 Huffman codes".into());
                }
                let branch = ((bits >> bit) & 1) as usize;
                index = match nodes[index].children[branch] {
                    Some(child) => child,
                    None => {
                        let child = nodes.len();
                        nodes.push(Node::default());
                        nodes[index].children[branch] = Some(child);
                        child
                    }
                };
            }
            if nodes[index].delta.is_some() || nodes[index].children != [None; 2] {
                return Err("overlapping format-6 Huffman codes".into());
            }
            nodes[index].delta = Some(delta);
        }
        if nodes.len() == 1 {
            return Err("empty format-6 Huffman table".into());
        }
        let prefixes = std::array::from_fn(|prefix| {
            let mut node = Some(0);
            let mut length = 0;
            while length < PREFIX_BITS {
                let Some(index) = node else { break };
                if nodes[index].delta.is_some() {
                    break;
                }
                node = nodes[index].children[(prefix >> (PREFIX_BITS - 1 - length)) & 1];
                length += 1;
            }
            (node, length)
        });
        Ok(Self { nodes, prefixes })
    }

    fn row(&self, input: &[u8], output: &mut [f32]) -> Result<()> {
        let mut position = 0;
        let total = product(input.len(), 8)?;
        let mut accumulators = [0_i16; 3];
        for (sample, value) in output.iter_mut().enumerate() {
            let byte = position / 8;
            let mut word = [0_u8; 4];
            let available = input.len().saturating_sub(byte).min(4);
            word[..available].copy_from_slice(&input[byte..byte + available]);
            let prefix =
                ((u32::from_be_bytes(word) << (position % 8)) >> (32 - PREFIX_BITS)) as usize;
            let (node, length) = self.prefixes[prefix];
            position += length;
            let mut index = node.ok_or("unassigned format-6 Huffman prefix")?;
            while self.nodes[index].delta.is_none() {
                if position >= total {
                    return Err("truncated format-6 Huffman row".into());
                }
                let bit = (input[position / 8] >> (7 - position % 8)) & 1;
                position += 1;
                index = self.nodes[index].children[bit as usize]
                    .ok_or("unassigned format-6 Huffman suffix")?;
            }
            if position > total {
                return Err("truncated format-6 Huffman row".into());
            }
            let channel = sample % 3;
            accumulators[channel] =
                accumulators[channel].wrapping_add(self.nodes[index].delta.expect("leaf"));
            *value = f32::from(accumulators[channel]);
        }
        Ok(())
    }
}

pub(super) fn decode(section: &[u8], width: usize, height: usize) -> Result<Vec<f32>> {
    let bytes = Bytes(section);
    let rows_size = product(height, 4)?;
    let directory = section
        .len()
        .checked_sub(rows_size)
        .ok_or("truncated format-6 row directory")?;
    if directory < DATA_OFFSET {
        return Err("truncated format-6 tables".into());
    }
    let codes: Vec<_> = (0..SYMBOLS)
        .map(|i| bytes.u32(2076 + i * 4))
        .collect::<Result<_>>()?;
    let deltas: Vec<_> = (0..SYMBOLS)
        .map(|i| bytes.u16(28 + i * 2).map(|v| v as i16))
        .collect::<Result<_>>()?;
    let table = Table::new(&codes, &deltas)?;
    let data = bytes.range(DATA_OFFSET, directory - DATA_OFFSET)?;
    let mut rows: Vec<_> = (0..height)
        .map(|i| bytes.offset(directory + i * 4))
        .collect::<Result<_>>()?;
    rows.push(data.len());
    if rows[0] != 0 || rows.windows(2).any(|r| r[0] > r[1]) {
        return Err("invalid format-6 row offsets".into());
    }
    let row_samples = product(width, 3)?;
    let mut output = vec![0.; product(row_samples, height)?];
    for (bounds, row) in rows.windows(2).zip(output.chunks_exact_mut(row_samples)) {
        crate::background::checkpoint_or_cancel()?;
        table.row(&data[bounds[0]..bounds[1]], row)?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_reset_channels_and_preserve_signed_wrapping() {
        let table = Table::new(&[1 << 27, (1 << 27) | 1], &[-1, 32767]).unwrap();
        let mut row = [0.; 6];
        table.row(&[0b1011_0000], &mut row).unwrap();
        assert_eq!(row, [32767., -1., 32767., -2., -2., 32766.]);
        table.row(&[0], &mut row).unwrap();
        assert_eq!(row, [-1., -1., -1., -2., -2., -2.]);
    }

    #[test]
    fn rejects_ambiguous_codes_and_truncated_consumption() {
        assert!(Table::new(&[1 << 27, 2 << 27], &[0, 1]).is_err());
        assert!(Table::new(&[], &[]).is_err());
        assert!(Table::new(&[(1 << 27) | 2], &[0]).is_err());
        let table = Table::new(&[27 << 27], &[1]).unwrap();
        assert!(table.row(&[0; 3], &mut [0.]).is_err());
        table.row(&[0; 4], &mut [0.]).unwrap();
    }

    #[test]
    #[ignore = "requires an original X3F and independently captured signed sensor words"]
    fn original_samples_match_reference_capture() {
        let original = std::fs::read(std::env::var("FORMAT6_X3F").expect("FORMAT6_X3F")).unwrap();
        let bytes = Bytes(&original);
        let directory = bytes.offset(original.len() - 4).unwrap();
        let count = bytes.offset(directory + 8).unwrap();
        let section = (0..count)
            .find_map(|i| {
                let entry = directory + 12 + i * 12;
                let start = bytes.offset(entry).unwrap();
                let size = bytes.offset(entry + 4).unwrap();
                let section = bytes.range(start, size).unwrap();
                (section.starts_with(b"SECi") && Bytes(section).u32(12).unwrap() == 6)
                    .then_some(section)
            })
            .expect("format-6 section");
        let (width, height, samples, _) = super::super::read_pixels(section).unwrap();
        let expected =
            std::fs::read(std::env::var("FORMAT6_REFERENCE").expect("FORMAT6_REFERENCE")).unwrap();
        assert_eq!(expected.len(), width * height * 6);
        for (index, (actual, expected)) in
            samples.iter().zip(expected.as_chunks::<2>().0).enumerate()
        {
            assert_eq!(
                *actual,
                f32::from(i16::from_le_bytes(*expected)),
                "sample {index}"
            );
        }
    }
}
