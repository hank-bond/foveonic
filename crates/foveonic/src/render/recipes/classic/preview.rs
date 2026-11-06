//! Pools corrected signed sensor codes and maps reduced Classic photographs onto their native frame.

use super::super::BuiltRecipe;
use crate::{
    color_space::ColorSpace,
    graph::{Graph, Operation},
    nodes::{color::pyramid_resample, sensor::calibration::EncodeParams},
    render::{Scale, WorkingGrid},
    source::Result,
};

pub(super) struct Grid {
    native: EncodeParams,
    pub encode: EncodeParams,
    pub grid: WorkingGrid,
    pub divisor: u32,
}
impl Grid {
    pub fn new(
        mut encode: EncodeParams,
        scale: Scale,
        output_size: Option<[u32; 2]>,
        halo: u32,
    ) -> Result<Self> {
        let native = encode;
        let grid =
            WorkingGrid::new([native.source[0], native.source[1]], scale, 8)?.with_halo(halo)?;
        encode.source[..2].copy_from_slice(&grid.extent);
        encode.crop = grid.bounds(native.crop);
        encode.output[..2].copy_from_slice(
            &output_size.unwrap_or_else(|| scale.extent([native.output[0], native.output[1]])),
        );
        Ok(Self {
            native,
            encode,
            grid,
            divisor: scale.divisor(),
        })
    }

    pub fn extent(&self) -> [u32; 2] {
        [self.encode.source[0], self.encode.source[1]]
    }

    pub fn append(&self, graph: &mut Graph, mut input: usize) -> Result<usize> {
        let mut extent = [self.native.source[0], self.native.source[1]];
        let scope = graph.outputs[input].cache_name();
        for level in 0..self.divisor.ilog2() {
            let output = if level + 1 == self.divisor.ilog2() {
                self.grid.extent
            } else {
                extent.map(|v| v.div_ceil(2))
            };
            let origin = if level + 1 == self.divisor.ilog2() {
                self.grid.origin
            } else {
                [0; 2]
            };
            let prepared = pyramid_resample::Prepared::half_to(extent, output)?
                .with_signed_codes()
                .with_origin(origin)
                .with_working_scale(Scale::new(1_u32 << (level + 1))?);
            extent = prepared.output_extent();
            input = graph.add_scoped(
                Operation::ReduceLayers(prepared),
                &[input],
                &format!("sensor-grid/{scope}/{}", 1_u32 << (level + 1)),
            )?;
        }
        graph.retain_for_seal(input);
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
                    mapping: [
                        [r - l, 0., l + self.grid.origin[0] as f32, 1.],
                        [0., b - t, t + self.grid.origin[1] as f32, 1.],
                    ],
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
        recipe
            .graph
            .set_view(recipe.final_id, super::super::camera_view(recipe.params));
        Ok(())
    }
}

#[cfg(test)]
#[path = "preview_test.rs"]
mod tests;
