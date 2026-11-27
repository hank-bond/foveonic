//! Splits sensor and calibration streams on verified Huffman boundaries.

use super::{Bits, Difference, Plane, Table, PREFIX_TABLE_ENTRIES};
use crate::{diagnostics, priority, source::Result};
use std::ops::Range;

const SCOUT_BITS: usize = 4096;
const CHECKPOINT_SAMPLES: usize = 4096;
/// Verified sub-ranges each fragment decodes in lockstep. Their symbol lookups
/// are independent, so one core overlaps them instead of waiting on each.
const LANES: usize = 4;

pub(super) struct Split {
    boundaries: Vec<usize>,
    longest: usize,
    capacity: usize,
}

impl Split {
    pub(super) fn new(table: &Table, plane: Plane<'_>, fragments: usize) -> Option<Self> {
        // Compressed bytes do not divide evenly by sample count, so a fragment
        // may hold somewhat more than its share before it counts as damaged.
        let share = (plane.width * plane.height).div_ceil(fragments);
        Self::plan(table, plane.input, fragments, share + share / 2)
    }

    fn plan(table: &Table, input: &[u8], fragments: usize, capacity: usize) -> Option<Self> {
        let longest = table.0.iter().map(|s| s.length + s.category).max()? as usize;
        if longest == 0 {
            return None;
        }
        let end = input.len().checked_mul(8)?;
        let cuts = fragments * LANES;
        let mut boundaries = vec![0];
        for i in 1..cuts {
            // `end * i / cuts` without overflow.
            let cut = end / cuts * i + end % cuts * i / cuts;
            // In a short stream a search can end at or before the previous
            // boundary. Reusing that verified boundary leaves an empty lane.
            let next = scout(table, input, cut, longest)?.max(boundaries[i - 1]);
            if next >= end {
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
        (self.boundaries.len() - 1) / LANES
    }

    /// The lane boundaries of one fragment.
    fn fragment(&self, part: usize) -> &[usize; LANES + 1] {
        self.boundaries[part * LANES..=(part + 1) * LANES]
            .try_into()
            .expect("whole lanes per fragment")
    }

    pub(super) fn unpack(
        &self,
        table: &Table,
        lookup: &[Difference; PREFIX_TABLE_ENTRIES],
        input: &[u8],
        layer: usize,
        part: usize,
    ) -> Result<Option<Vec<Vec<u16>>>> {
        let bounds = self.fragment(part);
        let _scope = diagnostics::scope_with(
            diagnostics::Category::Cpu,
            "decode.plane.fragment",
            |fields| {
                fields.u64("plane", layer as u64);
                fields.u64("fragment", part as u64);
                fields.u64("bits", (bounds[LANES] - bounds[0]) as u64);
            },
        );
        unpack(table, lookup, input, bounds, self.longest, self.capacity)
    }
}

/// Join a split stream's fragments with `append`, cut to the declared count.
/// Only the last fragment may run past it, into the stream's padding.
fn join(
    parts: Vec<Option<Vec<Vec<u16>>>>,
    count: usize,
    mut append: impl FnMut(&mut Vec<u16>, Vec<Vec<u16>>),
) -> Result<Option<Vec<u16>>> {
    let fragments = parts.len();
    let mut output = Vec::with_capacity(count);
    for (i, part) in parts.into_iter().enumerate() {
        priority::checkpoint_or_cancel()?;
        let Some(lanes) = part else {
            return Ok(None);
        };
        let length: usize = lanes.iter().map(Vec::len).sum();
        let remaining = count - output.len();
        if if i == fragments - 1 {
            length < remaining
        } else {
            length > remaining
        } {
            return Ok(None);
        }
        append(&mut output, lanes);
    }
    output.truncate(count);
    Ok(Some(output))
}

pub(super) fn reconstruct(
    parts: Vec<Option<Vec<Vec<u16>>>>,
    Plane {
        width,
        height,
        seed,
        ..
    }: Plane<'_>,
    maximum: u16,
) -> Result<Option<Vec<u16>>> {
    let _scope = diagnostics::scope(diagnostics::Category::Cpu, "decode.plane.reconstruct");
    let append = |output: &mut Vec<u16>, lanes: Vec<Vec<u16>>| {
        for lane in lanes {
            output.extend_from_slice(&lane);
        }
    };
    let Some(mut output) = join(parts, width * height, append)? else {
        return Ok(None);
    };

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
        let lanes = unpack(
            table,
            &lookup,
            input,
            split.fragment(part),
            split.longest,
            split.capacity,
        )?;
        // Each fragment sums its own bytes from zero on the pool. The join then
        // adds the total of the fragments before it (a prefix sum).
        Ok(lanes.map(|mut lanes| {
            let mut byte = 0_u8;
            for difference in lanes.iter_mut().flatten() {
                byte = byte.wrapping_add(*difference as u8);
                *difference = u16::from(byte);
            }
            lanes
        }))
    })?;
    let mut carry = seed;
    let append = |output: &mut Vec<u16>, lanes: Vec<Vec<u16>>| {
        let total = lanes
            .iter()
            .rev()
            .find_map(|lane| lane.last())
            .map_or(0, |&byte| byte as u8);
        for lane in lanes {
            output.extend(
                lane.into_iter()
                    .map(|byte| u16::from((byte as u8).wrapping_add(carry))),
            );
        }
        carry = carry.wrapping_add(total);
    };
    Ok(
        join(parts, length, append)?
            .map(|bytes| bytes.into_iter().map(|byte| byte as u8).collect()),
    )
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

/// One lane of a fragment: a verified range decoded into its own differences,
/// stored as wrapping `u16` so the sample predictors add them directly.
struct Lane<'a> {
    bits: Bits<'a>,
    padding: usize,
    failed: bool,
    differences: Vec<u16>,
}

impl<'a> Lane<'a> {
    fn new(input: &'a [u8], range: Range<usize>, capacity: usize) -> Self {
        let end_byte = range.end.div_ceil(8);
        let padding = end_byte * 8 - range.end;
        Self {
            bits: at_bit(&input[..end_byte], range.start),
            padding,
            failed: false,
            differences: Vec::with_capacity(capacity.div_ceil(LANES)),
        }
    }

    fn remaining(&self) -> usize {
        self.bits.data.len() * 8 + self.bits.buffered_bits as usize
    }

    /// Symbols that fit before the boundary without a per-symbol end check.
    fn room(&self, longest: usize) -> usize {
        self.remaining().saturating_sub(self.padding) / longest
    }

    /// Decode to the lane's boundary, using at most `spare` more differences.
    /// Says whether it got there, rather than stopping at an invalid code or
    /// running out of spare capacity.
    fn finish(
        &mut self,
        table: &Table,
        lookup: &[Difference; PREFIX_TABLE_ENTRIES],
        longest: usize,
        spare: usize,
    ) -> Result<bool> {
        let limit = self.differences.len() + spare;
        loop {
            if self.failed {
                return Ok(false);
            }
            priority::checkpoint_or_cancel()?;
            let remaining = self.remaining();
            if remaining <= self.padding || self.differences.len() == limit {
                return Ok(remaining == self.padding);
            }
            // Close to the boundary, check after one symbol at a time.
            let batch = self
                .room(longest)
                .clamp(1, CHECKPOINT_SAMPLES)
                .min(limit - self.differences.len());
            for _ in 0..batch {
                if !self.step(table, lookup) {
                    break;
                }
            }
        }
    }

    #[inline(always)]
    fn step(&mut self, table: &Table, lookup: &[Difference; PREFIX_TABLE_ENTRIES]) -> bool {
        match table.looked_up_difference(lookup, &mut self.bits) {
            Ok(difference) => {
                self.differences.push(difference as u16);
                true
            }
            Err(_) => {
                self.failed = true;
                false
            }
        }
    }
}

/// Decode one fragment's lanes in lockstep while each has room, then finish
/// each lane alone. The lanes share the fragment's `capacity`, which bounds
/// the work a damaged stream can cause.
fn unpack(
    table: &Table,
    lookup: &[Difference; PREFIX_TABLE_ENTRIES],
    input: &[u8],
    bounds: &[usize; LANES + 1],
    longest: usize,
    capacity: usize,
) -> Result<Option<Vec<Vec<u16>>>> {
    let mut lanes: [Lane; LANES] =
        std::array::from_fn(|lane| Lane::new(input, bounds[lane]..bounds[lane + 1], capacity));
    let used = |lanes: &[Lane]| {
        lanes
            .iter()
            .map(|lane| lane.differences.len())
            .sum::<usize>()
    };
    'lockstep: loop {
        priority::checkpoint_or_cancel()?;
        let room = lanes.iter().map(|lane| lane.room(longest)).min();
        let batch = room
            .expect("a fragment has lanes")
            .min((capacity - used(&lanes)) / LANES)
            .min(CHECKPOINT_SAMPLES);
        if batch == 0 {
            break;
        }
        for _ in 0..batch {
            for lane in &mut lanes {
                if !lane.step(table, lookup) {
                    break 'lockstep;
                }
            }
        }
    }
    // The declared sample count ends the stream. Padding after it can hold
    // an invalid code, so in the stream's last fragment the first lane that
    // stops early ends it and the join keeps only the declared samples.
    let stream_end = bounds[LANES] == input.len() * 8;
    for index in 0..LANES {
        let spare = capacity - used(&lanes);
        if !lanes[index].finish(table, lookup, longest, spare)? {
            if !stream_end {
                return Ok(None);
            }
            return Ok(Some(
                lanes
                    .into_iter()
                    .take(index + 1)
                    .map(|lane| lane.differences)
                    .collect(),
            ));
        }
    }
    Ok(Some(
        lanes.into_iter().map(|lane| lane.differences).collect(),
    ))
}

#[cfg(test)]
#[path = "huffman_parallel_test.rs"]
mod tests;
