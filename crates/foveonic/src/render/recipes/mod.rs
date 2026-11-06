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
#[cfg(test)]
#[path = "preview_test.rs"]
mod preview_tests;
pub(super) mod quattro;

/// A recipe keeps the original camera metadata even when native samples have been released.
#[derive(Clone, Copy)]
pub(super) enum RecipeInput<'a> {
    Original(&'a Source),
    Preparing {
        source: &'a Source,
        preparation: &'a super::source_preparation::SourcePreparation,
    },
    Metadata {
        metadata: &'a std::sync::Arc<crate::source::Metadata>,
        origin: &'a crate::source::SourceOrigin,
        preparation: &'a super::source_preparation::SourcePreparation,
    },
}
impl<'a> RecipeInput<'a> {
    pub fn metadata(self) -> &'a std::sync::Arc<crate::source::Metadata> {
        match self {
            Self::Original(source) | Self::Preparing { source, .. } => &source.metadata,
            Self::Metadata { metadata, .. } => metadata,
        }
    }
    pub fn origin(self) -> &'a crate::source::SourceOrigin {
        match self {
            Self::Original(source) | Self::Preparing { source, .. } => source.origin(),
            Self::Metadata { origin, .. } => origin,
        }
    }
    pub fn preparation(self) -> Option<&'a super::source_preparation::SourcePreparation> {
        match self {
            Self::Original(_) => None,
            Self::Preparing { preparation, .. } | Self::Metadata { preparation, .. } => {
                Some(preparation)
            }
        }
    }
    pub fn original(self) -> Result<&'a Source> {
        match self {
            Self::Original(source) | Self::Preparing { source, .. } => Ok(source),
            Self::Metadata { .. } => Err(Box::new(super::execute::SourceRequired)),
        }
    }
    pub fn context(self) -> super::execute::Input<'a> {
        match self {
            Self::Original(source) | Self::Preparing { source, .. } => {
                super::execute::Input::Source(source)
            }
            Self::Metadata { metadata, .. } => super::execute::Input::Metadata(metadata),
        }
    }
}

pub(super) enum Recipe<'a> {
    Photographic(&'a [&'a str]),
    DngBase,
}

pub(super) fn native_extent(metadata: &crate::source::Metadata) -> Result<[u32; 2]> {
    let params = match metadata.processing_family()? {
        crate::source::ProcessingFamily::Classic => classic::geometry(metadata)?,
        _ => calibration::encode(metadata, crate::color_space::ColorSpace::Srgb)?,
    };
    Ok([params.output[0], params.output[1]])
}

pub(super) struct BuiltRecipe {
    pub(super) graph: Graph,
    pub(super) final_id: usize,
    pub(super) params: calibration::EncodeParams,
    /// Working RGB grid at the shared photographic boundary, before output encoding.
    pub(super) rgb_extent: [u32; 2],
    /// Valid image bounds on that RGB grid, independent of sensor/output framing.
    pub(super) rgb_bounds: [u32; 4],
}

fn camera_view(params: calibration::EncodeParams) -> crate::image::View {
    crate::image::View {
        crop: params.crop,
        rotation: params.source[2],
        extent: [params.output[0], params.output[1]],
    }
}

fn requested(graph: &Graph, names: &[&str]) -> bool {
    !names.contains(&"final")
        && names
            .iter()
            .all(|name| graph.outputs.iter().any(|o| o.name == *name))
}

fn execution_settings<'a>(
    metadata: &crate::source::Metadata,
    settings: &'a Settings,
    names: &[&str],
) -> std::borrow::Cow<'a, Settings> {
    let mut request = std::borrow::Cow::Borrowed(settings);
    if settings.preview_region.is_some()
        && (settings.working_scale.is_reduced()
            || !matches!(names, ["final"] | ["linear.base"])
            || !settings.regional_preview_available(metadata))
    {
        request.to_mut().preview_region = None;
    }
    request
}

impl Engine {
    pub(super) fn render_recipe(
        &self,
        source: RecipeInput<'_>,
        settings: &Settings,
        recipe: Recipe<'_>,
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<Vec<crate::inspection::Artifact>> {
        let names = match recipe {
            Recipe::Photographic(names) => names,
            Recipe::DngBase => &["final"],
        };
        let preparation = profile.mode.enabled().then(std::time::Instant::now);
        execution.check()?;
        profile.mode.check_gpu_features(self.device().features())?;
        settings.validate_values()?;
        if names.is_empty() {
            return Ok(Vec::new());
        }
        // Image artifacts must describe the region actually rendered, so
        // whole-frame fallback still applies saved crop and rotation.
        let mut request = execution_settings(source.metadata(), settings, names);
        if matches!(recipe, Recipe::DngBase) {
            request.to_mut().working_scale = super::Scale::NATIVE;
            request.to_mut().preview_region = None;
        }
        let settings = request.as_ref();
        let resolved = settings.resolved_exposure(source.metadata());
        let dng_settings;
        let input = if matches!(recipe, Recipe::DngBase) {
            dng_settings = resolved.dng_settings();
            &dng_settings
        } else {
            resolved.as_ref()
        };
        let effective = input.processing_settings(source.metadata().processing_family()?);
        let mut built = match recipe {
            Recipe::Photographic(_) => build_photographic(
                self, source, &effective, settings, names, profile, execution,
            )?,
            Recipe::DngBase => build_dng(
                self,
                source.original()?,
                &effective,
                settings,
                profile,
                execution,
            )?,
        };
        linear_edits::prioritize_cache(&mut built.graph);
        if let Some(start) = preparation {
            profile.record("prepare render recipe", start);
        }
        if let Some(progress) = execution.progress {
            progress(super::RenderProgress::Prepared);
        }
        self.collect_artifacts(
            source,
            settings,
            &built.graph,
            built.final_id,
            names,
            profile,
            execution,
        )
    }
}

pub(super) fn build_camera(
    engine: &Engine,
    source: RecipeInput<'_>,
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
    match source.metadata().processing_family()? {
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

#[allow(clippy::too_many_arguments)]
fn build_photographic(
    engine: &Engine,
    source: RecipeInput<'_>,
    effective: &Settings,
    authored: &Settings,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let effective = effective.active_tone_settings();
    let camera = finishing::camera_settings(&effective);
    let built = build_camera(engine, source, &camera, names, profile, execution)?;
    let mut built = finishing::build(&effective, built, names, execution)?;
    if authored.dehaze_estimate
        && effective.dehaze_auto()
        && !built
            .graph
            .outputs
            .iter()
            .any(|o| o.name == "dehaze.analysis")
    {
        let analysis = build_camera(
            engine,
            source,
            &camera,
            &["dehaze.analysis"],
            profile,
            execution,
        )?;
        built.graph.append(analysis.graph, "dehaze-swatch");
    }
    Ok(built)
}

/// Select only declared sensor inputs reachable in this exact photographic graph.
pub(super) fn sealing_inputs(
    engine: &Engine,
    source: RecipeInput<'_>,
    settings: &Settings,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<Graph> {
    let request = execution_settings(source.metadata(), settings, &["final"]);
    let settings = request.as_ref();
    let resolved = settings.resolved_exposure(source.metadata());
    let effective = resolved.processing_settings(source.metadata().processing_family()?);
    let mut built = build_photographic(
        engine,
        source,
        &effective,
        settings,
        &["final"],
        profile,
        execution,
    )?;
    let mut roots = vec![built.final_id];
    if settings.dehaze_estimate && effective.dehaze_auto() {
        if let Some(estimate) = built
            .graph
            .outputs
            .iter()
            .position(|o| o.name == "dehaze.analysis")
        {
            roots.push(estimate);
        }
    }
    let required = built.graph.required(&roots)?;
    built.graph.sealed_outputs.retain(|id| required[*id]);
    Ok(built.graph)
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
    linear_edits::bake(
        engine,
        RecipeInput::Original(source),
        authored,
        &mut recipe,
        profile,
        execution,
    )?;
    Ok(recipe)
}

pub(in crate::render) fn saturation(
    mut dimensions: [u32; 4],
    factor: f32,
    protect: bool,
) -> calibration::ColorParams {
    // Fixed reference saturation weights are independent of the working primaries.
    let saturation_weights = [0.299, 0.587, 0.114];
    dimensions[2] = u32::from(protect && factor > 1.);
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
    let resolved = settings.resolved_exposure(metadata);
    let settings = resolved.as_ref();
    let effective_edits = settings.processing_settings(metadata.processing_family()?);
    let baked_chromatic_edits = effective_edits.linear_chromatic_active();
    let dng = settings.dng_settings();
    let effective = dng.processing_settings(metadata.processing_family()?);
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
    if settings.adaptive_tone_active() {
        prepared.defer_range_compensation();
    }
    if family != ProcessingFamily::Merrill {
        prepared.baked_color_mode = settings.is_monochrome();
    }
    if !settings.is_monochrome() && !settings.white_balance_bias.is_neutral() {
        let space = match family {
            ProcessingFamily::Classic => crate::color_space::ColorSpace::ProPhotoRgb,
            ProcessingFamily::Merrill => crate::color_space::ColorSpace::from_metadata(metadata)?,
            ProcessingFamily::Quattro => crate::color_space::ColorSpace::Srgb,
        };
        prepared.white_balance_bias(settings.white_balance_bias.matrix(space))?;
    }
    if baked_chromatic_edits {
        prepared.sample_notes = Some("Selected pre-tone color reconstruction and chromatic edits are baked through the inverse linear profile basis. Camera response, exposure and global presentation remain in the profile.");
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
        classic::estimate_white_balance(
            engine,
            RecipeInput::Original(source),
            &settings.white_balance,
            profile,
        )
    } else {
        Ok(super::WhiteBalanceStats::default())
    }
}
