//! Splits large sensor streams on verified Huffman boundaries and reconstructs sensor samples.

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
        let input = plane.input;
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
        // Compressed bytes do not divide evenly by sample count. Leave room
        // for changes in compression while keeping scratch bounded.
        let share = (plane.width * plane.height).div_ceil(fragments);
        let capacity = share + share / 2;
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
        unpack(table, lookup, input, range, self.longest, self.capacity)
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

fn unpack(
    table: &Table,
    lookup: &[Difference; PREFIX_TABLE_ENTRIES],
    input: &[u8],
    range: Range<usize>,
    longest: usize,
    capacity: usize,
) -> Result<Option<Vec<u16>>> {
    let end_byte = range.end.div_ceil(8);
    let padding = end_byte * 8 - range.end;
    let last = end_byte == input.len() && padding == 0;
    let mut bits = at_bit(&input[..end_byte], range.start);
    let mut differences = Vec::with_capacity(capacity);
    loop {
        priority::checkpoint_or_cancel()?;
        let remaining = bits.data.len() * 8 + bits.buffered_bits as usize;
        if remaining <= padding || differences.len() == capacity {
            return Ok((remaining == padding || last).then_some(differences));
        }
        // Each full batch fits without a per-symbol end-position check. Close
        // to the boundary, check after one symbol at a time.
        let batch = ((remaining - padding) / longest)
            .clamp(1, CHECKPOINT_SAMPLES)
            .min(capacity - differences.len());
        for _ in 0..batch {
            match table.looked_up_difference(lookup, &mut bits) {
                Ok(difference) => differences.push(difference as u16),
                Err(_) => return Ok(last.then_some(differences)),
            }
        }
    }
}
