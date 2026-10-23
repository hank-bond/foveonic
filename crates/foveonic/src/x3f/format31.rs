//! Decodes unsigned mixed-resolution sensor planes into RGB/blue-phase cells.

use super::bytes::{product, Bytes};
use super::huffman::{Table, MAXIMUM_PLANE_SAMPLES};
use crate::source::Result;

/// Returns R, G, B00, B01, B10, B11 for each half-resolution cell.
/// The extra blue columns in format 35 replace selected physical sensor pixels.
pub(super) fn decode(section: &[u8], width: usize, height: usize, format: u32) -> Result<Vec<f32>> {
    if !matches!(format, 35 | 37 | 39)
        || width == 0
        || height == 0
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
        || (format == 35 && width <= 4000)
    {
        return Err("unsupported mixed-resolution sensor geometry or format".into());
    }
    let bytes = Bytes(section);
    let dimensions = (0..6)
        .map(|i| bytes.u16(28 + i * 2).map(usize::from))
        .collect::<Result<Vec<_>>>()?;
    let low_width = width / 2;
    let low_height = height / 2;
    if dimensions[..4] != [low_width, low_height, low_width, low_height]
        || dimensions[5] != height
        || dimensions[4] < width
        || (format != 35 && dimensions[4] != width)
    {
        return Err("unsupported compressed sensor plane geometry".into());
    }
    let blue_width = dimensions[4];
    let extra_pairs = low_width.div_ceil(16);
    if blue_width != width
        && (blue_width - width < product(extra_pairs, 2)? || !height.is_multiple_of(8))
    {
        return Err("invalid special-column sensor geometry".into());
    }
    let words = (0..18)
        .map(|i| bytes.u16(48 + i * 2))
        .collect::<Result<Vec<_>>>()?;
    let table = Table::packed(&words)?;
    // Check every compressed slice and allocation bound before starting workers.
    let mut offset = 96usize;
    let mut inputs = [(&[][..], 0usize, 0usize, 0u16); 3];
    for (channel, input) in inputs.iter_mut().enumerate() {
        let length = bytes.offset(84 + channel * 4)?;
        let plane_width = dimensions[channel * 2];
        let plane_height = dimensions[channel * 2 + 1];
        let seed = bytes.u16(40 + channel * 2)?;
        if product(plane_width, plane_height)? > MAXIMUM_PLANE_SAMPLES || seed > 16383 {
            return Err("unsupported compressed plane geometry or seed".into());
        }
        *input = (
            bytes.range(offset, length)?,
            plane_width,
            plane_height,
            seed,
        );
        offset = offset
            .checked_add(
                length
                    .checked_add(15)
                    .ok_or("compressed plane length overflow")?
                    & !15,
            )
            .ok_or("compressed plane offset overflow")?;
    }
    let planes = std::thread::scope(|scope| -> Result<Vec<_>> {
        let workers = inputs.map(|(input, width, height, seed)| {
            let table = &table;
            let scheduling = crate::background::context();
            scope.spawn(move || {
                crate::background::run(scheduling, || {
                    table.decode_bounded(input, width, height, seed, 16383)
                })
            })
        });
        // Join all three before returning an error so scoped panics remain errors.
        workers
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| "sensor decoder worker panicked".into())
                    .and_then(|result| result)
            })
            .into_iter()
            .collect()
    })?;
    let cells = product(low_width, low_height)?;
    let mut output = Vec::with_capacity(product(cells, 6)?);
    for y in 0..low_height {
        crate::background::checkpoint();
        for x in 0..low_width {
            let index = y * low_width + x;
            output.push(f32::from(planes[0][index]));
            output.push(f32::from(planes[1][index]));
            let blue_x = if blue_width != width && (y * 2).is_multiple_of(8) && x.is_multiple_of(16)
            {
                width + (x / 16) * 2
            } else {
                x * 2
            };
            for dy in 0..2 {
                for dx in 0..2 {
                    output.push(f32::from(
                        planes[2][(y * 2 + dy) * blue_width + blue_x + dx],
                    ));
                }
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
#[path = "format31_test.rs"]
mod tests;
