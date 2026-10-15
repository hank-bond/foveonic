//! Builds a fresh linear technical base while leaving selected appearance to DNG metadata.

use super::{shared, BuiltRecipe};
use crate::{
    interactive::Execution,
    nodes::color::color_mode,
    profile::Profile,
    render::{Engine, Settings},
    source::{Result, Source},
};

pub(super) fn build(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<BuiltRecipe> {
    match engine.build_technical(source, settings, &["final"], None, profile, execution)? {
        shared::Prepared::Requested(recipe) | shared::Prepared::Linear(recipe, _) => Ok(recipe),
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
