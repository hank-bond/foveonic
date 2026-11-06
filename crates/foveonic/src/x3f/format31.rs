//! Decodes unsigned sensor planes into equal-resolution RGB or RGB/blue-phase cells.

use super::bytes::{product, Bytes};
use super::huffman::{Table, MAXIMUM_PLANE_SAMPLES};
use crate::input_error::damaged;
use crate::source::{Result, SourceStorage};

pub(super) struct Decoded {
    pub storage: SourceStorage,
    pub pixels: Vec<f32>,
}

/// Returns RGB triplets for equal planes or R, G, B00, B01, B10, B11 cells.
/// The extra blue columns in format 35 replace selected physical sensor pixels.
pub(super) fn decode(section: &[u8], width: usize, height: usize, format: u32) -> Result<Decoded> {
    if !matches!(format, 31 | 35 | 37 | 39 | 41) || width == 0 || height == 0 {
        return Err("unsupported unsigned sensor geometry or format".into());
    }
    let bytes = Bytes(section);
    let dimensions = (0..6)
        .map(|i| bytes.u16(28 + i * 2).map(usize::from))
        .collect::<Result<Vec<_>>>()?;
    let equal = dimensions[..4] == [width, height, width, height];
    let packed = width.is_multiple_of(2)
        && height.is_multiple_of(2)
        && dimensions[..4] == [width / 2, height / 2, width / 2, height / 2];
    if (!equal && !packed)
        || dimensions[5] != height
        || dimensions[4] < width
        || (format != 35 && dimensions[4] != width)
    {
        return Err("unsupported compressed sensor plane geometry".into());
    }
    let blue_width = dimensions[4];
    let divisor = if equal { 1 } else { 2 };
    let low_width = width / divisor;
    let low_height = height / divisor;
    let extra_samples = product(low_width.div_ceil(16), divisor)?;
    if blue_width != width
        && (blue_width - width < extra_samples || !height.is_multiple_of(4 * divisor))
    {
        return Err(damaged("invalid special-column sensor geometry"));
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
        if product(plane_width, plane_height)? > MAXIMUM_PLANE_SAMPLES {
            return Err("unsupported compressed plane geometry".into());
        }
        if seed > 16383 {
            return Err(damaged(
                "compressed plane seed exceeds the sensor code range",
            ));
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
                    .ok_or_else(|| damaged("compressed plane length overflow"))?
                    & !15,
            )
            .ok_or_else(|| damaged("compressed plane offset overflow"))?;
    }
    let planes = super::decode_planes(|channel| {
        let (input, width, height, seed) = inputs[channel];
        table.decode_bounded(input, width, height, seed, 16383)
    })?;
    let cells = product(low_width, low_height)?;
    let mut output = Vec::with_capacity(product(cells, if equal { 3 } else { 6 })?);
    for y in 0..low_height {
        crate::background::checkpoint_or_cancel()?;
        for x in 0..low_width {
            let index = y * low_width + x;
            output.push(f32::from(planes[0][index]));
            output.push(f32::from(planes[1][index]));
            let blue_x = if blue_width != width && y.is_multiple_of(4) && x.is_multiple_of(16) {
                width + (x / 16) * divisor
            } else {
                x * divisor
            };
            for dy in 0..divisor {
                for dx in 0..divisor {
                    output.push(f32::from(
                        planes[2][(y * divisor + dy) * blue_width + blue_x + dx],
                    ));
                }
            }
        }
    }
    Ok(Decoded {
        storage: if equal {
            SourceStorage::InterleavedRgb
        } else {
            SourceStorage::PackedRgbBluePhases
        },
        pixels: output,
    })
}

#[cfg(test)]
#[path = "format31_test.rs"]
mod tests;
