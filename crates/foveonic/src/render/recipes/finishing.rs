//! Composes shared photographic edits after each camera's base appearance.

use super::{BuiltRecipe, Recipe};
use crate::{
    color_space::ColorSpace,
    graph::{DataType, Operation},
    interactive::Execution,
    nodes::finish::{
        photographic::{local_contrast, Method, Params},
        tone_equalizer,
    },
    profile::Profile,
    render::{Engine, Settings},
    source::{ProcessingFamily, Result, Source},
};

/// Camera adapters retain their existing RAW exposure and base appearance.
/// Universal contrast/saturation are evaluated once after that boundary.
pub(super) fn camera_settings(settings: &Settings) -> Settings {
    let mut base = settings.clone();
    if base.photographic.tone_method == Method::Universal {
        base.tone.contrast = 0.;
        base.saturation = 1.;
    }
    base
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    mut recipe: BuiltRecipe,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    if super::requested(&recipe.graph, names) {
        return Ok(recipe);
    }
    let DataType::ToneMappedRgb(space) = recipe.graph.outputs[recipe.final_id].data else {
        // Early inspection deliberately stops before the photographic boundary.
        return Ok(recipe);
    };
    let family = source.metadata.processing_family()?;
    let extent = if family == ProcessingFamily::Quattro {
        [recipe.params.output[0], recipe.params.output[1]]
    } else {
        [recipe.params.source[0], recipe.params.source[1]]
    };
    if settings.photographic != Default::default()
        || !settings.tonal_ranges.is_neutral()
        || names.iter().any(|n| n.starts_with("finish."))
    {
        recipe.final_id = recipe.graph.add(
            Operation::ImageAlias {
                data: DataType::ToneMappedRgb(space),
                output: "finish.base",
            },
            &[recipe.final_id],
        )?;
    }
    let mut scale = if settings.preview_quality.is_reduced() && !settings.is_monochrome() {
        0.5
    } else {
        1.
    };
    if family == ProcessingFamily::Quattro
        && settings.preview_region.is_none()
        && settings.output_size.is_some()
    {
        let native = crate::nodes::sensor::calibration::encode(&source.metadata, space)?;
        scale = (extent[0] as f32 / native.output[0] as f32)
            .min(extent[1] as f32 / native.output[1] as f32);
    }
    if settings.is_monochrome() && settings.photographic.sensitivity_active() {
        let guide = color_guide(engine, source, settings, profile, execution)?;
        let DataType::ToneMappedRgb(guide_space) = guide.graph.outputs[guide.final_id].data else {
            return Err("invalid monochrome guide color space".into());
        };
        if recipe.params.output[..2] != guide.params.output[..2]
            || recipe.params.crop != guide.params.crop
        {
            return Err("monochrome guide framing mismatch".into());
        }
        let mut guide = guide;
        let guide_id = guide.graph.add(
            Operation::ImageAlias {
                data: DataType::ToneMappedRgb(guide_space),
                output: "finish.color-guide",
            },
            &[guide.final_id],
        )?;
        let mapping = recipe.graph.append(guide.graph, "monochrome-guide");
        let guide_id = mapping[guide_id];
        let p = Params::sensitivity(
            extent,
            space,
            guide_space,
            settings.photographic.monochrome_sensitivity,
        );
        recipe.graph.retain_for_session(guide_id);
        recipe.graph.retain_for_session(recipe.final_id);
        recipe.final_id = recipe.graph.add(
            Operation::MonochromeSensitivity {
                params: p,
                space,
                guide_space,
            },
            &[recipe.final_id, guide_id],
        )?;
    }
    if settings.photographic.local_contrast != 0. {
        let p = Params::local_contrast(extent, space, settings.photographic.local_contrast, scale);
        recipe.graph.retain_for_session(recipe.final_id);
        recipe.final_id = recipe.graph.add(
            Operation::LocalContrast { params: p, space },
            &[recipe.final_id],
        )?;
    }
    // Merrill's already-reviewed reference HDR retains its original pre-curve
    // location. Other families share the normalized photographic RGB adapter.
    if family != ProcessingFamily::Merrill
        && (!settings.tonal_ranges.is_neutral()
            || names.contains(&"finish.hdr-mask")
            || names.contains(&"finish.hdr"))
    {
        let guide =
            tone_equalizer::Guide::photographic(extent, tone_equalizer::radius(scale < 1.), space)?;
        let input = recipe.final_id;
        recipe.graph.retain_for_session(input);
        let mask = recipe
            .graph
            .add(Operation::PhotographicHdrMask { guide, space }, &[input])?;
        recipe.graph.retain_for_session(mask);
        recipe.final_id = recipe.graph.add(
            Operation::PhotographicHdr {
                controls: settings.tonal_ranges,
                space,
            },
            &[input, mask],
        )?;
    }
    if settings.photographic.tone_method == Method::Universal
        && (settings.tone.contrast != 0.
            || (!settings.is_monochrome() && settings.saturation != 1.))
    {
        let p = Params::tone(
            extent,
            space,
            settings.tone.contrast,
            if settings.is_monochrome() {
                1.
            } else {
                settings.saturation
            },
        );
        recipe.graph.retain_for_session(recipe.final_id);
        recipe.final_id = recipe.graph.add(
            Operation::PhotographicTone { params: p, space },
            &[recipe.final_id],
        )?;
    }
    let sharpening = settings.photographic.sharpening;
    if settings.sharpening_enabled && sharpening.is_active() {
        let p = Params::sharpening(extent, space, sharpening, scale);
        recipe.graph.retain_for_session(recipe.final_id);
        recipe.final_id = recipe.graph.add(
            Operation::UniversalSharpen { params: p, space },
            &[recipe.final_id],
        )?;
    }
    execution.check()?;
    Ok(recipe)
}

/// Reuse an ordinary color recipe for subject hue selection. Its camera mode,
/// exposure and creative controls are independent of the authored mono mixture.
fn color_guide(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let mut color = settings.clone();
    color.rendering = crate::render::Rendering::Color;
    color.photographic = Default::default();
    color.sharpen_amount = Some(0.);
    color.exposure = 0.;
    color.tone = Default::default();
    color.saturation = 1.;
    color.tonal_ranges = Default::default();
    // The color guide has its own stable branch and ignores the mono mix.
    // Use the ordinary neutral camera mode, preserving selected WB.
    color.color_mode = source
        .metadata
        .color_mode_presets()
        .into_iter()
        .find(|v| *v == "Standard")
        .or_else(|| source.metadata.color_mode_presets().into_iter().next())
        .ok_or("no color mode for monochrome sensitivity")?
        .to_owned();
    color.preview_quality = crate::render::PreviewQuality::Full;
    color.preview_region = None;
    color.output_size = settings.output_size;
    if color.white_balance.is_empty() {
        color.white_balance = crate::render::ControlCapabilities::from_metadata(&source.metadata)?
            .default_white_balance(&source.metadata)?;
    }
    super::build_camera(
        engine,
        source,
        &color,
        Recipe::Photographic(&["final"]),
        profile,
        execution,
    )
}

/// Native support for shared spatial edits, in physical image coordinates.
pub(super) fn support(settings: &Settings) -> u32 {
    let sharp = if settings.sharpening_enabled && settings.photographic.sharpening.is_active() {
        settings.photographic.sharpening.support(1.)
    } else {
        0
    };
    let local = if settings.photographic.local_contrast != 0. {
        local_contrast::SUPPORT
    } else {
        0
    };
    sharp + local
}

pub(super) fn expand_region(
    source: &Source,
    settings: &mut Settings,
    names: &[&str],
) -> Result<Option<[u32; 4]>> {
    if source.metadata.processing_family()? != ProcessingFamily::Quattro || names != ["final"] {
        return Ok(None);
    }
    let Some(rect) = settings.preview_region else {
        return Ok(None);
    };
    let margin = support(settings)
        + if !settings.tonal_ranges.is_neutral() {
            tone_equalizer::support()
        } else {
            0
        };
    if margin == 0 {
        return Ok(None);
    }
    let encode = crate::nodes::sensor::calibration::encode(&source.metadata, ColorSpace::Srgb)?;
    let alignment = tone_equalizer::GUIDE_ALIGNMENT;
    let expanded = [
        rect[0].saturating_sub(margin) / alignment * alignment,
        rect[1].saturating_sub(margin) / alignment * alignment,
        rect[2]
            .saturating_add(margin)
            .div_ceil(alignment)
            .saturating_mul(alignment)
            .min(encode.output[0]),
        rect[3]
            .saturating_add(margin)
            .div_ceil(alignment)
            .saturating_mul(alignment)
            .min(encode.output[1]),
    ];
    settings.preview_region = Some(expanded);
    Ok(Some([
        rect[0] - expanded[0],
        rect[1] - expanded[1],
        rect[2] - expanded[0],
        rect[3] - expanded[1],
    ]))
}
pub(super) fn crop_region(recipe: &mut BuiltRecipe, rect: Option<[u32; 4]>) -> Result<()> {
    let Some(rect) = rect else {
        return Ok(());
    };
    let source = [recipe.params.output[0], recipe.params.output[1]];
    let data = recipe.graph.outputs[recipe.final_id].data;
    recipe.final_id = recipe.graph.add(
        Operation::ImageRegion {
            prepared: crate::nodes::finish::image_region::Prepared::new(source, rect, 3)?,
            data,
            output: "finish.region",
        },
        &[recipe.final_id],
    )?;
    let extent = [rect[2] - rect[0], rect[3] - rect[1]];
    recipe.params.output[..2].copy_from_slice(&extent);
    recipe.params.crop = [0, 0, extent[0], extent[1]];
    Ok(())
}

#[cfg(test)]
#[path = "finishing_test.rs"]
mod tests;
