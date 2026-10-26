//! Selects authored photographic and DNG-base graphs before shared execution.

use super::{Engine, Settings};
use crate::{
    graph::Graph,
    interactive::Execution,
    nodes::sensor::calibration,
    profile::Profile,
    source::{Result, Source},
};

pub(super) mod classic;
pub(super) mod merrill;
pub(super) mod quattro;

pub(super) enum Recipe<'a> {
    Photographic(&'a [&'a str]),
    DngBase,
}

struct BuiltRecipe {
    graph: Graph,
    final_id: usize,
    params: calibration::EncodeParams,
}

fn requested(graph: &Graph, names: &[&str]) -> bool {
    !names.contains(&"final")
        && names
            .iter()
            .all(|name| graph.outputs.iter().any(|o| o.name == *name))
}

impl Engine {
    pub(super) fn render_recipe(
        &self,
        source: &Source,
        settings: &Settings,
        recipe: Recipe<'_>,
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<Vec<crate::inspection::Artifact>> {
        let names = match recipe {
            Recipe::Photographic(names) => names,
            Recipe::DngBase => &["final"],
        };
        execution.check()?;
        profile.mode.check_gpu_features(self.device().features())?;
        settings.validate_values()?;
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let effective = settings.processing_settings();
        let built = match recipe {
            Recipe::Photographic(_) => match source.metadata.processing_family()? {
                crate::source::ProcessingFamily::Merrill => {
                    merrill::build(self, source, &effective, names, profile, execution)?
                }
                crate::source::ProcessingFamily::Classic => {
                    classic::build(self, source, &effective, names, profile, execution)?
                }
                crate::source::ProcessingFamily::Quattro => profile
                    .measure("prepare recipe", || {
                        quattro::build(self, source, &effective, names, execution)
                    })?,
            },
            Recipe::DngBase => match source.metadata.processing_family()? {
                crate::source::ProcessingFamily::Merrill => {
                    merrill::build_dng(self, source, &effective, profile, execution)?
                }
                crate::source::ProcessingFamily::Classic => {
                    classic::build_dng(self, source, &effective, profile, execution)?
                }
                crate::source::ProcessingFamily::Quattro => profile
                    .measure("prepare Quattro DNG recipe", || {
                        quattro::build_dng(self, source, &effective, execution)
                    })?,
            },
        };
        if let Some(progress) = execution.progress {
            progress(super::RenderProgress::Prepared);
        }
        self.collect_artifacts(
            source,
            settings,
            &built.graph,
            built.final_id,
            names,
            built.params,
            profile,
            execution,
        )
    }
}

pub(in crate::render) fn saturation(dimensions: [u32; 4], factor: f32) -> calibration::ColorParams {
    // Fixed reference saturation weights are independent of the working primaries.
    let saturation_weights = [0.299, 0.587, 0.114];
    calibration::ColorParams {
        source: dimensions,
        matrix: std::array::from_fn(|row| {
            std::array::from_fn(|col| {
                if col == 3 {
                    0.
                } else {
                    (1. - factor) * saturation_weights[col] + if row == col { factor } else { 0. }
                }
            })
        }),
    }
}

/// Resolve camera interpretation once before the numerical DNG fitter.
pub(super) fn prepare_dng_appearance(
    metadata: &crate::source::Metadata,
    settings: &Settings,
    statistics: super::WhiteBalanceStats,
) -> Result<crate::dng::appearance::Prepared> {
    use crate::{dng::appearance::Prepared, source::ProcessingFamily};
    let effective = settings.processing_settings();
    let settings = &effective;
    let family = metadata.processing_family()?;
    let mut prepared = match family {
        ProcessingFamily::Merrill => merrill::prepare_dng_appearance(metadata, settings)?,
        ProcessingFamily::Classic => Prepared::signed(classic::prepare_dng_appearance(
            metadata, settings, statistics,
        )?)?,
        ProcessingFamily::Quattro => {
            Prepared::sampled(quattro::prepare_dng_appearance(metadata, settings)?)?
        }
    };
    if family != ProcessingFamily::Merrill {
        prepared.baked_color_mode = settings.is_monochrome();
    }
    Ok(prepared)
}

/// Keep source-derived sharpening policy beside each family's recipe.
pub(super) fn default_sharpen_amount(
    settings: &Settings,
    metadata: &crate::source::Metadata,
) -> Result<f32> {
    use crate::source::ProcessingFamily;
    match metadata.processing_family().ok() {
        Some(ProcessingFamily::Classic) => Ok(1.),
        Some(ProcessingFamily::Quattro) => quattro::default_sharpen_amount(settings, metadata),
        Some(ProcessingFamily::Merrill) if !settings.is_monochrome() => Ok(0.),
        _ => Ok(crate::noise::couplings(
            metadata,
            &settings.white_balance,
            settings.sensor_noise_controls(),
        )?
        .sharpen_base),
    }
}

/// Select source-derived statistics only for recipes that require them.
pub(super) fn estimated_white_balance(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    profile: &mut Profile,
) -> Result<super::WhiteBalanceStats> {
    if source.metadata.processing_family()? == crate::source::ProcessingFamily::Classic
        && !settings.is_monochrome()
    {
        classic::estimate_white_balance(engine, source, &settings.white_balance, profile)
    } else {
        Ok(super::WhiteBalanceStats::default())
    }
}
