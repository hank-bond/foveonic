//! Prepares filtered calibrated sensor grids and preserves fractional active-image framing.

use super::super::BuiltRecipe;
use crate::{
    color_space::ColorSpace,
    graph::{Graph, Operation},
    nodes::{color::pyramid_resample, sensor::calibration::EncodeParams},
    render::{Scale, WorkingGrid},
    source::Result,
};

// The global reconstruction can shrink this grid to 3/8. Its four-pixel
// boundary and one-pixel bilateral support need ceil(5 * 8/3) working pixels.
// Aligning that support to the reconstruction lattice gives sixteen pixels.
const FILTER_HALO: u32 = (5_u32 * 8).div_ceil(3).div_ceil(8) * 8;

pub(super) struct Grid {
    reductions: Vec<pyramid_resample::Prepared>,
    pub grid: WorkingGrid,
    native: EncodeParams,
    pub encode: EncodeParams,
}
impl Grid {
    /// Filter calibrated layers before expensive noise and color reconstruction.
    pub fn append(&self, graph: &mut Graph, mut input: usize) -> Result<usize> {
        let scope = graph.outputs[input].cache_name();
        for (step, prepared) in self.reductions.iter().enumerate() {
            input = graph.add_scoped(
                Operation::ReduceLayers(*prepared),
                &[input],
                &format!("sensor-grid/{scope}/{}", 1_u32 << (step + 1)),
            )?;
        }
        graph.retain_for_seal(input);
        Ok(input)
    }

    /// Retain noise controls on a smaller grid, using complete aligned processing tiles.
    pub fn noise(&self, mut params: crate::noise::Parameters) -> Result<crate::noise::Parameters> {
        let extent = self.reductions.last().unwrap().output_extent();
        // Small previews fit one processing tile. Keeping the native 4×4 split
        // repeats large halos and can create fractional internal resampling grids.
        params.source[2..].fill(1);
        params.window = [0, 0, 1, 1];
        let divisor = self.grid.scale.divisor();
        params.sampling = [divisor, 32_u32.div_ceil(divisor).max(1), 0, 0];
        // One working-grid step spans d native steps. Keep the recursive mean's
        // flat-field decay distance in native pixels rather than preview pixels.
        params.filter[3] = params.filter[3].powf(divisor as f32);
        // The native-code signal mean is unchanged by pooling. Only coefficients
        // representing its independent sensor noise change: layer models are
        // variances, while the luminance diffusion polynomial is a sigma.
        let noise_std = pyramid_resample::independent_noise_std(self.grid.scale);
        for coefficient in params.model.iter_mut().flatten() {
            *coefficient *= noise_std * noise_std;
        }
        for coefficient in &mut params.mono[..3] {
            *coefficient *= noise_std;
        }
        params.tile[2] = params.tile[2].div_ceil(divisor).max(8).div_ceil(8) * 8;
        params.tile[3] = (params.tile[3].div_ceil(divisor) | 1).max(3);
        params.geometry[0] = params.geometry[0].div_ceil(divisor).max(1);
        params.geometry[1] = params.geometry[1].div_ceil(divisor).max(1);
        params.geometry[2] = (params.geometry[2].div_ceil(divisor) | 1).max(1);
        params.column[1] = ((params.column[1] as u32).div_ceil(divisor) | 1).max(3) as f32;
        params.column[2] = (params.column[2] as u32).div_ceil(divisor).max(1) as f32;
        params.area = self.grid.filter_bounds(params.area);
        params.column_area = self.grid.filter_bounds(params.column_area);
        for (axis, length) in extent.into_iter().enumerate() {
            let tiles = params.source[axis + 2];
            if tiles == 0 || !length.is_multiple_of(tiles) {
                return Err("reduced noise grid requires complete processing tiles".into());
            }
            params.source[axis] = length;
            params.tile[axis] = length / tiles + 2 * params.tile[2];
        }
        params.region = [0, 0, extent[0], extent[1]];
        if let Err(error) = params.reduced_tile_extent() {
            return Err(format!(
                "{error}: preview source {:?}, tile {:?}, scale {:?}",
                params.source, params.tile, params.reduction
            )
            .into());
        }
        Ok(params)
    }

    pub fn new(
        mut encode: EncodeParams,
        output_size: Option<[u32; 2]>,
        quality: Scale,
    ) -> Result<Self> {
        let native = encode;
        let grid = WorkingGrid::new([encode.source[0], encode.source[1]], quality, 8)?
            .with_halo(FILTER_HALO)?;
        let divisor = quality.divisor();
        if quality.is_native() {
            return Err("sensor reduction requires a reduced working scale".into());
        }
        let mut extent = grid.native;
        let mut reductions = Vec::new();
        for step in 0..divisor.ilog2() {
            let output = if step + 1 == divisor.ilog2() {
                grid.extent
            } else {
                extent.map(|v| v.div_ceil(2))
            };
            let reduction = pyramid_resample::Prepared::half_to(extent, output)?
                .with_working_scale(Scale::new(1_u32 << (step + 1))?)
                .with_origin(if step + 1 == divisor.ilog2() {
                    grid.origin
                } else {
                    [0; 2]
                });
            extent = reduction.output_extent();
            reductions.push(reduction);
        }
        encode.source[..2].copy_from_slice(&extent);
        encode.crop = grid.bounds(native.crop);
        let output = output_size.unwrap_or(quality.extent([native.output[0], native.output[1]]));
        encode.output[..2].copy_from_slice(&output);
        Ok(Self {
            reductions,
            grid,
            native,
            encode,
        })
    }

    /// Camera active edges can lie between reduced pixels. Materialize their exact
    /// affine frame after reconstruction instead of rounding the photograph's crop.
    pub fn frame(&self, recipe: &mut BuiltRecipe, space: ColorSpace) -> Result<()> {
        let mut output = [self.encode.output[0], self.encode.output[1]];
        if matches!(self.native.source[2], 90 | 270) {
            output.swap(0, 1);
        }
        let d = self.grid.scale.divisor() as f32;
        let [l, t, r, b] = self.native.crop.map(|v| v as f32 / d);
        let [x, y] = self.grid.origin.map(|v| v as f32);
        let extent = [self.encode.source[0], self.encode.source[1]];
        recipe.final_id = recipe.graph.add(
            Operation::ResampleRgb {
                params: EncodeParams {
                    mapping: [[r - l, 0., l + x, 1.], [0., b - t, t + y, 1.]],
                    source: [extent[0], extent[1], 0, 0],
                    crop: [0, 0, extent[0], extent[1]],
                    output: [output[0], output[1], 0, 0],
                },
                space,
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
