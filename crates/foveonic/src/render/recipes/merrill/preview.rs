//! Prepares pooled calibrated sensor grids and preserves fractional active-image framing.

use super::super::BuiltRecipe;
use crate::{
    color_space::ColorSpace,
    graph::{Graph, Operation},
    nodes::sensor::{calibration::EncodeParams, pool},
    render::{Scale, WorkingGrid},
    source::Result,
};

// The global reconstruction can shrink this grid to 3/8. Its four-pixel
// boundary and one-pixel bilateral support need ceil(5 * 8/3) working pixels.
// Aligning that support to the reconstruction lattice gives sixteen pixels.
const FILTER_HALO: u32 = (5_u32 * 8).div_ceil(3).div_ceil(8) * 8;

pub(super) struct Grid {
    reduction: pool::Prepared,
    pub grid: WorkingGrid,
    native: EncodeParams,
    pub encode: EncodeParams,
}
impl Grid {
    /// Filter triplet layers onto the same covered sensor grid.
    pub fn append(&self, graph: &mut Graph, mut input: usize) -> Result<usize> {
        // Recovery and Compact despeckle share the same pooled sensor codes.
        if let Some(node) = graph.nodes.iter().find(|node| {
            matches!(&node.operation, Operation::ReduceLayers { prepared, .. }
                if prepared.key() == self.reduction.key())
                && node.inputs == [input]
        }) {
            return Ok(node.outputs[0]);
        }
        let scope = graph.outputs[input].cache_name();
        let data = graph.outputs[input].data;
        input = graph.add_scoped(
            Operation::ReduceLayers {
                prepared: self.reduction,
                data,
            },
            &[input],
            &format!("sensor-grid/{scope}/{}", self.grid.scale.divisor()),
        )?;
        graph.retain_for_seal(input);
        Ok(input)
    }

    /// Retain noise controls on a smaller grid, using complete aligned processing tiles.
    pub fn noise(&self, mut params: crate::noise::Parameters) -> Result<crate::noise::Parameters> {
        let extent = self.reduction.output_extent();
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
        let noise_std = pool::independent_noise_std(self.grid.scale);
        for coefficient in params.model.iter_mut().flatten() {
            *coefficient *= noise_std * noise_std;
        }
        for coefficient in &mut params.mono[..3] {
            *coefficient *= noise_std;
        }
        // Tile projection still blends five working pixels at its boundary.
        params.tile[2] = (params.tile[2].div_ceil(divisor) + 5).max(8).div_ceil(8) * 8;
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
        params.input_bounds = params.region;
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
        let reduction = pool::Prepared::new(grid.native, divisor, pool::Signal::Rgb)?
            .with_extent(grid.extent)?
            .with_working_scale(quality)
            .with_origin(grid.origin);
        let extent = reduction.output_extent();
        encode.source[..2].copy_from_slice(&extent);
        encode.crop = grid.bounds(native.crop);
        let output = output_size.unwrap_or(quality.extent([native.output[0], native.output[1]]));
        encode.output[..2].copy_from_slice(&output);
        Ok(Self {
            reduction,
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
        let extent = [self.encode.source[0], self.encode.source[1]];
        recipe.final_id = recipe.graph.add(
            Operation::ResampleRgb {
                params: EncodeParams {
                    mapping: self.grid.frame_mapping(self.native.crop),
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
