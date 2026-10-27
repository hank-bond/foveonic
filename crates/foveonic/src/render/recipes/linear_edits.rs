//! Composes shared pre-curve edits and DNG baking using camera-supplied signal context.

use super::{requested, BuiltRecipe};
use crate::{
    color_space::ColorSpace,
    graph::{DataType, Graph, Operation},
    interactive::Execution,
    nodes::finish::{
        color_editor,
        photographic::{local_contrast, Params},
        tone_equalizer,
    },
    profile::Profile,
    render::{Engine, PreviewQuality, Rendering, Settings},
    source::{Result, Source},
};

/// Camera recipes supply code units and geometry. Numerical nodes do not identify cameras.
#[derive(Clone, Copy)]
pub(super) struct Context {
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
    const CODE_TABLE_LENGTH: usize = 1 << 16;
    let curve: Vec<f32> = (0..CODE_TABLE_LENGTH)
        .map(|code| response(code as f64) as f32)
        .collect();
    tone_equalizer::pivot_with_white(&curve, 0., white)
}

pub(super) fn active(settings: &Settings) -> bool {
    settings.color_editor_active()
        || (settings.tone_enabled
            && (!settings.tonal_ranges.is_neutral() || settings.photographic.local_contrast != 0.))
        || (settings.sharpening_enabled && settings.photographic.sharpening.is_active())
}

pub(super) fn requested_output(name: &str) -> bool {
    matches!(
        name,
        "linear.base"
            | "linear.edits-input"
            | "linear.edits"
            | "color-editor.input"
            | "color-editor.guide"
            | "color-editor.rgb"
            | "tonal-ranges.mask"
            | "tonal-ranges.rgb"
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
            data: DataType::LinearRgb(space),
            output,
        },
        &[recipe.final_id],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    recipe: &mut BuiltRecipe,
    names: &[&str],
    context: Context,
    pivot: impl FnOnce(&mut Profile) -> Result<f32>,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<()> {
    let Context {
        space,
        white,
        guide_white,
        scale,
    } = context;
    let extent = recipe.rgb_extent;
    alias(recipe, space, "linear.edits-input")?;
    if requested(&recipe.graph, names) {
        return Ok(());
    }
    if settings.color_editor_active() {
        let guide = color_guide(engine, source, settings, profile, execution)?;
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
        let guide_final = guide.final_id;
        let mapping = recipe.graph.append(guide.graph, "color-editor-guide");
        let guide_id = recipe.graph.add(
            Operation::ImageAlias {
                data: DataType::LinearRgb(space),
                output: "color-editor.guide",
            },
            &[mapping[guide_final]],
        )?;
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
        );
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
        let guide =
            tone_equalizer::Guide::new(extent, tone_equalizer::radius_for_scale(scale), space)?
                .with_white(white);
        let input = recipe.final_id;
        recipe.graph.retain_for_session(input);
        let mask = recipe
            .graph
            .add(Operation::ToneMask { guide, space }, &[input])?;
        recipe.graph.retain_for_session(mask);
        if requested(&recipe.graph, names) {
            return Ok(());
        }
        recipe.final_id = recipe.graph.add(
            Operation::TonalRanges {
                controls: settings.tonal_ranges,
                pivot: pivot(profile)?,
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
    alias(recipe, space, "linear.edits")
}

fn color_guide(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let mut color = settings.clone();
    color.rendering = Rendering::Color;
    let caps = crate::render::ControlCapabilities::from_metadata(&source.metadata)?;
    color.color_mode = caps.default_color_mode(&source.metadata)?;
    // The neutral color look is independent of the authored rendering and mono mix.
    if caps
        .color_mode_presets(&source.metadata)
        .contains(&"Standard")
    {
        color.color_mode = "Standard".into();
    }
    if color.white_balance.is_empty() {
        color.white_balance = caps.default_white_balance(&source.metadata)?;
    }
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
        color.preview_quality = PreviewQuality::Full;
        color.preview_region = None;
    } else if color.preview_quality == PreviewQuality::Draft {
        color.preview_quality = PreviewQuality::Fast;
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
                Operation::TonalRanges { .. } | Operation::LinearLocalContrast { .. }
            )
        })
        .flat_map(|node| node.inputs.iter().copied())
        .collect();
    let editor_inputs: Vec<_> = graph
        .nodes
        .iter()
        .filter(|node| matches!(node.operation, Operation::LinearColorEditor { .. }))
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
    let editor = if settings.color_editor_active() {
        color_editor::NATIVE_RADIUS
    } else {
        0
    };
    sharp + local + editor
}

pub(super) fn bake(
    engine: &Engine,
    source: &Source,
    authored: &Settings,
    recipe: &mut BuiltRecipe,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<()> {
    let mut settings = authored.clone();
    settings.preview_quality = PreviewQuality::Full;
    settings.preview_region = None;
    settings.output_size = None;
    let effective = settings.processing_settings();
    let active = effective.active_tone_settings();
    if !self::active(&active) {
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
    recipe.final_id = if active.color_editor_chromatic_active() {
        let statistics = super::estimated_white_balance(engine, source, &active, profile)?;
        let appearance = super::prepare_dng_appearance(&source.metadata, &active, statistics)?;
        let matrix = appearance.technical_from_native()?;
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

#[cfg(test)]
#[path = "linear_edits_test.rs"]
mod tests;
