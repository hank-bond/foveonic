//! Selects authored photographic and DNG-base graphs before shared execution.

use super::{Engine, Settings};
use crate::{
    graph::Graph,
    interactive::Execution,
    nodes::sensor::calibration,
    profile::Profile,
    source::{Result, Source},
};

mod dng_base;
pub(super) mod photographic;
mod shared;

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
        let built = match recipe {
            Recipe::Photographic(_) => {
                photographic::build(self, source, settings, names, profile, execution)?
            }
            Recipe::DngBase => dng_base::build(self, source, settings, profile, execution)?,
        };
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
