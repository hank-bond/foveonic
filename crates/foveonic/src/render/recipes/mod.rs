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
mod finishing;
mod linear_edits;
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
    /// Working RGB grid at the shared photographic boundary, before output encoding.
    rgb_extent: [u32; 2],
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
        let dng_settings;
        let input = if matches!(recipe, Recipe::DngBase) {
            dng_settings = settings.dng_settings();
            &dng_settings
        } else {
            settings
        };
        let effective = input.processing_settings();
        let mut built = match recipe {
            Recipe::Photographic(_) => {
                let effective = effective.active_tone_settings();
                let camera = finishing::camera_settings(&effective);
                let built = build_camera(self, source, &camera, names, profile, execution)?;
                finishing::build(&effective, built, names, execution)?
            }
            Recipe::DngBase => build_dng(self, source, &effective, settings, profile, execution)?,
        };
        linear_edits::prioritize_cache(&mut built.graph);
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

fn build_camera(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let names = if names.iter().any(|n| n.starts_with("finish.")) {
        &["final"][..]
    } else {
        names
    };
    match source.metadata.processing_family()? {
        crate::source::ProcessingFamily::Merrill => {
            merrill::build(engine, source, settings, names, profile, execution)
        }
        crate::source::ProcessingFamily::Classic => {
            classic::build(engine, source, settings, names, profile, execution)
        }
        crate::source::ProcessingFamily::Quattro => {
            quattro::build(engine, source, settings, names, profile, execution)
        }
    }
}

fn build_dng(
    engine: &Engine,
    source: &Source,
    technical: &Settings,
    authored: &Settings,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let mut recipe = match source.metadata.processing_family()? {
        crate::source::ProcessingFamily::Merrill => {
            merrill::build_dng(engine, source, technical, profile, execution)
        }
        crate::source::ProcessingFamily::Classic => {
            classic::build_dng(engine, source, technical, profile, execution)
        }
        crate::source::ProcessingFamily::Quattro => {
            quattro::build_dng(engine, source, technical, profile, execution)
        }
    }?;
    linear_edits::bake(engine, source, authored, &mut recipe, profile, execution)?;
    Ok(recipe)
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
    let effective_edits = settings.processing_settings();
    let baked_color_editor = effective_edits.color_editor_chromatic_active();
    let dng = settings.dng_settings();
    let effective = dng.processing_settings();
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
    if baked_color_editor {
        prepared.sample_notes = Some("Selected pre-tone color reconstruction and Color Editor edits are baked through the inverse linear profile basis. Camera response, exposure and global presentation remain in the profile.");
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
