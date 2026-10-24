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
