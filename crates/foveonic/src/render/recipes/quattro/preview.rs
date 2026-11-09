//! Reduces factory-repaired Quattro RGB before image-wide speckle and highlight processing.

use super::{defects, highlights, stage2};
use crate::{
    graph::{DataType, Graph, Operation},
    nodes::{
        color::purple_fringe,
        sensor::{chroma_clusters, pool},
    },
    render::Scale,
    source::{Metadata, Result},
};
use std::sync::Arc;

pub(super) struct NativeRepairs {
    pub rgb: usize,
    pub defects: usize,
    pub clusters: Arc<chroma_clusters::Prepared>,
    pub optical: Option<Arc<purple_fringe::Prepared>>,
}

pub(super) struct WorkingInputs {
    pub rgb: usize,
    pub blue: usize,
    pub highlight_mask: usize,
    pub recovery_mask: Option<usize>,
}

/// Catalogue repairs keep native coordinates. Neighborhood detection and
/// recovery consume means and original recovery controls on the working grid.
pub(super) fn build(
    graph: &mut Graph,
    metadata: &Metadata,
    scale: Scale,
    native: NativeRepairs,
    white_balance: &str,
) -> Result<WorkingInputs> {
    let geometry = super::color_grid(metadata, scale)?;
    let mut repaired = graph.add_scoped(
        Operation::ChromaClusters(native.clusters),
        &[native.rgb, native.defects],
        "preview-native-repairs",
    )?;
    if let Some(prepared) = native.optical {
        repaired = graph.add_scoped(
            Operation::PurpleFringe(prepared),
            &[repaired],
            "preview-native-repairs",
        )?;
    }
    let pool = |signal| {
        pool::Prepared::new(geometry.native, geometry.scale.divisor(), signal)?
            .with_extent(geometry.extent)
            .map(|p| p.with_origin(geometry.origin).with_working_scale(scale))
    };
    let rgb = graph.add_scoped(
        Operation::PoolSignal(pool(pool::Signal::Rgb)?),
        &[repaired],
        "sensor-rgb-grid",
    )?;
    let evidence = graph.add_scoped(
        Operation::HighlightEvidence(
            highlights::prepare_restoration(metadata)?.evidence(pool(pool::Signal::Rgb)?)?,
        ),
        &[repaired],
        "sensor-clipping-grid",
    )?;
    let flags = graph.add_scoped(
        Operation::ReduceLayers {
            prepared: pool(pool::Signal::Flags)?,
            data: DataType::DefectFlags,
        },
        &[native.defects],
        "sensor-defect-grid",
    )?;
    for input in [rgb, evidence, flags] {
        graph.retain_for_session(input);
        graph.retain_for_seal(input);
    }
    let unfiltered = rgb;
    let energy = super::pool_noise_energy(geometry.native, geometry.scale);
    let rgb = graph.add_scoped(
        Operation::RgbDespeckle(Arc::new(
            defects::prepare_despeckle(metadata)?.on_grid(geometry, energy)?,
        )),
        &[rgb, flags],
        "working-grid",
    )?;
    let restored = graph.add_scoped(
        Operation::HighlightRestoration(Arc::new(
            highlights::prepare_restoration(metadata)?.on_grid(geometry)?,
        )),
        &[rgb, evidence, unfiltered],
        "working-grid",
    )?;
    let highlight_mask = graph.named_output(restored, "highlight-restoration.mask")?;
    let recovery_mask = if super::tiger::supported(metadata) {
        Some(graph.named_output(restored, "highlight-restoration.recovery-flags")?)
    } else {
        None
    };
    let rgb = if let Some(prepared) = stage2::prepare_shading(metadata, white_balance)? {
        graph.add_scoped(
            Operation::SpatialColorGain {
                prepared: Arc::new(prepared.on_grid(geometry)?),
                output: "preview.shaded-rgb",
            },
            &[restored],
            "working-grid",
        )?
    } else {
        restored
    };
    let blue = graph.add(Operation::ExtractBlue(geometry.extent), &[rgb])?;
    for input in [rgb, blue, highlight_mask].into_iter().chain(recovery_mask) {
        graph.retain_for_session(input);
        graph.retain_for_seal(input);
    }
    Ok(WorkingInputs {
        rgb,
        blue,
        highlight_mask,
        recovery_mask,
    })
}
