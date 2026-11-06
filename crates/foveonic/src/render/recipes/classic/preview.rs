//! Pools corrected signed sensor codes and maps reduced Classic photographs onto their native frame.

use super::super::BuiltRecipe;
use crate::{
    color_space::ColorSpace,
    graph::{Graph, Operation},
    nodes::{color::pyramid_resample, sensor::calibration::EncodeParams},
    render::{PreviewQuality, Settings},
    source::{Metadata, Result},
};

pub(in crate::render) fn available(
    metadata: &Metadata,
    settings: &Settings,
    quality: PreviewQuality,
) -> bool {
    // High-ISO range reconstruction keeps its native grid until its reduced
    // spatial equations have independent photographic validation.
    !settings.is_monochrome()
        && matches!(
            (metadata.width, metadata.height),
            (2304, 1531) | (2688, 1792)
        )
        && metadata
            .properties
            .get("RESOLUTION")
            .is_none_or(|resolution| {
                !resolution.trim().eq_ignore_ascii_case("MED")
                    && !resolution.trim().eq_ignore_ascii_case("LOW")
            })
        && super::prepare::capture_iso(metadata)
            .is_ok_and(|iso| iso < super::reference::RANGE_RECONSTRUCTION_ISO)
        && super::prepare::geometry(metadata)
            .and_then(|encode| Grid::new(encode, quality))
            .is_ok()
}

pub(super) struct Grid {
    native: EncodeParams,
    pub encode: EncodeParams,
    pub divisor: u32,
}
impl Grid {
    pub fn new(mut encode: EncodeParams, quality: PreviewQuality) -> Result<Self> {
        let divisor = quality.divisor();
        let extent = [encode.source[0], encode.source[1]];
        if !matches!(quality, PreviewQuality::Draft | PreviewQuality::Thumb)
            || extent.iter().any(|v| v.div_ceil(divisor) < 8)
            || encode.output[..2].iter().any(|v| v / divisor == 0)
        {
            return Err("Classic small preview requires a nonempty reconstruction grid".into());
        }
        let native = encode;
        encode.source[..2].copy_from_slice(&extent.map(|v| v.div_ceil(divisor)));
        encode.crop = std::array::from_fn(|i| {
            if i < 2 {
                native.crop[i] / divisor
            } else {
                native.crop[i].div_ceil(divisor).min(encode.source[i - 2])
            }
        });
        encode.output[..2]
            .copy_from_slice(&[native.output[0] / divisor, native.output[1] / divisor]);
        Ok(Self {
            native,
            encode,
            divisor,
        })
    }

    pub fn extent(&self) -> [u32; 2] {
        [self.encode.source[0], self.encode.source[1]]
    }

    pub fn append(&self, graph: &mut Graph, mut input: usize) -> Result<usize> {
        let mut extent = [self.native.source[0], self.native.source[1]];
        for level in 0..self.divisor.ilog2() {
            let prepared = pyramid_resample::Prepared::half_ceil(extent)?.with_signed_codes();
            extent = prepared.output_extent();
            input = graph.add_scoped(
                Operation::ReduceLayers(prepared),
                &[input],
                &format!("preview.sensor.{}", 1 << (level + 1)),
            )?;
        }
        Ok(input)
    }

    pub fn frame(&self, recipe: &mut BuiltRecipe) -> Result<()> {
        let mut output = [self.encode.output[0], self.encode.output[1]];
        if matches!(self.native.source[2], 90 | 270) {
            output.swap(0, 1);
        }
        let d = self.divisor as f32;
        let [l, t, r, b] = self.native.crop.map(|v| v as f32 / d);
        let extent = self.extent();
        recipe.final_id = recipe.graph.add(
            Operation::ResampleRgb {
                params: EncodeParams {
                    mapping: [[r - l, 0., l, 1.], [0., b - t, t, 1.]],
                    source: [extent[0], extent[1], 0, 0],
                    crop: [0, 0, extent[0], extent[1]],
                    output: [output[0], output[1], 0, 0],
                },
                space: ColorSpace::ProPhotoRgb,
            },
            &[recipe.final_id],
        )?;
        recipe.params.source[..2].copy_from_slice(&output);
        recipe.params.crop = [0, 0, output[0], output[1]];
        recipe.rgb_extent = output;
        recipe.rgb_bounds = recipe.params.crop;
        Ok(())
    }
}

#[cfg(test)]
#[path = "preview_test.rs"]
mod tests;
