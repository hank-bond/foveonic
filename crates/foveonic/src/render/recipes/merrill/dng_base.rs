//! Builds a fresh linear technical base while leaving selected appearance to DNG metadata.

use super::super::BuiltRecipe;
use super::technical;
use crate::{
    interactive::Execution,
    nodes::color::color_mode,
    profile::Profile,
    render::{Engine, Settings},
    source::{Result, Source},
};

pub(in crate::render::recipes) fn build(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    match technical::build(
        engine,
        source,
        settings,
        &["final"],
        None,
        profile,
        execution,
    )? {
        technical::Prepared::Requested(recipe) | technical::Prepared::Linear(recipe, _) => {
            Ok(recipe)
        }
    }
}

pub(super) fn unselected_mode() -> color_mode::Settings {
    // Technical corrections need working RGB without baking the selected look.
    color_mode::Settings {
        tone_index: 0,
        contrast: 0.,
        compensation: None,
    }
}
