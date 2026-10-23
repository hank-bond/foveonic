//! Decode category-coded differences with two-column and two-row sensor predictors.
//!
//! Imported from Foveonic's X3F reader.

use super::bytes::product;
use crate::source::Result;

pub(super) const SAMPLE_MAX: u32 = 4095;
pub(super) const MAXIMUM_PLANE_SAMPLES: usize = 32_000_000;
const CODE_BITS: usize = 12;
const PREFIX_TABLE_ENTRIES: usize = 1 << CODE_BITS;
const PACKED_CATEGORY_COUNT: usize = 18;
const TWELVE_BIT_CATEGORY_COUNT: usize = 13;
const SAMPLE_MASK: u32 = 0b1111_1111_1111;

#[derive(Clone, Copy)]
struct Symbol {
    length: u8,
    category: u8,
}

pub(super) struct Table([Symbol; PREFIX_TABLE_ENTRIES]);

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
                return Err("invalid Huffman code length".into());
            }
            let span = 1usize << (12 - length);
            if code as usize >= table.len() || !(code as usize).is_multiple_of(span) {
                return Err("invalid Huffman code alignment".into());
            }
            for entry in &mut table[code as usize..code as usize + span] {
                if entry.length != 0 {
                    return Err("overlapping Huffman codes".into());
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
            return Err("invalid Huffman prefix".into());
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
                        return Err("overlapping Huffman codes".into());
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
        let mut bits = Bits::new(input)?;
        let mut value = seed;
        (0..length)
            .map(|_| {
                value = value.wrapping_add(self.difference(&mut bits)? as u8);
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
    ) -> Result<Vec<u16>> {
        self.decode_bounded(input, width, height, seed, SAMPLE_MAX as u16)
    }

    pub(super) fn decode_bounded(
        &self,
        input: &[u8],
        width: usize,
        height: usize,
        seed: u16,
        maximum: u16,
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
        let mut output = Vec::with_capacity(count);
        let mut bits = Bits::new(input)?;
        let mut vertical = [[seed; 2]; 2];
        let mut horizontal = [seed; 2];
        for y in 0..height {
            crate::background::checkpoint();
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
                let value = (predictor as i16).wrapping_add(self.difference(&mut bits)?);
                if value as u16 > maximum {
                    return Err("decoded sample outside declared range".into());
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

struct Bits<'a> {
    data: &'a [u8],
    position: usize,
    total_bits: usize,
    packet: u64,
    packet_start: usize,
    loaded: bool,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Result<Self> {
        Ok(Self {
            data,
            position: 0,
            total_bits: product(data.len(), 8)?,
            packet: 0,
            packet_start: 0,
            loaded: false,
        })
    }

    /// Cache a word so prefixes and payloads do not each need fresh byte loads.
    /// Padding permits lookahead at the tail, not consumption past the real input.
    /// Advance must still check the number of bits actually available.
    fn peek(&mut self) -> u16 {
        if !self.loaded || self.position - self.packet_start > 64 - CODE_BITS {
            let byte = self.position / 8;
            self.packet_start = byte * 8;
            self.packet = if let Some(word) = self.data.get(byte..byte + 8) {
                u64::from_be_bytes(word.try_into().expect("eight-byte word"))
            } else {
                let mut padded = [0_u8; 8];
                let tail = &self.data[byte..];
                padded[..tail.len()].copy_from_slice(tail);
                u64::from_be_bytes(padded)
            };
            self.loaded = true;
        }
        ((self.packet >> (64 - CODE_BITS - (self.position - self.packet_start)))
            & u64::from(SAMPLE_MASK)) as u16
    }
    fn advance(&mut self, count: u8) -> Result<()> {
        if count as usize > self.total_bits - self.position {
            return Err("truncated Huffman stream".into());
        }
        self.position += count as usize;
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
            let mut bits = Bits::new(data).unwrap();
            while bits.position <= data.len() * 8 {
                let expected = (0..12).fold(0_u16, |v, i| {
                    let bit = bits.position + i;
                    (v << 1)
                        | data
                            .get(bit / 8)
                            .map_or(0, |b| u16::from((b >> (7 - bit % 8)) & 1))
                });
                assert_eq!(bits.peek(), expected);
                let remaining = data.len() * 8 - bits.position;
                if remaining == 0 {
                    assert!(bits.advance(1).is_err());
                    break;
                }
                let count = (bits.position % 12 + 1).min(remaining) as u8;
                assert_eq!(bits.read(count).unwrap(), expected >> (12 - count));
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
        assert_eq!(
            table.decode_bounded(&[0xc0, 0], 1, 1, 0, 16383).unwrap(),
            [8192]
        );
        assert!(table.decode(&[0xc0, 0], 1, 1, 0).is_err());
        assert!(table.decode_bounded(&[0xc0], 1, 1, 0, 16383).is_err());
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
            table.decode(&input, 3, 3, 512).unwrap(),
            [513, 511, 516, 510, 512, 510, 513, 511, 513]
        );
    }

    #[test]
    fn lookahead_never_accepts_missing_consumed_bits() {
        let table = Table::new(&[(0, 1)]).unwrap();
        assert_eq!(table.decode(&[0], 4, 2, 512).unwrap(), vec![512; 8]);
        assert!(table.decode(&[0], 3, 3, 512).is_err());
        assert!(table.decode(&[], 1, 1, 512).is_err());
        assert!(table.decode(&[0x80], 1, 1, 512).is_err());
        let table = Table::new(&[(0, 1), (2048, 1)]).unwrap();
        assert!(table.decode(&[0x80], 1, 1, 0).is_err());
    }
}
