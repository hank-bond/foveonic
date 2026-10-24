//! Prepares Merrill's half-size calibrated preview grid without changing source metadata.

use crate::{
    nodes::{color::pyramid_resample, sensor::calibration::EncodeParams},
    source::Result,
};

const SCALE: u32 = 2;

pub(in crate::render) fn available(metadata: &crate::source::Metadata) -> bool {
    metadata.processing_family().ok() == Some(crate::source::ProcessingFamily::Merrill)
        && crate::nodes::sensor::calibration::encode(metadata, crate::color_space::ColorSpace::Srgb)
            .and_then(|encode| Grid::new(encode, None))
            .is_ok()
}

pub(super) struct Grid {
    pub reduction: pyramid_resample::Prepared,
    pub encode: EncodeParams,
}
impl Grid {
    /// Apply camera noise controls to the reduced samples. Keep filter support
    /// in working pixels and map sensor-coordinate bounds without editing metadata.
    pub fn noise(&self, mut params: crate::noise::Parameters) -> Result<crate::noise::Parameters> {
        let extent = self.reduction.output_extent();
        for (axis, length) in extent.into_iter().enumerate() {
            let tiles = params.source[axis + 2];
            if tiles == 0 || !length.is_multiple_of(tiles) {
                return Err("reduced noise grid requires complete processing tiles".into());
            }
            params.source[axis] = length;
            params.tile[axis] = length / tiles + 2 * params.tile[2];
            params.area[axis] /= SCALE;
            params.area[axis + 2] = params.area[axis + 2].div_ceil(SCALE);
            params.column_area[axis] /= SCALE;
            params.column_area[axis + 2] = params.column_area[axis + 2].div_ceil(SCALE);
        }
        params.region = [0, 0, extent[0], extent[1]];
        params.reduced_tile_extent()?;
        Ok(params)
    }

    pub fn new(mut encode: EncodeParams, output_size: Option<[u32; 2]>) -> Result<Self> {
        let extent = [encode.source[0], encode.source[1]];
        // Keep framing exactly representable on the reduced grid.
        if extent.iter().any(|v| !v.is_multiple_of(SCALE))
            || encode.crop.iter().any(|v| !v.is_multiple_of(SCALE))
        {
            return Err("Merrill reduced preview requires even sensor dimensions and crop".into());
        }
        let reduction = pyramid_resample::Prepared::new(extent, pyramid_resample::Ratio::Half)?;
        encode.source[..2].copy_from_slice(&reduction.output_extent());
        encode.crop = encode.crop.map(|v| v / SCALE);
        let output = output_size.unwrap_or([encode.output[0] / SCALE, encode.output[1] / SCALE]);
        encode.output[..2].copy_from_slice(&output);
        Ok(Self { reduction, encode })
    }
}

#[cfg(test)]
#[path = "preview_test.rs"]
mod tests;
