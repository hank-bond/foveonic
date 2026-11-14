//! Pools corrected signed sensor codes and maps reduced Classic photographs onto their native frame.

use super::super::BuiltRecipe;
use crate::{
    color_space::ColorSpace,
    graph::{Graph, Operation},
    nodes::sensor::{calibration::EncodeParams, pool},
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
        let scope = graph.outputs[input].cache_name();
        let prepared = pool::Prepared::new(self.grid.native, self.divisor, pool::Signal::Rgb)?
            .with_signed_codes()
            .with_extent(self.grid.extent)?
            .with_origin(self.grid.origin)
            .with_working_scale(self.grid.scale);
        input = graph.add_scoped(
            Operation::ReduceLayers {
                prepared,
                data: crate::graph::DataType::CalibratedLayers,
            },
            &[input],
            &format!("sensor-grid/{scope}/{}", self.divisor),
        )?;
        graph.retain_for_seal(input);
        Ok(input)
    }

    pub fn frame(&self, recipe: &mut BuiltRecipe) -> Result<()> {
        let mut output = [self.encode.output[0], self.encode.output[1]];
        if matches!(self.native.source[2], 90 | 270) {
            output.swap(0, 1);
        }
        let extent = self.extent();
        recipe.final_id = recipe.graph.add(
            Operation::ResampleRgb {
                params: EncodeParams {
                    mapping: self.grid.frame_mapping(self.native.crop),
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
        let [l, t, r, b] = self.native.crop.map(f64::from);
        let step = [
            (r - l) / f64::from(output[0]),
            (b - t) / f64::from(output[1]),
        ];
        recipe.rgb_to_native = [
            [step[0], 0., l + 0.5 * step[0] - 0.5],
            [0., step[1], t + 0.5 * step[1] - 0.5],
        ];
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
