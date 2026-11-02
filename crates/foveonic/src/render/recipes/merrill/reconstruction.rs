//! Composes Merrill tiled noise, directional conditioning and texture reconstruction.

use super::super::requested;
use super::{preview, region};
use crate::{
    graph::{Graph, Operation},
    interactive::Execution,
    noise::{Controls, Parameters},
    profile::Profile,
    render::Settings,
    source::{Result, Source},
};
use std::sync::Arc;

pub(super) struct Request<'a> {
    pub source: &'a Source,
    pub settings: &'a Settings,
    pub names: &'a [&'a str],
    pub color: crate::nodes::sensor::calibration::ColorParams,
    pub space: crate::color_space::ColorSpace,
    pub preview: Option<&'a preview::Grid>,
    pub region: Option<&'a region::Prepared>,
    pub directional_banding: bool,
    pub native_calibrated: usize,
}

pub(super) struct Reconstructed {
    pub params: Parameters,
    pub tiles: usize,
}

/// None means construction stopped at the requested inspection output.
pub(super) fn append(
    request: Request<'_>,
    graph: &mut Graph,
    calibrated: usize,
    controls: Controls,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<Option<Reconstructed>> {
    macro_rules! checkpoint {
        () => {
            execution.check()?;
            if requested(graph, request.names) {
                return Ok(None);
            }
        };
    }
    let effects = controls.merrill_effects();
    // The Reference amount stays saved while Directional owns its strengths.
    let prepared_controls = if request.directional_banding {
        crate::noise::Controls {
            banding: crate::noise::Controls::default().banding,
            ..controls
        }
    } else {
        controls
    };
    let native_params = profile.measure("prepare reference noise", || {
        crate::noise::prepare(
            &request.source.metadata,
            &request.settings.white_balance,
            prepared_controls,
        )
    })?;
    let params = if let Some(grid) = request.preview {
        grid.noise(native_params)?
    } else if let Some(region) = request.region {
        native_params.for_region(region.physical)?
    } else {
        native_params
    };
    let base_inputs = if params.uses_global_context() {
        let context = graph.add(Operation::NoiseContext(params.full_frame()), &[calibrated])?;
        let edges = graph.named_output(context, "noise.global-edges")?;
        if request.region.is_some() {
            graph.retain_for_session(context);
            graph.retain_for_session(edges);
        }
        vec![context, edges]
    } else {
        vec![calibrated]
    };
    let base = graph.add(Operation::NoiseBase(params), &base_inputs)?;
    checkpoint!();
    let native_columns = if effects.noise {
        profile.measure("prepare column cuts", || {
            crate::noise::column_parameters(&request.source.metadata, native_params)
        })?
    } else {
        native_params
    };
    let column_params = if let Some(grid) = request.preview {
        grid.noise(native_columns)?
    } else if let Some(region) = request.region {
        native_columns.for_region(region.physical)?
    } else {
        native_columns
    };
    let conditioned = if request.directional_banding && effects.noise {
        let bands = append_banding(
            graph,
            request.native_calibrated,
            Arc::new(crate::noise::banding_parameters(
                &request.source.metadata,
                native_columns,
                request.settings.merrill_banding,
            )?),
        )?;
        // Pooling uses the same complete native field for every view. Regional
        // views pack original tiles, preserving their halos and seam phase.
        if request.region.is_some() {
            graph.retain_for_session(bands);
        }
        if let Some(grid) = request.preview {
            let native = graph.named_output(bands, "column.layers")?;
            let reduced = graph.add(Operation::ReduceLayers(grid.reduction), &[native])?;
            graph.add(
                Operation::DirectionalBanding {
                    stage: crate::nodes::color::directional_banding::Stage::Retile,
                    prepared: Arc::new(crate::noise::banding_parameters(
                        &request.source.metadata,
                        column_params,
                        request.settings.merrill_banding,
                    )?),
                },
                &[reduced],
            )?
        } else if request.region.is_some() {
            graph.add(
                Operation::DirectionalBanding {
                    stage: crate::nodes::color::directional_banding::Stage::Window,
                    prepared: Arc::new(crate::noise::banding_parameters(
                        &request.source.metadata,
                        column_params,
                        request.settings.merrill_banding,
                    )?),
                },
                &[bands],
            )?
        } else {
            bands
        }
    } else {
        graph.add(
            Operation::Column {
                params: column_params,
                enabled: effects.noise && request.settings.reference_banding_selected(),
            },
            &[calibrated],
        )?
    };
    checkpoint!();
    let mut active =
        crate::nodes::sensor::calibration::rectangle(&request.source.metadata, "ActiveImageArea")?;
    for axis in 0..2 {
        let native = [
            request.source.metadata.width as u32,
            request.source.metadata.height as u32,
        ][axis];
        active[axis] =
            (u64::from(active[axis]) * u64::from(params.source[axis]) / u64::from(native)) as u32;
        active[axis + 2] = (u64::from(active[axis + 2]) * u64::from(params.source[axis]))
            .div_ceil(u64::from(native)) as u32;
    }
    let color =
        crate::nodes::color::texture::Color::new(request.color.matrix, request.space, active)?;
    let prepared = crate::nodes::color::texture::Prepared::new(
        controls,
        params.source[0] as f32 / request.source.metadata.width as f32,
        color,
    )?;
    let inspect_ratio = request
        .names
        .iter()
        .any(|name| matches!(*name, "detail.raw-ratio" | "detail.ratio" | "pattern.mask"));
    let ratio = if prepared.needs_ratio() || inspect_ratio {
        let ratio = graph.add(Operation::Ratio(params), &[conditioned, base])?;
        checkpoint!();
        let pattern_recipe = profile.measure("prepare pattern recipe", || {
            crate::noise::pattern_parameters(&request.source.metadata, params)
        })?;
        let seed = graph.named_output(base, "noise.pattern-seed-tiles")?;
        let pattern = graph.add(
            Operation::Pattern {
                params,
                prepared: pattern_recipe,
            },
            &[ratio, seed],
        )?;
        checkpoint!();
        Some(pattern)
    } else {
        None
    };
    let mut texture_inputs = vec![conditioned];
    if prepared.needs_base() {
        texture_inputs.push(base);
    }
    if prepared.needs_ratio() {
        texture_inputs.push(ratio.ok_or("missing luminance conditioning")?);
    }
    let restored = graph.add(Operation::Texture { params, prepared }, &texture_inputs)?;
    checkpoint!();
    Ok(Some(Reconstructed {
        params,
        tiles: restored,
    }))
}

/// Freeze both Fine axes before fitting residual Coarse, with independent measurement keys.
pub(super) fn append_banding(
    graph: &mut Graph,
    calibrated: usize,
    prepared: Arc<crate::nodes::color::directional_banding::Prepared>,
) -> Result<usize> {
    use crate::nodes::color::directional_banding::{Axis, Band, Stage};
    let mut tiles = graph.add(
        Operation::DirectionalBanding {
            stage: Stage::Tiles,
            prepared: prepared.clone(),
        },
        &[calibrated],
    )?;
    for band in [Band::Fine, Band::Coarse] {
        for axis in [Axis::Columns, Axis::Rows] {
            let estimates = graph.add(
                Operation::DirectionalBanding {
                    stage: Stage::Measure(band, axis),
                    prepared: prepared.clone(),
                },
                &[tiles],
            )?;
            // Compact offset tables survive activation so the first strength
            // adjustment can reuse them within the existing cache budget.
            graph.retain_for_session(estimates);
            tiles = graph.add(
                Operation::DirectionalBanding {
                    stage: Stage::Correct(band, axis),
                    prepared: prepared.clone(),
                },
                &[tiles, estimates],
            )?;
        }
    }
    Ok(tiles)
}
