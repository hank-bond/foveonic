//! Splits sensor and calibration streams on verified Huffman boundaries.

use super::{Bits, Difference, Plane, Table, PREFIX_TABLE_ENTRIES};
use crate::{diagnostics, priority, source::Result};
use std::ops::Range;

const SCOUT_BITS: usize = 4096;
const CHECKPOINT_SAMPLES: usize = 4096;

pub(super) struct Split {
    boundaries: Vec<usize>,
    longest: usize,
    capacity: usize,
}

impl Split {
    pub(super) fn new(table: &Table, plane: Plane<'_>, fragments: usize) -> Option<Self> {
        if fragments <= 1 {
            return None;
        }
        // Compressed bytes do not divide evenly by sample count.
        let share = (plane.width * plane.height).div_ceil(fragments);
        Self::plan(table, plane.input, fragments, share + share / 2)
    }

    fn plan(table: &Table, input: &[u8], fragments: usize, capacity: usize) -> Option<Self> {
        let longest = table.0.iter().map(|s| s.length + s.category).max()? as usize;
        if longest == 0 {
            return None;
        }
        let end = input.len().checked_mul(8)?;
        let mut boundaries = vec![0];
        for i in 1..fragments {
            let cut = input.len() / fragments * i * 8;
            let next = scout(table, input, cut, longest)?;
            if next <= boundaries[i - 1] || next >= end {
                return None;
            }
            boundaries.push(next);
        }
        boundaries.push(end);
        Some(Self {
            boundaries,
            longest,
            capacity,
        })
    }

    pub(super) fn len(&self) -> usize {
        self.boundaries.len() - 1
    }

    pub(super) fn unpack(
        &self,
        table: &Table,
        lookup: &[Difference; PREFIX_TABLE_ENTRIES],
        input: &[u8],
        layer: usize,
        part: usize,
    ) -> Result<Option<Vec<u16>>> {
        let range = self.boundaries[part]..self.boundaries[part + 1];
        let _scope = diagnostics::scope_with(
            diagnostics::Category::Cpu,
            "decode.plane.fragment",
            |fields| {
                fields.u64("plane", layer as u64);
                fields.u64("fragment", part as u64);
                fields.u64("bits", range.len() as u64);
            },
        );
        unpack(
            table,
            lookup,
            input,
            range,
            self.longest,
            self.capacity,
            |difference| difference as u16,
        )
    }
}

pub(super) fn reconstruct(
    parts: Vec<Option<Vec<u16>>>,
    Plane {
        width,
        height,
        seed,
        ..
    }: Plane<'_>,
    maximum: u16,
) -> Result<Option<Vec<u16>>> {
    let _scope = diagnostics::scope(diagnostics::Category::Cpu, "decode.plane.reconstruct");
    let count = width * height;
    let fragments = parts.len();
    let mut output = Vec::with_capacity(count);
    for (i, part) in parts.into_iter().enumerate() {
        priority::checkpoint_or_cancel()?;
        let Some(mut differences) = part else {
            return Ok(None);
        };
        let remaining = count - output.len();
        if i == fragments - 1 {
            // The declared sample count ends the stream. Padding after it can
            // contain arbitrary bits, including an incomplete or invalid code.
            if differences.len() < remaining {
                return Ok(None);
            }
            differences.truncate(remaining);
        } else if differences.len() > remaining {
            return Ok(None);
        }
        output.extend_from_slice(&differences);
    }

    let mut vertical = [[seed; 2]; 2];
    for (y, row) in output.chunks_exact_mut(width).enumerate() {
        priority::checkpoint_or_cancel()?;
        let (pairs, tail) = row.as_chunks_mut::<2>();
        let row_seed = &mut vertical[y & 1];
        let mut horizontal = *row_seed;
        for pair in pairs {
            horizontal[0] = horizontal[0].wrapping_add(pair[0]);
            horizontal[1] = horizontal[1].wrapping_add(pair[1]);
            if horizontal[0] > maximum || horizontal[1] > maximum {
                return Ok(None);
            }
            *pair = horizontal;
        }
        if let Some(value) = tail.first_mut() {
            *value = horizontal[0].wrapping_add(*value);
            if *value > maximum {
                return Ok(None);
            }
        }
        for (base, &sample) in row_seed.iter_mut().zip(row.iter()) {
            *base = sample;
        }
    }
    Ok(Some(output))
}

pub(super) fn decode_bytes(
    table: &Table,
    input: &[u8],
    length: usize,
    seed: u8,
    fragments: usize,
) -> Result<Option<Vec<u8>>> {
    priority::checkpoint_or_cancel()?;
    // Calibration records can vary widely in compressed symbol density.
    let capacity = 2 * length.div_ceil(fragments);
    let Some(split) = Split::plan(table, input, fragments, capacity) else {
        return Ok(None);
    };
    let lookup = table.differences();
    let parts = crate::decode_pool::plane_parts(0..split.len(), |part| {
        let _scope = diagnostics::scope_with(
            diagnostics::Category::Cpu,
            "decode.camf.fragment",
            |fields| {
                fields.u64("fragment", part as u64);
            },
        );
        let range = split.boundaries[part]..split.boundaries[part + 1];
        let mut byte = 0_u8;
        unpack(
            table,
            &lookup,
            input,
            range,
            split.longest,
            split.capacity,
            |difference| {
                byte = byte.wrapping_add(difference as u8);
                byte
            },
        )
    })?;
    let mut carry = seed;
    let mut output = Vec::with_capacity(length);
    for (index, part) in parts.into_iter().enumerate() {
        priority::checkpoint_or_cancel()?;
        let Some(mut part) = part else {
            return Ok(None);
        };
        let remaining = length - output.len();
        if index + 1 == split.len() {
            // The declared size selects the last byte even if the input continues.
            if part.len() < remaining {
                return Ok(None);
            }
            part.truncate(remaining);
        } else if part.len() > remaining {
            return Ok(None);
        }
        // A chunk starts at zero. Its final byte is its total delta.
        let sum = part.last().copied().unwrap_or(0);
        output.extend(part.into_iter().map(|byte| byte.wrapping_add(carry)));
        carry = carry.wrapping_add(sum);
    }
    Ok(Some(output))
}

// Each set bit is a live candidate token boundary. Start with every possible
// boundary within one token of the cut, then walk and merge the candidates.
// See Dougall Johnson, "Parallelising Huffman decoding and x86 disassembly".
fn scout(table: &Table, input: &[u8], cut: usize, longest: usize) -> Option<usize> {
    let end = input.len() * 8;
    let mut offset = cut;
    let mut pending = (1_u32 << longest) - 1;
    while pending != 1 {
        if offset - cut > SCOUT_BITS || offset >= end {
            return None;
        }
        let mut bits = at_bit(input, offset);
        let symbol = table.0[bits.peek() as usize];
        pending &= !1;
        let length = usize::from(symbol.length + symbol.category);
        if symbol.length != 0 && length <= end - offset {
            pending |= 1 << length;
        }
        if pending == 0 {
            return None;
        }
        let delta = pending.trailing_zeros();
        offset += delta as usize;
        pending >>= delta;
    }
    (offset - cut <= SCOUT_BITS).then_some(offset)
}

fn at_bit(input: &[u8], start: usize) -> Bits<'_> {
    let mut bits = Bits::new(&input[start / 8..]);
    let skip = (start % 8) as u8;
    if skip != 0 {
        bits.peek();
        bits.advance(skip).expect("starting bit inside input");
    }
    bits
}

fn unpack<T>(
    table: &Table,
    lookup: &[Difference; PREFIX_TABLE_ENTRIES],
    input: &[u8],
    range: Range<usize>,
    longest: usize,
    capacity: usize,
    mut sample: impl FnMut(i16) -> T,
) -> Result<Option<Vec<T>>> {
    let end_byte = range.end.div_ceil(8);
    let padding = end_byte * 8 - range.end;
    let last = end_byte == input.len() && padding == 0;
    let mut bits = at_bit(&input[..end_byte], range.start);
    let mut decoded = Vec::with_capacity(capacity);
    loop {
        priority::checkpoint_or_cancel()?;
        let remaining = bits.data.len() * 8 + bits.buffered_bits as usize;
        if remaining <= padding || decoded.len() == capacity {
            return Ok((remaining == padding || last).then_some(decoded));
        }
        // Each full batch fits without a per-symbol end-position check. Close
        // to the boundary, check after one symbol at a time.
        let batch = ((remaining - padding) / longest)
            .clamp(1, CHECKPOINT_SAMPLES)
            .min(capacity - decoded.len());
        for _ in 0..batch {
            match table.looked_up_difference(lookup, &mut bits) {
                Ok(difference) => decoded.push(sample(difference)),
                Err(_) => return Ok(last.then_some(decoded)),
            }
        }
    }
}
