//! Decode category-coded differences with two-column and two-row sensor predictors.
//!
//! Imported from Foveonic's X3F reader.

use super::bytes::product;
use crate::diagnostics::{self, Category};
use crate::input_error::damaged;
use crate::{decode_pool, profile::Timing, source::Result};
use std::time::Instant;

#[path = "huffman_parallel.rs"]
mod parallel;

pub(super) const SAMPLE_MAX: u32 = 4095;
pub(super) const MAXIMUM_PLANE_SAMPLES: usize = 32_000_000;
const CODE_BITS: usize = 12;
const PREFIX_TABLE_ENTRIES: usize = 1 << CODE_BITS;
const PACKED_CATEGORY_COUNT: usize = 18;
const TWELVE_BIT_CATEGORY_COUNT: usize = 13;
const REFILL_BYTES: usize = 6;
/// A plane splits only into fragments at least this long, so short streams stay whole.
const MINIMUM_FRAGMENT_BYTES: usize = 512 * 1024;
const REFILL_BITS: u32 = (REFILL_BYTES * 8) as u32;

#[derive(Clone, Copy)]
struct Symbol {
    length: u8,
    category: u8,
}

#[derive(Clone, Copy, Default)]
struct Difference {
    length: u8,
    value: i16,
}

pub(super) struct Table([Symbol; PREFIX_TABLE_ENTRIES]);

#[derive(Clone, Copy, Default)]
pub(super) struct Plane<'a> {
    pub input: &'a [u8],
    pub width: usize,
    pub height: usize,
    pub seed: u16,
}

impl Table {
    /// Codes occupy the high bits of a twelve-bit word, in category order.
    pub(super) fn new(codes: &[(u16, u8)]) -> Result<Self> {
        if codes.is_empty() || codes.len() > TWELVE_BIT_CATEGORY_COUNT {
            return Err("unsupported Huffman category count".into());
        }
        let mut table = [Symbol {
            length: 0,
            category: 0,
        }; PREFIX_TABLE_ENTRIES];
        for (category, &(code, length)) in codes.iter().enumerate() {
            if !(1..=12).contains(&length) {
                return Err(damaged("invalid Huffman code length"));
            }
            let span = 1usize << (12 - length);
            if code as usize >= table.len() || !(code as usize).is_multiple_of(span) {
                return Err(damaged("invalid Huffman code alignment"));
            }
            for entry in &mut table[code as usize..code as usize + span] {
                if entry.length != 0 {
                    return Err(damaged("overlapping Huffman codes"));
                }
                *entry = Symbol {
                    length,
                    category: category as u8,
                };
            }
        }
        Ok(Self(table))
    }

    fn difference(&self, stream: &mut Bits<'_>) -> Result<i16> {
        let symbol = self.0[stream.peek() as usize];
        if symbol.length == 0 {
            return Err(damaged("invalid Huffman prefix"));
        }
        stream.advance(symbol.length)?;
        if symbol.category == 0 {
            return Ok(0);
        }
        let value = stream.read(symbol.category)? as i32;
        Ok((if value >= 1 << (symbol.category - 1) {
            value
        } else {
            value + 1 - (1 << symbol.category)
        }) as i16)
    }

    /// Fold short category payloads into the existing twelve-bit lookahead.
    /// Longer payloads retain the checked category decoder.
    fn differences(&self) -> [Difference; PREFIX_TABLE_ENTRIES] {
        std::array::from_fn(|prefix| {
            let symbol = self.0[prefix];
            let length = symbol.length + symbol.category;
            if symbol.length == 0 || length > CODE_BITS as u8 {
                return Difference::default();
            }
            let value = if symbol.category == 0 {
                0
            } else {
                let value = ((prefix >> (CODE_BITS - usize::from(length)))
                    & ((1 << symbol.category) - 1)) as i32;
                if value >= 1 << (symbol.category - 1) {
                    value
                } else {
                    value + 1 - (1 << symbol.category)
                }
            };
            Difference {
                length,
                value: value as i16,
            }
        })
    }

    // Sharing the lookup otherwise introduced a call for every decoded difference.
    #[inline(always)]
    fn looked_up_difference(
        &self,
        lookup: &[Difference; PREFIX_TABLE_ENTRIES],
        stream: &mut Bits<'_>,
    ) -> Result<i16> {
        let entry = lookup[stream.peek() as usize];
        if entry.length == 0 {
            return self.difference(stream);
        }
        // Lookahead is padded. Only advance authorizes consuming real bits.
        stream.advance(entry.length)?;
        Ok(entry.value)
    }

    /// Packed words keep the category prefix in the high twelve bits.
    pub(super) fn packed(words: &[u16]) -> Result<Self> {
        let codes: Vec<_> = words
            .iter()
            .map(|&word| (word >> 4, (word & 15) as u8))
            .collect();
        // Unused categories have zero length and no prefix.
        let mut table = [Symbol {
            length: 0,
            category: 0,
        }; PREFIX_TABLE_ENTRIES];
        if words.is_empty() || words.len() > PACKED_CATEGORY_COUNT {
            return Err("unsupported Huffman category count".into());
        }
        for (category, &(code, length)) in codes.iter().enumerate() {
            if length == 0 {
                continue;
            }
            let single = Self::new(&[(code, length)])?;
            for (destination, source) in table.iter_mut().zip(single.0) {
                if source.length != 0 {
                    if destination.length != 0 {
                        return Err(damaged("overlapping Huffman codes"));
                    }
                    *destination = Symbol {
                        length,
                        category: category as u8,
                    };
                }
            }
        }
        Ok(Self(table))
    }

    pub(super) fn decode_bytes(&self, input: &[u8], length: usize, seed: u8) -> Result<Vec<u8>> {
        let lookup = self.differences();
        let mut bits = Bits::new(input);
        let mut value = seed;
        (0..length)
            .map(|_| {
                value = value.wrapping_add(self.looked_up_difference(&lookup, &mut bits)? as u8);
                Ok(value)
            })
            .collect()
    }

    pub(super) fn decode(
        &self,
        input: &[u8],
        width: usize,
        height: usize,
        seed: u16,
        maximum: u16,
    ) -> Result<Vec<u16>> {
        self.decode_plane(
            Plane {
                input,
                width,
                height,
                seed,
            },
            maximum,
            &self.differences(),
        )
    }

    /// Decode the three sensor planes on the shared processing pool. Large planes
    /// split into fragments so one photo fills the pool.
    pub(super) fn decode_sensor_planes(
        &self,
        planes: [Plane<'_>; 3],
        maximum: u16,
        format: u32,
    ) -> Result<Vec<(Vec<u16>, Timing)>> {
        self.decode_split_planes(planes, maximum, fragments(&planes), format)
    }

    fn decode_split_planes(
        &self,
        planes: [Plane<'_>; 3],
        maximum: u16,
        fragments: [usize; 3],
        format: u32,
    ) -> Result<Vec<(Vec<u16>, Timing)>> {
        const STAGES: [&str; 3] = [
            "sensor plane bottom",
            "sensor plane middle",
            "sensor plane top",
        ];
        crate::priority::checkpoint_or_cancel()?;
        let lookup = {
            let _scope = diagnostics::scope(Category::Cpu, "decode.plane.lookup");
            self.differences()
        };
        let start = Instant::now();
        let scope = |layer: usize| {
            let plane = planes[layer];
            super::sensor_plane_scope(layer, plane.width, plane.height, plane.input.len(), format)
        };
        let splits: [_; 3] = std::array::from_fn(|layer| {
            parallel::Split::new(self, planes[layer], fragments[layer])
        });
        // One job per whole plane or fragment. A whole plane decodes its samples
        // directly; a fragment yields differences for the join below.
        let jobs: Vec<(usize, Option<usize>)> = (0..planes.len())
            .flat_map(|layer| match &splits[layer] {
                Some(split) => (0..split.len()).map(|part| (layer, Some(part))).collect(),
                None => vec![(layer, None)],
            })
            .collect();
        let mut decoded = decode_pool::plane_parts(jobs.iter().copied(), |(layer, part)| {
            match (part, &splits[layer]) {
                (Some(part), Some(split)) => {
                    split.unpack(self, &lookup, planes[layer].input, layer, part)
                }
                _ => {
                    let _scope = scope(layer);
                    self.decode_plane(planes[layer], maximum, &lookup).map(Some)
                }
            }
        })?
        .into_iter();
        let per_layer: Vec<Vec<_>> = jobs
            .chunk_by(|a, b| a.0 == b.0)
            .map(|group| decoded.by_ref().take(group.len()).collect())
            .collect();
        decode_pool::plane_parts(per_layer.into_iter().enumerate(), |(layer, parts)| {
            let samples = if splits[layer].is_none() {
                parts
                    .into_iter()
                    .next()
                    .flatten()
                    .ok_or("missing sensor plane data")?
            } else {
                let _scope = scope(layer);
                match parallel::reconstruct(parts, planes[layer], maximum)? {
                    Some(samples) => samples,
                    // Replay in stored order when a join or sample is invalid, so
                    // damage reports the first technical cause.
                    None => self.decode_plane(planes[layer], maximum, &lookup)?,
                }
            };
            Ok((samples, Timing::elapsed(STAGES[layer], start)))
        })
    }

    fn decode_plane(
        &self,
        Plane {
            input,
            width,
            height,
            seed,
        }: Plane<'_>,
        maximum: u16,
        lookup: &[Difference; PREFIX_TABLE_ENTRIES],
    ) -> Result<Vec<u16>> {
        let count = product(width, height)?;
        if width == 0
            || height == 0
            || width > u16::MAX as usize
            || height > u16::MAX as usize
            || count > MAXIMUM_PLANE_SAMPLES
            || seed > maximum
        {
            return Err("unsupported compressed plane geometry or seed".into());
        }
        let mut output = {
            let _scope = diagnostics::scope(Category::Cpu, "decode.plane.allocate");
            Vec::with_capacity(count)
        };
        let mut bits = Bits::new(input);
        let mut vertical = [[seed; 2]; 2];
        let mut horizontal = [seed; 2];
        let _scope = diagnostics::scope(Category::Cpu, "decode.plane.samples");
        for y in 0..height {
            crate::priority::checkpoint_or_cancel()?;
            for x in 0..width {
                let predictor = if x < 2 {
                    if y < 2 {
                        seed
                    } else {
                        vertical[y & 1][x & 1]
                    }
                } else {
                    horizontal[x & 1]
                };
                let value =
                    (predictor as i16).wrapping_add(self.looked_up_difference(lookup, &mut bits)?);
                if value as u16 > maximum {
                    return Err(damaged("decoded sample outside declared range"));
                }
                let value = value as u16;
                horizontal[x & 1] = value;
                if x < 2 {
                    vertical[y & 1][x & 1] = value;
                }
                output.push(value);
            }
        }
        Ok(output)
    }
}

/// Fragments per plane, so one photo's planes fill the processing pool. Decoding
/// work follows compressed size, so a larger plane gets more fragments. Photos
/// prefetched for the cache already fill the pool together and decode whole planes.
fn fragments(planes: &[Plane<'_>; 3]) -> [usize; 3] {
    if crate::priority::decoder_context().is_some() {
        return [1; 3];
    }
    let workers = decode_pool::available_workers();
    let total = planes
        .iter()
        .map(|plane| plane.input.len())
        .sum::<usize>()
        .max(1);
    planes.map(|plane| {
        let length = plane.input.len();
        // The nearest whole share of the workers, and no fragment shorter than the minimum.
        ((2 * workers * length + total) / (2 * total))
            .min(length / MINIMUM_FRAGMENT_BYTES)
            .max(1)
    })
}

struct Bits<'a> {
    data: &'a [u8],
    buffer: u64,
    buffered_bits: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            buffer: 0,
            buffered_bits: 0,
        }
    }

    // Keep the next prefix at the top of the buffer to avoid a variable lookahead shift.
    fn peek(&mut self) -> u16 {
        if self.buffered_bits < CODE_BITS as u32 {
            // Six bytes fit even when eleven bits are still buffered.
            let (word, bits) = if let Some(word) = self.data.first_chunk::<8>() {
                self.data = &self.data[REFILL_BYTES..];
                (
                    u64::from_be_bytes(*word) & (u64::MAX << (u64::BITS - REFILL_BITS)),
                    REFILL_BITS,
                )
            } else {
                let count = self.data.len().min(REFILL_BYTES);
                let mut padded = [0; 8];
                padded[..count].copy_from_slice(&self.data[..count]);
                self.data = &self.data[count..];
                (u64::from_be_bytes(padded), (count * 8) as u32)
            };
            self.buffer |= word >> self.buffered_bits;
            self.buffered_bits += bits;
        }
        (self.buffer >> (u64::BITS - CODE_BITS as u32)) as u16
    }
    fn advance(&mut self, count: u8) -> Result<()> {
        // Callers peek first and consume at most CODE_BITS. A short buffer then means EOF.
        self.buffered_bits = self
            .buffered_bits
            .checked_sub(u32::from(count))
            .ok_or_else(|| damaged("truncated Huffman stream"))?;
        self.buffer <<= count;
        Ok(())
    }
    fn read(&mut self, count: u8) -> Result<u16> {
        if count > 12 {
            let high = self.read(count - 12)?;
            return Ok((high << 12) | self.read(12)?);
        }
        let value = self.peek() >> (12 - count);
        self.advance(count)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookahead_matches_scalar_bits_across_words_and_tails() {
        let data: Vec<u8> = (0..97).map(|i| (i * 73 + 19) as u8).collect();
        for length in 0..=data.len() {
            let data = &data[..length];
            let mut bits = Bits::new(data);
            let mut position = 0;
            loop {
                let expected = (0..12).fold(0_u16, |v, i| {
                    let bit = position + i;
                    (v << 1)
                        | data
                            .get(bit / 8)
                            .map_or(0, |b| u16::from((b >> (7 - bit % 8)) & 1))
                });
                assert_eq!(bits.peek(), expected);
                let remaining = data.len() * 8 - position;
                if remaining == 0 {
                    assert!(bits.advance(1).is_err());
                    break;
                }
                assert_eq!(bits.read(1).unwrap(), expected >> 11);
                position += 1;
            }
        }
    }

    #[test]
    fn fourteen_bit_payloads_and_wrapping_byte_recurrence() {
        let mut words = vec![0; 15];
        words[0] = 1; // zero difference = 0
        words[14] = 0x8001; // fourteen-bit difference = 1 + payload
        let table = Table::packed(&words).unwrap();
        // Prefix one and payload 10_0000_0000_0000: +8192.
        assert_eq!(table.decode(&[0xc0, 0], 1, 1, 0, 16383).unwrap(), [8192]);
        assert!(table
            .decode(&[0xc0, 0], 1, 1, 0, SAMPLE_MAX as u16)
            .is_err());
        assert!(table.decode(&[0xc0], 1, 1, 0, 16383).is_err());
        let table = Table::packed(&[1, 0x8001]).unwrap();
        // +1 then +1 wraps the byte predictor, while independent image seeds do not.
        assert_eq!(table.decode_bytes(&[0xf0], 2, 255).unwrap(), [0, 1]);
        assert!(table.decode_bytes(&[], 1, 0).is_err());
    }

    #[test]
    fn rejects_overlapping_unaligned_and_unbounded_codes() {
        assert!(Table::new(&[(0, 1), (0, 2)]).is_err());
        assert!(Table::new(&[(1, 1)]).is_err());
        assert!(Table::new(&[(4096, 1)]).is_err());
        assert!(Table::new(&[(0, 13)]).is_err());
        assert!(Table::new(&[]).is_err());
    }

    #[test]
    fn signed_differences_and_predictors_preserve_row_parity() {
        // Zero = 0, category one = 10, category two = 11.
        let table = Table::new(&[(0, 1), (2048, 2), (3072, 2)]).unwrap();
        // +1, -1, +3, -2, zero, zero, zero, zero, zero.
        let bits = "1011001111110100000";
        let mut input = vec![0; bits.len().div_ceil(8)];
        for (i, b) in bits.bytes().enumerate() {
            input[i / 8] |= (b - b'0') << (7 - i % 8);
        }
        assert_eq!(
            table.decode(&input, 3, 3, 512, SAMPLE_MAX as u16).unwrap(),
            [513, 511, 516, 510, 512, 510, 513, 511, 513]
        );
    }

    #[test]
    fn lookahead_never_accepts_missing_consumed_bits() {
        let table = Table::new(&[(0, 1)]).unwrap();
        assert_eq!(
            table.decode(&[0], 4, 2, 512, SAMPLE_MAX as u16).unwrap(),
            vec![512; 8]
        );
        assert!(table.decode(&[0], 3, 3, 512, SAMPLE_MAX as u16).is_err());
        assert!(table.decode(&[], 1, 1, 512, SAMPLE_MAX as u16).is_err());
        assert!(table.decode(&[0x80], 1, 1, 512, SAMPLE_MAX as u16).is_err());
        let table = Table::new(&[(0, 1), (2048, 1)]).unwrap();
        assert!(table.decode(&[0x80], 1, 1, 0, SAMPLE_MAX as u16).is_err());
    }
}

#[cfg(test)]
#[path = "huffman_lookup_test.rs"]
mod lookup_tests;
