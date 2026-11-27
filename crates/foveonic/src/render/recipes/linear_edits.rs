//! Composes shared pre-curve edits and DNG baking using camera-supplied signal context.

use super::{requested, BuiltRecipe};
use crate::{
    color_space::ColorSpace,
    graph::{DataType, Graph, Operation},
    interactive::Execution,
    nodes::finish::{
        color_editor, perceptual_grade,
        photographic::{local_contrast, Params},
        tone_equalizer,
    },
    profile::Profile,
    render::{Engine, Rendering, Scale, Settings},
    source::Result,
};

/// Camera response and the shared profile-dependent analysis scale.
#[derive(Clone)]
pub(super) struct Adaptive {
    pub params: crate::nodes::finish::adaptive_tone::Params,
    pub curve: crate::recipe::Shared<f32>,
    pub analysis_scale: Scale,
}
impl Adaptive {
    pub(super) fn new(
        settings: &Settings,
        recipe: &BuiltRecipe,
        space: ColorSpace,
        white: f32,
        curve: impl Into<crate::recipe::Shared<f32>>,
        thumb_scale: Scale,
    ) -> Result<Self> {
        let curve = curve.into();
        Ok(Self {
            params: crate::nodes::finish::adaptive_tone::Params::new(
                recipe.rgb_extent,
                recipe.rgb_bounds,
                white,
                space,
                settings.photographic.base_response,
                settings.adaptive_tone_tuning,
                &curve,
                settings.processing_quality,
            )?
            .with_exposure(settings.exposure)?,
            curve,
            analysis_scale: adaptive_analysis_scale(settings, thumb_scale)?,
        })
    }
}

/// Profiles select a target grid. Preparation never requests finer data than
/// the main working grid, including explicit scales coarser than Thumb.
fn adaptive_analysis_scale(settings: &Settings, thumb_scale: Scale) -> Result<Scale> {
    if settings.processing_quality == crate::render::ProcessingQuality::Quality {
        return Ok(settings.working_scale);
    }
    let divisor = if settings.photographic.base_response == crate::render::BaseResponse::Strong {
        (thumb_scale.divisor() / 2).max(1)
    } else {
        thumb_scale.divisor()
    };
    Ok(settings.working_scale.coarsest(Scale::new(divisor)?))
}

/// Camera recipes supply code units and geometry. Numerical nodes do not identify cameras.
#[derive(Clone)]
pub(super) struct Context {
    pub dehaze_basis: Option<crate::nodes::finish::dehaze::Basis>,
    pub adaptive: Option<Adaptive>,
    pub space: ColorSpace,
    /// Nominal white of the main linear image.
    pub white: f32,
    /// Classifier normalization; Merrill retains its established 4096-code unit.
    pub guide_white: f32,
    /// Working pixels per native pixel, independent of viewport crop size.
    pub scale: f32,
}

/// Camera responses use a 16-bit code lattice. Evaluate metadata curves only.
pub(super) fn pivot_from_response(white: f32, response: impl Fn(f64) -> f64) -> Result<f32> {
    let curve = response_curve(response);
    tone_equalizer::pivot_with_white(&curve, 0., white)
}

pub(super) fn response_curve(response: impl Fn(f64) -> f64) -> Vec<f32> {
    const CODE_TABLE_LENGTH: usize = 1 << 16;
    (0..CODE_TABLE_LENGTH)
        .map(|code| response(code as f64) as f32)
        .collect()
}

pub(super) fn active(settings: &Settings) -> bool {
    (!settings.is_monochrome() && !settings.white_balance_bias.is_neutral())
        || settings.advanced_color_active()
        || baked_active(settings)
}
fn baked_active(settings: &Settings) -> bool {
    settings.dehaze_active()
        || settings.adaptive_tone_active()
        || settings.color_editor_active()
        || settings.universal_saturation_active()
        || (settings.tone_enabled
            && (!settings.tonal_ranges.is_neutral()
                || settings.photographic.brightness != 0.
                || settings.photographic.local_contrast != 0.))
        || (settings.sharpening_enabled && settings.photographic.sharpening.is_active())
}

pub(super) fn needs_dehaze(settings: &Settings, names: &[&str]) -> bool {
    settings.dehaze_active() || names.iter().any(|name| name.starts_with("dehaze."))
}

pub(super) fn requested_output(name: &str) -> bool {
    matches!(
        name,
        "dehaze.input"
            | "dehaze.rgb"
            | "dehaze.analysis"
            | "base-response.mask"
            | "base-response.rgb"
            | "base-response.analysis"
            | "base-response.analysis-mask"
            | "linear.base"
            | "linear.edits-input"
            | "linear.edits"
            | "color-editor.input"
            | "color-editor.guide"
            | "color-editor.rgb"
            | "advanced-color.input"
            | "advanced-color.rgb"
            | "perceptual.input"
            | "perceptual.rgb"
            | "perceptual.shadows"
            | "perceptual.midtones"
            | "perceptual.highlights"
            | "tonal-ranges.mask"
            | "tonal-ranges.rgb"
            | "brightness.input"
            | "linear.brightness"
            | "local-contrast.input"
            | "linear.local-contrast"
            | "linear.sharpen-input"
            | "linear.sharpen"
    )
}

pub(super) fn alias(
    recipe: &mut BuiltRecipe,
    space: ColorSpace,
    output: &'static str,
) -> Result<()> {
    recipe.final_id = recipe.graph.add(
        Operation::ImageAlias {
            dehaze_basis: None,
            data: DataType::LinearRgb(space),
            output,
        },
        &[recipe.final_id],
    )?;
    let mut view = super::camera_view(recipe.params);
    view.crop = recipe.rgb_bounds;
    recipe.graph.set_view(recipe.final_id, view);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    engine: &Engine,
    source: super::RecipeInput<'_>,
    settings: &Settings,
    recipe: &mut BuiltRecipe,
    names: &[&str],
    context: Context,
    pivot: impl FnOnce(&mut Profile) -> Result<f32>,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<()> {
    let Context {
        dehaze_basis,
        adaptive,
        space,
        white,
        guide_white,
        scale,
    } = context;
    let haze_scale = settings
        .processing_quality
        .guide_scale(settings.working_scale);
    let extent = recipe.rgb_extent;
    white_balance(recipe, settings, space)?;
    alias(recipe, space, "linear.edits-input")?;
    if requested(&recipe.graph, names) {
        return Ok(());
    }
    if needs_dehaze(settings, names) {
        let basis = dehaze_basis.ok_or("missing Dehaze camera basis")?;
        recipe.final_id = recipe.graph.add(
            Operation::ImageAlias {
                data: DataType::LinearRgb(space),
                output: "dehaze.input",
                dehaze_basis: Some(basis),
            },
            &[recipe.final_id],
        )?;
        if requested(&recipe.graph, names) {
            return Ok(());
        }
        let input = recipe.final_id;
        recipe.graph.retain_for_session(input);
        let analysis = haze_analysis(
            engine, source, settings, recipe, space, white, basis, haze_scale, profile, execution,
        )?;
        recipe.graph.retain_for_session(analysis);
        recipe.graph.prioritize_for_session(analysis);
        if requested(&recipe.graph, names) {
            return Ok(());
        }
        if settings.dehaze_active() {
            recipe.final_id = recipe.graph.add(
                Operation::Dehaze {
                    params: crate::nodes::finish::dehaze::Params::new(
                        extent,
                        basis,
                        white,
                        settings.is_monochrome(),
                        settings.dehaze,
                    ),
                    space,
                },
                &[input, analysis],
            )?;
        } else {
            alias(recipe, space, "dehaze.rgb")?;
        }
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(());
        }
    }
    if let Some(Adaptive {
        params,
        curve,
        analysis_scale,
    }) = adaptive
    {
        let params = params.with_geometry(extent, recipe.rgb_bounds, scale)?;
        let input = recipe.final_id;
        recipe.graph.retain_for_session(input);
        let regional = settings.preview_region.is_some();
        let application_scale = settings
            .processing_quality
            .guide_scale(settings.working_scale);
        let separate_guide = regional || application_scale != settings.working_scale;
        let mask = if separate_guide {
            None
        } else {
            let guide =
                tone_equalizer::Guide::new(extent, tone_equalizer::radius_for_scale(scale), space)?
                    .with_white(white);
            let mask = recipe
                .graph
                .add(Operation::AdaptiveToneGuide { guide, space }, &[input])?;
            mask_inspection(&mut recipe.graph, mask, guide, "base-response.mask", names)?;
            recipe.graph.retain_for_session(mask);
            Some((mask, guide))
        };
        let (analysis_rgb, analysis_mask, analysis, analysis_mapping) =
            if analysis_scale == settings.working_scale && !separate_guide {
                let (mask, guide) = mask.expect("whole-photo application prepared its guide");
                let rgb = recipe.graph.add(
                    Operation::ImageAlias {
                        dehaze_basis: None,
                        data: DataType::LinearRgb(space),
                        output: "base-response.analysis",
                    },
                    &[input],
                )?;
                let mask = recipe.graph.add(
                    Operation::ImageAlias {
                        dehaze_basis: None,
                        data: DataType::BrightnessGuide,
                        output: "base-response.analysis-guide",
                    },
                    &[mask],
                )?;
                mask_inspection(
                    &mut recipe.graph,
                    mask,
                    guide,
                    "base-response.analysis-mask",
                    names,
                )?;
                (rgb, mask, params, recipe.rgb_to_native)
            } else {
                adaptive_analysis(
                    engine,
                    source,
                    settings,
                    recipe,
                    space,
                    white,
                    dehaze_basis,
                    params,
                    analysis_scale,
                    names,
                    profile,
                    execution,
                )?
            };
        let (mask, params) = if separate_guide {
            let (application_mask, application, application_mapping) =
                if application_scale == analysis_scale {
                    let guide = tone_equalizer::Guide::new(
                        analysis.extent(),
                        tone_equalizer::radius_for_scale(analysis_scale.factor()),
                        space,
                    )?
                    .with_white(white);
                    mask_inspection(
                        &mut recipe.graph,
                        analysis_mask,
                        guide,
                        "base-response.mask",
                        names,
                    )?;
                    (analysis_mask, analysis, analysis_mapping)
                } else {
                    adaptive_application_guide(
                        engine,
                        source,
                        settings,
                        recipe,
                        space,
                        white,
                        dehaze_basis,
                        params,
                        application_scale,
                        names,
                        profile,
                        execution,
                    )?
                };
            (
                application_mask,
                params.with_application_guide(
                    application,
                    guide_mapping(recipe.rgb_to_native, application_mapping)?,
                    application_scale == settings.working_scale,
                )?,
            )
        } else {
            (
                mask.expect("whole-photo application prepared its guide").0,
                params,
            )
        };
        recipe.final_id = recipe.graph.add(
            Operation::AdaptiveTone {
                window: Default::default(),
                params,
                analysis,
                curve,
                space,
            },
            &[input, mask, analysis_rgb, analysis_mask],
        )?;
        recipe.graph.retain_for_session(recipe.final_id);
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(());
        }
    }
    let grade_requested = names.iter().any(|name| name.starts_with("perceptual."));
    let hdr_requested = names
        .iter()
        .any(|name| matches!(*name, "tonal-ranges.mask" | "tonal-ranges.rgb"));
    let tonal_pivot = if settings.universal_saturation_active()
        || grade_requested
        || !settings.tonal_ranges.is_neutral()
        || hdr_requested
    {
        pivot(profile)?
    } else {
        0.
    };
    if settings.universal_saturation()
        && (settings.universal_saturation_active() || grade_requested)
    {
        alias(recipe, space, "perceptual.input")?;
        let input = recipe.final_id;
        recipe.graph.retain_for_session(input);
        let params = perceptual_grade::Params::new(
            extent,
            space,
            white,
            tonal_pivot,
            settings.photographic.perceptual,
            settings.is_monochrome(),
        )
        .with_clipping_protection(settings.prevent_color_clipping);
        for (band, name) in [
            "perceptual.shadows",
            "perceptual.midtones",
            "perceptual.highlights",
        ]
        .into_iter()
        .enumerate()
        {
            if names.contains(&name) {
                recipe.graph.add(
                    Operation::PerceptualGrade {
                        params: params.with_mask(band as u32),
                        space,
                    },
                    &[input],
                )?;
            }
        }
        if requested(&recipe.graph, names) {
            return Ok(());
        }
        if settings.universal_saturation_active() {
            recipe.final_id = recipe
                .graph
                .add(Operation::PerceptualGrade { params, space }, &[input])?;
        } else {
            alias(recipe, space, "perceptual.rgb")?;
        }
        if requested(&recipe.graph, names) {
            return Ok(());
        }
    }
    let mut advanced_guide = None;
    if settings.color_editor_active()
        || settings.advanced_color_active()
        || names.contains(&"color-editor.guide")
    {
        let mut guide = color_guide(engine, source, settings, profile, execution)?;
        let DataType::LinearRgb(guide_space) = guide.graph.outputs[guide.final_id].data else {
            return Err("color guide must be linear RGB".into());
        };
        if guide_space != space
            || guide.rgb_extent != extent
            || guide.params.source[..2] != recipe.params.source[..2]
            || guide.params.output[..2] != recipe.params.output[..2]
            || guide.params.crop != recipe.params.crop
        {
            return Err("Color Editor guide framing mismatch".into());
        }
        white_balance(&mut guide, settings, space)?;
        let guide_final = guide.final_id;
        let mapping = recipe.graph.append(guide.graph, "color-editor-guide");
        let guide_id = recipe.graph.add(
            Operation::ImageAlias {
                dehaze_basis: None,
                data: DataType::LinearRgb(space),
                output: "color-editor.guide",
            },
            &[mapping[guide_final]],
        )?;
        advanced_guide = Some(guide_id);
        if requested(&recipe.graph, names) {
            return Ok(());
        }
        alias(recipe, space, "color-editor.input")?;
        let params = color_editor::Params::new(
            extent,
            settings.photographic.color_editor,
            space,
            white,
            guide_white,
            scale,
            settings.is_monochrome(),
        )
        .with_clipping_protection(settings.prevent_color_clipping);
        recipe.final_id = recipe.graph.add(
            Operation::LinearColorEditor { params, space },
            &[recipe.final_id, guide_id],
        )?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(());
        }
    }
    if !settings.tonal_ranges.is_neutral()
        || names
            .iter()
            .any(|n| matches!(*n, "tonal-ranges.mask" | "tonal-ranges.rgb"))
    {
        let input = recipe.final_id;
        recipe.graph.retain_for_session(input);
        let guide_scale = settings
            .processing_quality
            .guide_scale(settings.working_scale);
        let (mask, sampling) = if settings.preview_region.is_some()
            || guide_scale != settings.working_scale
        {
            let (mask, sampling) =
                dynamic_range_guide(engine, source, settings, recipe, profile, execution)?;
            (mask, Some(sampling))
        } else {
            let guide =
                tone_equalizer::Guide::new(extent, tone_equalizer::radius_for_scale(scale), space)?
                    .with_white(white);
            let mask = recipe
                .graph
                .add(Operation::ToneMask { guide, space }, &[input])?;
            recipe.graph.retain_for_session(mask);
            mask_inspection(&mut recipe.graph, mask, guide, "tonal-ranges.mask", names)?;
            (mask, None)
        };
        if requested(&recipe.graph, names) {
            return Ok(());
        }
        recipe.final_id = recipe.graph.add(
            Operation::TonalRanges {
                sampling,
                window: Default::default(),
                controls: settings.tonal_ranges,
                pivot: tonal_pivot,
                white,
                space,
            },
            &[input, mask],
        )?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(());
        }
    }
    if settings.photographic.brightness != 0. {
        alias(recipe, space, "brightness.input")?;
        recipe.graph.retain_for_session(recipe.final_id);
        let mut params = Params::new(extent, space);
        params.values[0] = (2. * settings.photographic.brightness).exp2();
        // Camera adapters apply Exposure later with their response. Classify in
        // the exposed domain here, then transfer only the brightness gain.
        params.luma = params.luma.map(|v| v * settings.exposure.exp2() / white);
        recipe.final_id = recipe.graph.add(
            Operation::LinearBrightness { params, space },
            &[recipe.final_id],
        )?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(());
        }
    }
    if settings.photographic.local_contrast != 0. {
        let mut params =
            Params::local_contrast(extent, space, settings.photographic.local_contrast, scale);
        params.luma = params.luma.map(|v| v / white);
        alias(recipe, space, "local-contrast.input")?;
        recipe.graph.retain_for_session(recipe.final_id);
        recipe.final_id = recipe.graph.add(
            Operation::LinearLocalContrast { params, space },
            &[recipe.final_id],
        )?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(());
        }
    }
    if settings.sharpening_enabled && settings.photographic.sharpening.is_active() {
        let mut params = Params::sharpening(extent, space, settings.photographic.sharpening, scale);
        params.luma = params.luma.map(|v| v / white);
        alias(recipe, space, "linear.sharpen-input")?;
        recipe.graph.retain_for_session(recipe.final_id);
        recipe.final_id = recipe.graph.add(
            Operation::LinearUniversalSharpen { params, space },
            &[recipe.final_id],
        )?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(());
        }
    }
    if settings.advanced_color_active() {
        alias(recipe, space, "advanced-color.input")?;
        recipe.graph.retain_for_session(recipe.final_id);
        if requested(&recipe.graph, names) {
            return Ok(());
        }
        let params = crate::nodes::finish::advanced_color::Params::new(
            extent,
            settings.photographic.color_editor.advanced,
            space,
            white,
            guide_white,
            settings.exposure,
            settings.is_monochrome(),
            settings.color_range_preview,
        )
        .with_clipping_protection(settings.prevent_color_clipping);
        recipe.final_id = recipe.graph.add(
            Operation::AdvancedColor {
                params: Box::new(params),
                space,
            },
            &[
                recipe.final_id,
                advanced_guide.ok_or("missing Advanced color guide")?,
            ],
        )?;
        execution.check()?;
        if requested(&recipe.graph, names) {
            return Ok(());
        }
    }
    alias(recipe, space, "linear.edits")
}

/// Shared camera RGB preparation for whole-photo analysis consumers. The caller
/// owns its processing choices. This boundary owns only scale and presentation.
fn whole_photo_rgb(
    engine: &Engine,
    source: super::RecipeInput<'_>,
    mut settings: Settings,
    scale: Scale,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<(Settings, BuiltRecipe)> {
    settings.working_scale = scale;
    settings.preview_region = None;
    settings.output_size = None;
    let recipe = super::build_camera(
        engine,
        source,
        &settings,
        &["linear.base"],
        profile,
        execution,
    )?;
    Ok((settings, recipe))
}

/// Rebuild only the reduced input to Dynamic Range, including its preceding edits.
/// The inspection stop prevents recursively applying Dynamic Range itself.
fn dynamic_range_guide(
    engine: &Engine,
    source: super::RecipeInput<'_>,
    settings: &Settings,
    recipe: &mut BuiltRecipe,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<(usize, tone_equalizer::GuideSampling)> {
    let mut overview = settings.clone();
    overview.working_scale = settings
        .processing_quality
        .guide_scale(settings.working_scale);
    overview.preview_region = None;
    overview.output_size = None;
    overview.dehaze_estimate = false;
    let branch = super::build_camera(
        engine,
        source,
        &overview,
        &["tonal-ranges.mask"],
        profile,
        execution,
    )?;
    let mask = branch
        .graph
        .outputs
        .iter()
        .position(|output| output.name == "tonal-ranges.guide")
        .ok_or("Dynamic Range branch did not prepare its brightness guide")?;
    let Operation::ToneMask { guide, .. } =
        branch.graph.nodes[branch.graph.outputs[mask].producer].operation
    else {
        return Err("Dynamic Range guide has an unexpected producer".into());
    };
    // Same-grid Whole gains also read padding anchors beside the active crop.
    // Keep those cells available when only the RGB backing becomes regional.
    let guide_bounds = if overview.working_scale == settings.working_scale {
        [0, 0, branch.rgb_extent[0], branch.rgb_extent[1]]
    } else {
        branch.rgb_bounds
    };
    let sampling = tone_equalizer::GuideSampling::new(
        branch.rgb_extent,
        guide_bounds,
        guide_mapping(recipe.rgb_to_native, branch.rgb_to_native)?,
        guide.cell_step(),
        guide.cell_step(),
    )?;
    let mapping = recipe
        .graph
        .append(branch.graph, "tonal-ranges-application");
    let mask = mapping[mask];
    recipe.graph.retain_for_session(mask);
    recipe.graph.prioritize_for_session(mask);
    Ok((mask, sampling))
}

/// Compose the input's native sample map with the inverse guide sample map.
fn guide_mapping(input: [[f64; 3]; 2], guide: [[f64; 3]; 2]) -> Result<[[f32; 4]; 2]> {
    let [[a, b, tx], [c, d, ty]] = guide;
    let determinant = a * d - b * c;
    if !determinant.is_finite() || determinant == 0. {
        return Err("adaptive guide sampling is not invertible".into());
    }
    let inverse = [
        [d / determinant, -b / determinant],
        [-c / determinant, a / determinant],
    ];
    Ok(std::array::from_fn(|axis| {
        let [x, y] = inverse[axis];
        [
            (x * input[0][0] + y * input[1][0]) as f32,
            (x * input[0][1] + y * input[1][1]) as f32,
            (x * (input[0][2] - tx) + y * (input[1][2] - ty)) as f32,
            0.,
        ]
    }))
}

/// Both guides use the same pre-curve signal. Fitting and application select
/// independent grids without changing authored processing or applying tone twice.
#[allow(clippy::too_many_arguments)]
fn adaptive_rgb(
    engine: &Engine,
    source: super::RecipeInput<'_>,
    settings: &Settings,
    space: ColorSpace,
    white: f32,
    dehaze_basis: Option<crate::nodes::finish::dehaze::Basis>,
    scale: Scale,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let mut overview = settings.clone();
    overview.photographic.base_response = Default::default();
    overview.dehaze_estimate = false;
    let output = if settings.dehaze_active() {
        "dehaze.rgb"
    } else {
        "linear.edits-input"
    };
    let (overview, mut branch) =
        whole_photo_rgb(engine, source, overview, scale, profile, execution)?;
    build(
        engine,
        source,
        &overview,
        &mut branch,
        &[output],
        Context {
            dehaze_basis,
            adaptive: None,
            space,
            white,
            guide_white: white,
            scale: scale.factor(),
        },
        |_| Err("tone pivot is not required for guide preparation".into()),
        profile,
        execution,
    )?;

    Ok(branch)
}

#[allow(clippy::too_many_arguments)]
fn adaptive_application_guide(
    engine: &Engine,
    source: super::RecipeInput<'_>,
    settings: &Settings,
    recipe: &mut BuiltRecipe,
    space: ColorSpace,
    white: f32,
    dehaze_basis: Option<crate::nodes::finish::dehaze::Basis>,
    params: crate::nodes::finish::adaptive_tone::Params,
    scale: Scale,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<(
    usize,
    crate::nodes::finish::adaptive_tone::Params,
    [[f64; 3]; 2],
)> {
    let mut branch = adaptive_rgb(
        engine,
        source,
        settings,
        space,
        white,
        dehaze_basis,
        scale,
        profile,
        execution,
    )?;
    let guide = tone_equalizer::Guide::new(
        branch.rgb_extent,
        tone_equalizer::radius_for_scale(scale.factor()),
        space,
    )?
    .with_white(white);
    // Keep the ordinary guide's direct RGB parent. An inspection alias before
    // the filter would prevent numerical reuse of a matching Fast overview guide.
    let mask = branch.graph.add(
        Operation::AdaptiveToneGuide { guide, space },
        &[branch.final_id],
    )?;
    mask_inspection(&mut branch.graph, mask, guide, "base-response.mask", names)?;
    let geometry = params.with_geometry(branch.rgb_extent, branch.rgb_bounds, scale.factor())?;
    let rgb_mapping = branch.rgb_to_native;
    let mapping = recipe
        .graph
        .append(branch.graph, "base-response-application");
    let mask = mapping[mask];
    recipe.graph.retain_for_session(mask);
    recipe.graph.prioritize_for_session(mask);
    Ok((mask, geometry, rgb_mapping))
}

#[allow(clippy::too_many_arguments)]
fn adaptive_analysis(
    engine: &Engine,
    source: super::RecipeInput<'_>,
    settings: &Settings,
    recipe: &mut BuiltRecipe,
    space: ColorSpace,
    white: f32,
    dehaze_basis: Option<crate::nodes::finish::dehaze::Basis>,
    params: crate::nodes::finish::adaptive_tone::Params,
    analysis_scale: Scale,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<(
    usize,
    usize,
    crate::nodes::finish::adaptive_tone::Params,
    [[f64; 3]; 2],
)> {
    let mut branch = adaptive_rgb(
        engine,
        source,
        settings,
        space,
        white,
        dehaze_basis,
        analysis_scale,
        profile,
        execution,
    )?;

    let guide_rgb = branch.final_id;
    alias(&mut branch, space, "base-response.analysis")?;
    let rgb = branch.final_id;
    let guide = tone_equalizer::Guide::new(
        branch.rgb_extent,
        tone_equalizer::radius_for_scale(analysis_scale.factor()),
        space,
    )?
    .with_white(white);
    let mask = branch
        .graph
        .add(Operation::AdaptiveToneGuide { guide, space }, &[guide_rgb])?;
    let mask = branch.graph.add(
        Operation::ImageAlias {
            dehaze_basis: None,
            data: DataType::BrightnessGuide,
            output: "base-response.analysis-guide",
        },
        &[mask],
    )?;
    mask_inspection(
        &mut branch.graph,
        mask,
        guide,
        "base-response.analysis-mask",
        names,
    )?;
    let analysis = params.with_geometry(
        branch.rgb_extent,
        branch.rgb_bounds,
        analysis_scale.factor(),
    )?;
    let rgb_mapping = branch.rgb_to_native;
    let mapping = recipe.graph.append(branch.graph, "base-response-analysis");
    for id in [mapping[rgb], mapping[mask]] {
        recipe.graph.retain_for_session(id);
        recipe.graph.prioritize_for_session(id);
    }
    Ok((mapping[rgb], mapping[mask], analysis, rgb_mapping))
}

#[allow(clippy::too_many_arguments)]
fn haze_analysis(
    engine: &Engine,
    source: super::RecipeInput<'_>,
    settings: &Settings,
    recipe: &mut BuiltRecipe,
    space: ColorSpace,
    white: f32,
    basis: crate::nodes::finish::dehaze::Basis,
    analysis_scale: Scale,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<usize> {
    let mut overview = settings.clone();
    overview.dehaze = Default::default();
    overview.photographic.base_response = Default::default();
    let mut branch =
        if analysis_scale != settings.working_scale || settings.preview_region.is_some() {
            // The adapter supplies analysis sampling in physical sensor units.
            // Stop before shared edits so estimating the color never feeds back into itself.
            let (_, mut branch) =
                whole_photo_rgb(engine, source, overview, analysis_scale, profile, execution)?;
            white_balance(&mut branch, settings, space)?;
            Some(branch)
        } else {
            None
        };
    let target = branch.as_mut().unwrap_or(recipe);
    let analysis = target.graph.add(
        Operation::DehazeEstimate {
            params: crate::nodes::finish::dehaze::Estimate::new(
                target.rgb_extent,
                basis,
                white,
                settings.is_monochrome(),
                target.params.crop,
                settings.processing_quality,
            )
            .with_color_estimation(settings.dehaze.shadow.is_none()),
            space,
        },
        &[target.final_id],
    )?;
    if let Some(branch) = branch {
        Ok(recipe.graph.append(branch.graph, "dehaze-overview")[analysis])
    } else {
        Ok(analysis)
    }
}

fn white_balance(recipe: &mut BuiltRecipe, settings: &Settings, space: ColorSpace) -> Result<()> {
    if !settings.is_monochrome() && !settings.white_balance_bias.is_neutral() {
        recipe.final_id = recipe.graph.add(
            Operation::LinearWhiteBalance {
                rows: settings.white_balance_bias.rows(space),
                space,
            },
            &[recipe.final_id],
        )?;
    }
    Ok(())
}

fn color_guide(
    engine: &Engine,
    source: super::RecipeInput<'_>,
    settings: &Settings,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let mut color = settings.clone();
    color.rendering = Rendering::Color;
    let caps = crate::render::ControlCapabilities::from_metadata(source.metadata())?;
    // The neutral color look is independent of the authored rendering and mono mix:
    // Standard where the camera offers it, otherwise the recorded mode.
    color.color_mode = if caps
        .color_mode_presets(source.metadata())
        .contains(&"Standard")
    {
        "Standard".into()
    } else {
        crate::render::AS_SHOT.into()
    };
    let mut color = color.with_camera_choices(source.metadata()).into_owned();
    color.processing_sections.noise = true;
    color.processing_sections.detail = true;
    color.sharpen_amount = Some(0.);
    // Keep the main recipe's spatial support, while requesting the technical
    // boundary before shared edits. Universal also omits native creative sharpening.
    if !settings.photographic.sharpening.is_active() {
        color.photographic.sharpening.amount = 0.;
    }
    color.photographic.sharpening.method = crate::render::PhotographicMethod::Universal;
    if settings.is_monochrome() {
        color.preview_region = None;
    }
    let mut noise = caps.default_noise_controls();
    noise.detail_enabled = false;
    color.reconstruction = Some(noise);
    color.color_dq_gain = None;
    caps.normalize(&mut color);
    super::build_camera(engine, source, &color, &["linear.base"], profile, execution)
}

/// Prioritize costly linear checkpoints after the complete graph has been composed.
pub(super) fn prioritize_cache(graph: &mut Graph) {
    // Native Quattro frames cannot retain every RGB checkpoint in the session
    // budget. Prefer the HDR/Local Contrast boundary and the independent color
    // classifier over cheaper later sharpening and tone intermediates.
    let boundaries: Vec<_> = graph
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.operation,
                Operation::TonalRanges { .. }
                    | Operation::LinearLocalContrast { .. }
                    | Operation::PerceptualGrade { .. }
            )
        })
        .flat_map(|node| node.inputs.iter().copied())
        .collect();
    let editor_inputs: Vec<_> = graph
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.operation,
                Operation::LinearColorEditor { .. } | Operation::AdvancedColor { .. }
            )
        })
        .flat_map(|node| node.inputs.iter().copied())
        .collect();
    for input in boundaries.into_iter().chain(editor_inputs) {
        graph.prioritize_for_session(input);
    }
}

/// Native support for shared spatial edits, in physical image coordinates.
pub(super) fn support(settings: &Settings) -> u32 {
    let sharp = if settings.sharpening_enabled && settings.photographic.sharpening.is_active() {
        settings.photographic.sharpening.support(1.)
    } else {
        0
    };
    let local = if settings.tone_enabled && settings.photographic.local_contrast != 0. {
        local_contrast::SUPPORT
    } else {
        0
    };
    let editor = if settings.color_editor_enabled
        && settings
            .photographic
            .color_editor
            .basic_active(settings.is_monochrome())
    {
        color_editor::NATIVE_RADIUS
    } else {
        0
    };
    let shadows = if !settings.is_monochrome() && !settings.neutralize_shadows.is_neutral() {
        crate::nodes::finish::shadow_neutralize::NATIVE_RADIUS
    } else {
        0
    };
    let adaptive = if settings.adaptive_tone_active() {
        if settings.preview_region.is_some() {
            crate::nodes::finish::adaptive_tone::REGIONAL_SUPPORT
        } else {
            crate::nodes::finish::adaptive_tone::support()
        }
    } else {
        0
    };
    let tonal = if settings.tonal_ranges.is_neutral() {
        0
    } else if settings.preview_region.is_some() {
        2
    } else {
        tone_equalizer::support()
    };
    sharp + local + editor + shadows + adaptive + tonal
}

pub(super) fn bake(
    engine: &Engine,
    source: super::RecipeInput<'_>,
    authored: &Settings,
    recipe: &mut BuiltRecipe,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<()> {
    let mut settings = authored.resolved_exposure(source.metadata()).into_owned();
    settings.working_scale = Scale::NATIVE;
    settings.preview_region = None;
    settings.color_range_preview = None;
    settings.output_size = None;
    let effective = settings.processing_settings(source.metadata().processing_family()?);
    let active = effective.active_tone_settings();
    if !baked_active(&active) {
        return Ok(());
    }
    let camera = super::finishing::camera_settings(&active);
    let reference = super::build_camera(
        engine,
        source,
        &camera,
        &["linear.edits"],
        profile,
        execution,
    )?;
    if reference.rgb_extent != recipe.rgb_extent
        || reference.params.source[..2] != recipe.params.source[..2]
        || reference.params.output[..2] != recipe.params.output[..2]
        || reference.params.crop != recipe.params.crop
    {
        return Err("DNG linear edit framing mismatch".into());
    }
    let before = reference
        .graph
        .outputs
        .iter()
        .position(|o| o.name == "linear.edits-input")
        .ok_or("missing linear edit input")?;
    let after = reference.final_id;
    let DataType::LinearRgb(space) = reference.graph.outputs[after].data else {
        return Err("linear edit reference must be RGB".into());
    };
    let data = recipe.graph.outputs[recipe.final_id].data;
    let mapping = recipe.graph.append(reference.graph, "dng-edit-reference");
    recipe.final_id = if active.linear_chromatic_active() || active.dehaze_active() {
        let statistics =
            super::estimated_white_balance(engine, source.original()?, &active, profile)?;
        let appearance = super::prepare_dng_appearance(source.metadata(), &active, statistics)?;
        let matrix = if active.is_monochrome() {
            appearance.technical_from_monochrome()?
        } else {
            appearance.technical_from_native()?
        };
        let rows = std::array::from_fn(|r| {
            std::array::from_fn(|c| {
                if c < 3 {
                    matrix.0[r * 3 + c] as f32
                } else {
                    0.
                }
            })
        });
        recipe.graph.add(
            Operation::LinearBasis { rows, space, data },
            &[mapping[after]],
        )?
    } else {
        recipe.graph.add(
            Operation::TransferLinearGain { data, space },
            &[recipe.final_id, mapping[before], mapping[after]],
        )?
    };
    execution.check()
}

/// Expanded masks belong to inspection, while processing retains the compact guide.
fn mask_inspection(
    graph: &mut Graph,
    input: usize,
    guide: tone_equalizer::Guide,
    output: &'static str,
    names: &[&str],
) -> Result<()> {
    if names.contains(&output) {
        graph.add(Operation::ExpandBrightnessGuide { guide, output }, &[input])?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "linear_edits_test.rs"]
mod tests;

#[cfg(test)]
#[path = "adaptive_views_test.rs"]
mod view_tests;
