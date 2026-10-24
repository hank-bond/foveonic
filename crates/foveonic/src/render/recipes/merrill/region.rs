//! Maps oriented Merrill detail requests onto native sensor rectangles and processing tiles.

use crate::{nodes::sensor::calibration::EncodeParams, source::Result};

// The ordinary sharpening signal reads one neighboring pixel and suppresses
// two border pixels. Keep four pixels so the requested view never sees that edge.
const FINISH_SUPPORT: u32 = 4;

pub(super) struct Prepared {
    pub physical: [u32; 4],
    pub encode: EncodeParams,
}
impl Prepared {
    pub fn new(mut encode: EncodeParams, output: [u32; 4]) -> Result<Self> {
        let [left, top, right, bottom] = encode.crop;
        let [x0, y0, x1, y1] = output;
        if x0 >= x1 || y0 >= y1 || x1 > encode.output[0] || y1 > encode.output[1] {
            return Err("invalid Merrill preview region".into());
        }
        let native = [right - left, bottom - top];
        let oriented = if encode.source[2].is_multiple_of(180) {
            native
        } else {
            [native[1], native[0]]
        };
        if encode.output[..2] != oriented {
            return Err("Merrill regional preview requires native output dimensions".into());
        }
        let crop = match encode.source[2] {
            0 => [left + x0, top + y0, left + x1, top + y1],
            90 => [left + y0, bottom - x1, left + y1, bottom - x0],
            180 => [right - x1, bottom - y1, right - x0, bottom - y0],
            270 => [right - y1, top + x0, right - y0, top + x1],
            _ => return Err("invalid Merrill preview rotation".into()),
        };
        let physical = [
            crop[0].saturating_sub(FINISH_SUPPORT),
            crop[1].saturating_sub(FINISH_SUPPORT),
            crop[2].saturating_add(FINISH_SUPPORT).min(encode.source[0]),
            crop[3].saturating_add(FINISH_SUPPORT).min(encode.source[1]),
        ];
        encode.source[..2].copy_from_slice(&[physical[2] - physical[0], physical[3] - physical[1]]);
        encode.crop = std::array::from_fn(|i| crop[i] - physical[i % 2]);
        encode.output[..2].copy_from_slice(&[x1 - x0, y1 - y0]);
        Ok(Self { physical, encode })
    }
}
