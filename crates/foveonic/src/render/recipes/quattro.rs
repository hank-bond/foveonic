//! Builds inspection of independent-resolution sensor planes before photographic reconstruction.

mod calibration;
mod inspection;
pub(crate) mod resources;

pub(crate) use inspection::outputs;
#[cfg(test)]
mod stage1;

use super::{requested, BuiltRecipe};
use crate::{
    graph::{Graph, Operation},
    nodes::sensor::{calibration::EncodeParams, phase_extraction},
    render::Engine,
    source::{Result, Source, SourceStorage},
};

pub(super) fn build(engine: &Engine, source: &Source, names: &[&str]) -> Result<BuiltRecipe> {
    if source.storage() != SourceStorage::PackedRgbBluePhases {
        return Err("sensor plane inspection requires six-sample cells".into());
    }
    let available = outputs(Some(&source.metadata));
    if names
        .iter()
        .any(|name| !available.iter().any(|output| output.name == *name))
    {
        return Err("Quattro photographic reconstruction is not available yet. Select an available sensor or extraction output".into());
    }
    let [width, height] = source.storage_extent().map(|v| v as u32);
    let mut graph = Graph::default();
    let input = graph.add(Operation::SourcePhaseCells, &[])?;
    let raw = graph.add(
        Operation::PhaseExtraction(phase_extraction::Params::new(width, height)?),
        &[input],
    )?;
    let params = EncodeParams {
        source: [width * 2, height * 2, source.metadata.rotation, 0],
        crop: [0, 0, width * 2, height * 2],
        output: [width * 2, height * 2, 0, 0],
    };
    if requested(&graph, names) {
        return Ok(BuiltRecipe {
            graph,
            final_id: raw,
            params,
        });
    }

    let extracted = graph.add(
        Operation::PhaseExtraction(calibration::extraction(&source.metadata)?),
        &[input],
    )?;
    if requested(&graph, names) {
        return Ok(BuiltRecipe {
            graph,
            final_id: extracted,
            params,
        });
    }

    let blue = graph.named_output(extracted, "extraction.blue")?;
    let gain = calibration::phase_gain(&source.metadata, engine.calibration_resources.as_deref())?;
    let final_id = graph.add(Operation::PhaseGain(std::sync::Arc::new(gain)), &[blue])?;
    Ok(BuiltRecipe {
        graph,
        final_id,
        params,
    })
}
