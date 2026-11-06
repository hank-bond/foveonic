//! Applies the camera tone curve and saturation after shared linear photographic edits.

use super::super::linear_edits::{self, Context};
use super::super::{requested, BuiltRecipe};
use super::technical;
use crate::{
    color_space::ColorSpace,
    graph::Operation,
    interactive::Execution,
    profile::Profile,
    render::{Engine, Settings},
    source::Result,
};

// Camera tone tables use 4095 as nominal white. The sensitivity model uses
// the reconstruction's Q12 signal unit, preserving its established response.
const LINEAR_CODE_WHITE: f32 = 4095.;
const CLASSIFIER_CODE_UNIT: f32 = 4096.;

pub(in crate::render::recipes) fn build(
    engine: &Engine,
    source: super::super::RecipeInput<'_>,
    settings: &Settings,
    names: &[&str],
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    let (mut recipe, mode, grid) = match technical::build(
        engine,
        source,
        settings,
        names,
        Some(&settings.color_mode),
        profile,
        execution,
    )? {
        technical::Prepared::Requested(recipe) => return Ok(recipe),
        technical::Prepared::Linear(recipe, mode, grid) => (recipe, mode, grid),
    };
    let space = ColorSpace::from_metadata(source.metadata())?;
    linear_edits::alias(&mut recipe, space, "linear.base")?;
    if requested(&recipe.graph, names) {
        return Ok(recipe);
    }
    let adaptive = if settings.adaptive_tone_active() {
        let curve = engine.prepare_tone(
            source.metadata(),
            &mode,
            Default::default(),
            settings.is_monochrome(),
        )?;
        Some(linear_edits::Adaptive::new(
            settings,
            &recipe,
            space,
            LINEAR_CODE_WHITE,
            curve,
            false,
        )?)
    } else {
        None
    };
    let mut prepared_curve = None;
    let prepare_curve = |profile: &mut Profile| {
        profile.measure("prepare tone curve", || {
            engine.prepare_tone(
                source.metadata(),
                &mode,
                settings.tone,
                settings.is_monochrome(),
            )
        })
    };
    let dehaze_basis = if linear_edits::needs_dehaze(settings, names) {
        use crate::nodes::finish::dehaze::Basis;
        Some(if settings.is_monochrome() {
            Basis::monochrome()
        } else {
            let rows = crate::nodes::sensor::calibration::color(
                source.metadata(),
                &settings.white_balance,
                &mode,
            )?
            .matrix;
            Basis::new(
                space,
                std::array::from_fn(|i| f64::from(rows[i / 3][i % 3])),
                [0.299, 0.587, 0.114],
            )?
        })
    } else {
        None
    };
    let context = Context {
        dehaze_basis,
        adaptive,
        space,
        white: LINEAR_CODE_WHITE,
        guide_white: CLASSIFIER_CODE_UNIT,
        scale: if settings.preview_region.is_some() {
            1.
        } else {
            settings.working_scale.factor()
        },
        analysis_grid: crate::render::AnalysisGrid::new(
            settings.working_scale,
            if settings.is_monochrome() {
                crate::render::Scale::NATIVE
            } else {
                crate::render::Scale::new(2)?
            },
            crate::render::Scale::new(2)?,
        ),
    };
    linear_edits::build(
        engine,
        source,
        settings,
        &mut recipe,
        names,
        context,
        |profile| {
            let curve = prepare_curve(profile)?;
            let pivot = crate::nodes::finish::tone_equalizer::pivot(&curve, settings.exposure)?;
            prepared_curve = Some(curve);
            Ok(pivot)
        },
        profile,
        execution,
    )?;
    if requested(&recipe.graph, names) {
        return Ok(recipe);
    }
    // Internal guides keep the sensor working grid. Apply the photographic
    // frame only after shared spatial edits and before the pointwise response.
    if let Some(grid) = &grid {
        grid.frame(&mut recipe, space)?;
    }
    let curve = match prepared_curve {
        Some(curve) => curve,
        None => prepare_curve(profile)?,
    };
    recipe.final_id = recipe.graph.add(
        Operation::Tone {
            curve,
            exposure: settings.exposure.exp2(),
            space,
        },
        &[recipe.final_id],
    )?;
    execution.check()?;
    if requested(&recipe.graph, names) {
        return Ok(recipe);
    }
    if !settings.is_monochrome() && settings.reference_saturation() != 1. {
        let dimensions = [recipe.params.source[0], recipe.params.source[1], 0, 0];
        recipe.final_id = recipe.graph.add(
            Operation::Saturation {
                params: super::super::saturation(
                    dimensions,
                    settings.reference_saturation(),
                    settings.prevent_color_clipping,
                ),
                space,
            },
            &[recipe.final_id],
        )?;
        execution.check()?;
    }
    Ok(recipe)
}
