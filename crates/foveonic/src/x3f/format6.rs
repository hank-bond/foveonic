//! Decode row-local Huffman differences into signed interleaved sensor codes.
//!
//! Equations and layout follow the reference implementation's independently
//! verified format-6 decoder. Each channel accumulates signed differences with
//! sixteen-bit wrapping, independently of the other channels and rows.

use super::bytes::{product, Bytes};
use crate::diagnostics::{self, Category};
use crate::input_error::damaged;
use crate::source::Result;

const SYMBOLS: usize = 1024;
const DATA_OFFSET: usize = 6172;
const PREFIX_BITS: usize = 12;
/// Rows each worker decodes in lockstep. Rows are independent and have the
/// same sample count, so one core overlaps their symbol lookups.
const ROW_LANES: usize = 4;

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
                return Err(damaged("invalid format-6 Huffman code"));
            }
            let mut index = 0;
            for bit in (0..length).rev() {
                if nodes[index].delta.is_some() {
                    return Err(damaged("overlapping format-6 Huffman codes"));
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
                return Err(damaged("overlapping format-6 Huffman codes"));
            }
            nodes[index].delta = Some(delta);
        }
        if nodes.len() == 1 {
            return Err(damaged("empty format-6 Huffman table"));
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

    /// Decode one difference at `position`, a bit offset into the row.
    #[inline(always)]
    fn difference(&self, input: &[u8], total: usize, position: &mut usize) -> Result<i16> {
        let byte = *position / 8;
        let mut word = [0_u8; 4];
        let available = input.len().saturating_sub(byte).min(4);
        word[..available].copy_from_slice(&input[byte..byte + available]);
        let prefix = ((u32::from_be_bytes(word) << (*position % 8)) >> (32 - PREFIX_BITS)) as usize;
        let (node, length) = self.prefixes[prefix];
        *position += length;
        let mut index = node.ok_or_else(|| damaged("unassigned format-6 Huffman prefix"))?;
        while self.nodes[index].delta.is_none() {
            if *position >= total {
                return Err(damaged("truncated format-6 Huffman row"));
            }
            let bit = (input[*position / 8] >> (7 - *position % 8)) & 1;
            *position += 1;
            index = self.nodes[index].children[bit as usize]
                .ok_or_else(|| damaged("unassigned format-6 Huffman suffix"))?;
        }
        if *position > total {
            return Err(damaged("truncated format-6 Huffman row"));
        }
        Ok(self.nodes[index].delta.expect("leaf"))
    }

    fn row(&self, input: &[u8], output: &mut [f32]) -> Result<()> {
        let total = product(input.len(), 8)?;
        let mut position = 0;
        let mut accumulators = [0_i16; 3];
        for (sample, value) in output.iter_mut().enumerate() {
            let channel = sample % 3;
            accumulators[channel] =
                accumulators[channel].wrapping_add(self.difference(input, total, &mut position)?);
            *value = f32::from(accumulators[channel]);
        }
        Ok(())
    }

    /// Decode rows with the same sample count in lockstep. False on any damage,
    /// so the caller replays them in order and reports the first row's cause.
    // Every lane advances one sample per step, and the lane indexes five arrays.
    #[allow(clippy::needless_range_loop)]
    fn rows(&self, inputs: [&[u8]; ROW_LANES], outputs: [&mut [f32]; ROW_LANES]) -> bool {
        let mut totals = [0; ROW_LANES];
        for (total, input) in totals.iter_mut().zip(inputs) {
            let Ok(bits) = product(input.len(), 8) else {
                return false;
            };
            *total = bits;
        }
        let mut positions = [0; ROW_LANES];
        let mut accumulators = [[0_i16; 3]; ROW_LANES];
        let samples = outputs[0].len();
        for sample in 0..samples {
            let channel = sample % 3;
            for lane in 0..ROW_LANES {
                let Ok(difference) =
                    self.difference(inputs[lane], totals[lane], &mut positions[lane])
                else {
                    return false;
                };
                let accumulator = &mut accumulators[lane][channel];
                *accumulator = accumulator.wrapping_add(difference);
                outputs[lane][sample] = f32::from(*accumulator);
            }
        }
        true
    }
}

pub(super) fn decode(section: &[u8], width: usize, height: usize) -> Result<Vec<f32>> {
    let prepare_scope = diagnostics::scope(Category::Cpu, "decode.sensor.prepare");
    let bytes = Bytes(section);
    let rows_size = product(height, 4)?;
    let directory = section
        .len()
        .checked_sub(rows_size)
        .ok_or_else(|| damaged("truncated format-6 row directory"))?;
    if directory < DATA_OFFSET {
        return Err(damaged("truncated format-6 tables"));
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
        return Err(damaged("invalid format-6 row offsets"));
    }
    let row_samples = product(width, 3)?;
    drop(prepare_scope);
    let mut output = {
        let _scope = diagnostics::scope(Category::Cpu, "decode.sensor.output.allocate");
        vec![0.; product(row_samples, height)?]
    };
    let _scope = diagnostics::scope(Category::Cpu, "decode.sensor.rows");
    #[cfg(test)]
    if parallel_tests::SERIAL.load(std::sync::atomic::Ordering::Relaxed) {
        decode_rows(&table, data, &rows, row_samples, &mut output)?;
        return Ok(output);
    }
    crate::decode_pool::rows(&mut output, row_samples, |first_row, output| {
        decode_rows(&table, data, &rows[first_row..], row_samples, output)
    })?;
    Ok(output)
}

#[cfg(test)]
#[path = "format6_parallel_test.rs"]
mod parallel_tests;

fn decode_rows(
    table: &Table,
    data: &[u8],
    rows: &[usize],
    row_samples: usize,
    output: &mut [f32],
) -> Result<()> {
    let row = |index: usize| &data[rows[index]..rows[index + 1]];
    for (group, outputs) in output.chunks_mut(row_samples * ROW_LANES).enumerate() {
        crate::priority::checkpoint_or_cancel()?;
        let first = group * ROW_LANES;
        if outputs.len() == row_samples * ROW_LANES {
            let mut lanes = outputs.chunks_exact_mut(row_samples);
            let lanes = std::array::from_fn(|_| lanes.next().expect("full group"));
            if table.rows(std::array::from_fn(|lane| row(first + lane)), lanes) {
                continue;
            }
        }
        for (offset, output) in outputs.chunks_exact_mut(row_samples).enumerate() {
            table.row(row(first + offset), output)?;
        }
    }
    Ok(())
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
    fn lockstep_rows_match_serial_rows_and_report_the_first_damaged_row() {
        let table = Table::new(&[1 << 27, (1 << 27) | 1], &[-1, 32767]).unwrap();
        let data = [0b1011_0000, 0b0110_1100, 0xff, 0, 0b1001_0110];
        let rows = [0, 1, 2, 3, 4, 5];
        let mut lanes = vec![0.; 5 * 6];
        decode_rows(&table, &data, &rows, 6, &mut lanes).unwrap();
        let mut serial = vec![0.; 5 * 6];
        for (index, output) in serial.as_chunks_mut::<6>().0.iter_mut().enumerate() {
            table
                .row(&data[rows[index]..rows[index + 1]], output)
                .unwrap();
        }
        assert_eq!(lanes, serial);

        // Row 0 decodes two samples before an unassigned prefix. Row 1 is empty
        // and fails first in lockstep, but row 0's cause must be reported.
        let table = Table::new(&[2 << 27], &[1]).unwrap();
        let data = [0b0000_1000, 0, 0];
        let rows = [0, 1, 1, 2, 3];
        let mut output = vec![0.; 4 * 3];
        let error = decode_rows(&table, &data, &rows, 3, &mut output).unwrap_err();
        let first = table.row(&data[..1], &mut [0.; 3]).unwrap_err();
        assert_eq!(error.to_string(), first.to_string());
        assert!(error
            .to_string()
            .contains("unassigned format-6 Huffman prefix"));
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
        let (width, height, _, samples, _) = super::super::read_pixels(section).unwrap();
        let crate::source::SensorSamples::Float(samples) = samples else {
            panic!("Classic samples retain their float representation");
        };
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
