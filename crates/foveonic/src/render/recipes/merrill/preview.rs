//! Prepares filtered calibrated sensor grids and preserves fractional active-image framing.

use super::super::BuiltRecipe;
use crate::{
    color_space::ColorSpace,
    graph::{Graph, Operation},
    nodes::{color::pyramid_resample, sensor::calibration::EncodeParams},
    render::PreviewQuality,
    source::Result,
};

pub(in crate::render) fn available(metadata: &crate::source::Metadata) -> bool {
    available_at(metadata, PreviewQuality::Fast)
}

pub(in crate::render) fn available_at(
    metadata: &crate::source::Metadata,
    quality: PreviewQuality,
) -> bool {
    metadata.processing_family().ok() == Some(crate::source::ProcessingFamily::Merrill)
        && crate::nodes::sensor::calibration::encode(metadata, ColorSpace::Srgb)
            .and_then(|encode| Grid::new(encode, None, quality))
            .is_ok()
}

pub(super) struct Grid {
    reductions: Vec<pyramid_resample::Prepared>,
    divisor: u32,
    native: EncodeParams,
    pub encode: EncodeParams,
}
impl Grid {
    /// Filter calibrated layers before expensive noise and color reconstruction.
    pub fn append(&self, graph: &mut Graph, mut input: usize) -> Result<usize> {
        for (step, prepared) in self.reductions.iter().enumerate() {
            input = if step == 0 {
                graph.add(Operation::ReduceLayers(*prepared), &[input])?
            } else {
                graph.add_scoped(
                    Operation::ReduceLayers(*prepared),
                    &[input],
                    &format!("preview.sensor.{}", 1 << (step + 1)),
                )?
            };
        }
        Ok(input)
    }

    /// Retain noise controls on a smaller grid, using complete aligned processing tiles.
    pub fn noise(&self, mut params: crate::noise::Parameters) -> Result<crate::noise::Parameters> {
        let extent = self.reductions.last().unwrap().output_extent();
        // Small previews fit one processing tile. Keeping the native 4×4 split
        // repeats large halos and can create fractional internal resampling grids.
        if self.divisor >= 4 {
            params.source[2..].fill(1);
            params.window = [0, 0, 1, 1];
        }
        for (axis, length) in extent.into_iter().enumerate() {
            let tiles = params.source[axis + 2];
            if tiles == 0 || !length.is_multiple_of(tiles) {
                return Err("reduced noise grid requires complete processing tiles".into());
            }
            params.source[axis] = length;
            params.tile[axis] = length / tiles + 2 * params.tile[2];
            params.area[axis] /= self.divisor;
            params.area[axis + 2] = params.area[axis + 2].div_ceil(self.divisor);
            params.column_area[axis] /= self.divisor;
            params.column_area[axis + 2] = params.column_area[axis + 2].div_ceil(self.divisor);
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
        quality: PreviewQuality,
    ) -> Result<Self> {
        let native = encode;
        let divisor = quality.divisor();
        let mut extent = [encode.source[0], encode.source[1]];
        if divisor < 2
            || extent.iter().any(|v| !v.is_multiple_of(divisor))
            || (divisor == 2 && encode.crop.iter().any(|v| !v.is_multiple_of(divisor)))
        {
            return Err(
                "Merrill reduced preview requires aligned sensor dimensions and crop".into(),
            );
        }
        // Reconstruction uses eighth-quantized scales. One small tile with its
        // retained halo must remain integral for every supported noise preset.
        if divisor >= 4 && extent.iter().any(|v| !(v / divisor).is_multiple_of(8)) {
            return Err("small preview requires an aligned reconstruction grid".into());
        }
        let mut reductions = Vec::new();
        for _ in 0..divisor.ilog2() {
            let reduction = pyramid_resample::Prepared::new(extent, pyramid_resample::Ratio::Half)?;
            extent = reduction.output_extent();
            reductions.push(reduction);
        }
        encode.source[..2].copy_from_slice(&extent);
        encode.crop = std::array::from_fn(|i| {
            if i < 2 {
                native.crop[i] / divisor
            } else {
                native.crop[i].div_ceil(divisor).min(extent[i - 2])
            }
        });
        let output =
            output_size.unwrap_or([native.output[0] / divisor, native.output[1] / divisor]);
        encode.output[..2].copy_from_slice(&output);
        Ok(Self {
            reductions,
            divisor,
            native,
            encode,
        })
    }

    /// Camera active edges can lie between reduced pixels. Materialize their exact
    /// affine frame after reconstruction instead of rounding the photograph's crop.
    pub fn frame(&self, recipe: &mut BuiltRecipe, space: ColorSpace) -> Result<()> {
        if self.divisor == 2 {
            return Ok(());
        }
        let mut output = [self.encode.output[0], self.encode.output[1]];
        if matches!(self.native.source[2], 90 | 270) {
            output.swap(0, 1);
        }
        let d = self.divisor as f32;
        let [l, t, r, b] = self.native.crop.map(|v| v as f32 / d);
        let extent = [self.encode.source[0], self.encode.source[1]];
        recipe.final_id = recipe.graph.add(
            Operation::ResampleRgb {
                params: EncodeParams {
                    mapping: [[r - l, 0., l, 1.], [0., b - t, t, 1.]],
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
        Ok(())
    }
}

/// Build the same native corrections used by the photographic recipe, then stop
/// at the reduced calibrated sensor. No original or native checkpoint is retained.
pub(in crate::render) fn prepare_sensor(
    engine: &crate::render::Engine,
    source: &crate::source::Source,
    settings: &crate::render::Settings,
    profile: &mut crate::profile::Profile,
    execution: &mut crate::interactive::Execution<'_>,
) -> Result<BuiltRecipe> {
    let space = ColorSpace::from_metadata(&source.metadata)?;
    let native = crate::nodes::sensor::calibration::encode(&source.metadata, space)?;
    let grid = Grid::new(native, None, settings.preview_quality)?;
    let policy = super::native_layers::Policy::new(settings, true, false);
    let mut recipe = match super::native_layers::build(
        engine,
        source,
        settings,
        native,
        &policy,
        &["final"],
        profile,
        execution,
    )? {
        super::native_layers::Output::Requested(recipe)
        | super::native_layers::Output::Ready { recipe, .. } => recipe,
    };
    recipe.final_id = grid.append(&mut recipe.graph, recipe.final_id)?;
    recipe.params = grid.encode;
    recipe.rgb_extent = [grid.encode.source[0], grid.encode.source[1]];
    recipe.rgb_bounds = grid.encode.crop;
    Ok(recipe)
}

#[cfg(test)]
#[path = "preview_test.rs"]
mod tests;
