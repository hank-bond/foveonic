//! Decodes unsigned sensor planes into equal-resolution RGB or RGB/blue-phase cells.

use super::bytes::{product, Bytes};
use super::huffman::{Plane, Table, MAXIMUM_PLANE_SAMPLES};
use crate::diagnostics::{self, Category};
use crate::input_error::damaged;
use crate::source::{Result, SourceStorage};

const SPECIAL_COLUMN_STEP: usize = 16;
const SPECIAL_ROW_STEP: usize = 4;
const MINIMUM_TOP_PLANE_SAMPLES: usize = 8_000_000;
const MAXIMUM_TOP_PLANE_FRAGMENTS: usize = 8;

pub(super) struct Decoded {
    pub storage: SourceStorage,
    pub pixels: Vec<f32>,
}

/// Returns RGB triplets for equal planes or R, G, B00, B01, B10, B11 cells.
/// The extra blue columns in format 35 replace selected physical sensor pixels.
pub(super) fn decode(section: &[u8], width: usize, height: usize, format: u32) -> Result<Decoded> {
    let prepare_scope = diagnostics::scope(Category::Cpu, "decode.sensor.prepare");
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
    let extra_samples = product(low_width.div_ceil(SPECIAL_COLUMN_STEP), divisor)?;
    if blue_width != width
        && (blue_width - width < extra_samples
            || !height.is_multiple_of(SPECIAL_ROW_STEP * divisor))
    {
        return Err(damaged("invalid special-column sensor geometry"));
    }
    let words = (0..18)
        .map(|i| bytes.u16(48 + i * 2))
        .collect::<Result<Vec<_>>>()?;
    let table = Table::packed(&words)?;
    // Check every compressed slice and allocation bound before starting workers.
    let mut offset = 96usize;
    let mut inputs = [Plane::default(); 3];
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
        *input = Plane {
            input: bytes.range(offset, length)?,
            width: plane_width,
            height: plane_height,
            seed,
        };
        offset = offset
            .checked_add(
                length
                    .checked_add(15)
                    .ok_or_else(|| damaged("compressed plane length overflow"))?
                    & !15,
            )
            .ok_or_else(|| damaged("compressed plane offset overflow"))?;
    }
    drop(prepare_scope);
    let top_fragments = if packed && blue_width * height >= MINIMUM_TOP_PLANE_SAMPLES {
        crate::decode_pool::available_workers()
            .saturating_sub(2)
            .clamp(1, MAXIMUM_TOP_PLANE_FRAGMENTS)
    } else {
        1
    };
    let planes = {
        let _scope = diagnostics::scope(Category::Cpu, "decode.sensor.planes");
        table.decode_sensor_planes(inputs, 16383, [1, 1, top_fragments], format)?
    };
    let output = crate::decode_pool::cpu(|| {
        let _assembly_scope = diagnostics::scope(Category::Cpu, "decode.sensor.assemble");
        let row_samples = product(low_width, if equal { 3 } else { 6 })?;
        let mut output = {
            let _scope = diagnostics::scope(Category::Cpu, "decode.sensor.output.allocate");
            vec![0.; product(row_samples, low_height)?]
        };
        let red_rows = planes[0].0.chunks_exact(low_width);
        let green_rows = planes[1].0.chunks_exact(low_width);
        let blue_rows = planes[2].0.chunks_exact(blue_width * divisor);
        for (y, (((output, red), green), blue)) in output
            .chunks_exact_mut(row_samples)
            .zip(red_rows)
            .zip(green_rows)
            .zip(blue_rows)
            .enumerate()
        {
            crate::priority::checkpoint_or_cancel()?;
            // Applying sparse replacements separately keeps the regular cell loop branch-free.
            let special = blue_width != width && y.is_multiple_of(SPECIAL_ROW_STEP);
            if equal {
                assemble_equal_row(output, red, green, blue, special);
            } else {
                let (top, bottom) = blue.split_at(blue_width);
                assemble_packed_row(output, red, green, top, bottom, special);
            }
        }
        Ok(output)
    })?;
    Ok(Decoded {
        storage: if equal {
            SourceStorage::InterleavedRgb
        } else {
            SourceStorage::PackedRgbBluePhases
        },
        pixels: output,
    })
}

fn assemble_equal_row(output: &mut [f32], red: &[u16], green: &[u16], blue: &[u16], special: bool) {
    let cells = output.as_chunks_mut::<3>().0;
    for (cell, ((&red, &green), &blue)) in cells.iter_mut().zip(red.iter().zip(green).zip(blue)) {
        *cell = [red, green, blue].map(f32::from);
    }
    if special {
        for (cell, &blue) in cells
            .iter_mut()
            .step_by(SPECIAL_COLUMN_STEP)
            .zip(&blue[red.len()..])
        {
            cell[2] = f32::from(blue);
        }
    }
}

fn assemble_packed_row(
    output: &mut [f32],
    red: &[u16],
    green: &[u16],
    top: &[u16],
    bottom: &[u16],
    special: bool,
) {
    let cells = output.as_chunks_mut::<6>().0;
    let top = top.as_chunks::<2>().0;
    let bottom = bottom.as_chunks::<2>().0;
    for (cell, ((&red, &green), (&[b00, b01], &[b10, b11]))) in cells
        .iter_mut()
        .zip(red.iter().zip(green).zip(top.iter().zip(bottom)))
    {
        *cell = [red, green, b00, b01, b10, b11].map(f32::from);
    }
    if special {
        let extra = top[red.len()..].iter().zip(&bottom[red.len()..]);
        for (cell, (&[b00, b01], &[b10, b11])) in
            cells.iter_mut().step_by(SPECIAL_COLUMN_STEP).zip(extra)
        {
            cell[2..].copy_from_slice(&[b00, b01, b10, b11].map(f32::from));
        }
    }
}

#[cfg(test)]
#[path = "format31_test.rs"]
mod tests;
