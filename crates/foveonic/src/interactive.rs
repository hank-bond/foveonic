//! Reuses unchanged inputs for the current edit while keeping earlier images safe.

use crate::{
    graph::{DataType, Graph},
    recipe::Key,
    render::{Settings, Value},
    source::Source,
};
use std::{collections::HashMap, sync::Arc};

/// Keep the original photo and the inputs needed to restart the active edit.
/// Retaining every stage would spend memory without helping that edit.
pub struct RenderSession {
    pub(crate) source: Source,
    pub(crate) cache: Cache,
    pub(crate) last_settings: Option<Settings>,
}
impl RenderSession {
    pub fn new(source: Source) -> Self {
        Self {
            source,
            cache: Cache::default(),
            last_settings: None,
        }
    }
    /// Exports need the original samples, not the cached result of a slider edit.
    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn metadata(&self) -> &Arc<crate::source::Metadata> {
        &self.source.metadata
    }
    pub fn cached_bytes(&self) -> u64 {
        self.cache.bytes()
    }
    /// These identities help replace pending requests. Processing stages still
    /// decide which parameter changes invalidate their own results.
    pub fn changed_parameters(&self, settings: &Settings) -> Vec<&'static str> {
        self.last_settings
            .as_ref()
            .map_or_else(Vec::new, |old| old.changed_parameters(settings))
    }
}
impl Settings {
    pub fn changed_parameters(&self, next: &Self) -> Vec<&'static str> {
        let mut changes = Vec::new();
        macro_rules! changed {
            ($test:expr,$name:expr) => {
                if $test {
                    changes.push($name);
                }
            };
        }
        changed!(self.white_balance != next.white_balance, "white-balance");
        changed!(self.color_mode != next.color_mode, "color-mode");
        changed!(self.repair != next.repair, "repair");
        changed!(
            self.highlight_restoration != next.highlight_restoration,
            "highlight-restoration"
        );
        changed!(
            self.highlight_threshold_ev.map(f32::to_bits)
                != next.highlight_threshold_ev.map(f32::to_bits),
            "highlight-threshold"
        );
        changed!(self.despeckle != next.despeckle, "despeckle");
        changed!(self.row_correction != next.row_correction, "row-correction");
        changed!(
            self.color_dq_gain.map(f32::to_bits) != next.color_dq_gain.map(f32::to_bits),
            "color-dq"
        );
        changed!(
            self.sharpen_amount.to_bits() != next.sharpen_amount.to_bits(),
            "sharpen"
        );
        changed!(
            self.exposure.to_bits() != next.exposure.to_bits(),
            "exposure"
        );
        changed!(
            self.saturation.to_bits() != next.saturation.to_bits(),
            "saturation"
        );
        changed!(
            self.tone.contrast.to_bits() != next.tone.contrast.to_bits(),
            "contrast"
        );
        changed!(
            self.tone.shadows.to_bits() != next.tone.shadows.to_bits(),
            "shadows"
        );
        changed!(
            self.tone.highlights.to_bits() != next.tone.highlights.to_bits(),
            "highlights"
        );
        changed!(self.output_size != next.output_size, "output-size");
        match (self.reconstruction, next.reconstruction) {
            (Some(a), Some(b)) => {
                changed!(a.detail.to_bits() != b.detail.to_bits(), "detail");
                changed!(a.chroma != b.chroma, "chroma-noise");
                changed!(a.luminance != b.luminance, "luma-noise");
                changed!(a.banding != b.banding, "banding-noise");
                for p in crate::noise::Parameter::ALL {
                    changed!(
                        a.overrides.get(p).map(f32::to_bits)
                            != b.overrides.get(p).map(f32::to_bits),
                        p.key()
                    );
                }
            }
            (None, None) => {}
            _ => changes.push("reconstruction"),
        }
        changes
    }
}

#[derive(PartialEq, Eq)]
struct Recipe {
    parameters: Key,
    data: DataType,
    inputs: Vec<(&'static str, u64)>,
}
struct Version {
    recipe: Recipe,
    generation: u64,
}
struct Entry {
    name: &'static str,
    generation: u64,
    value: Value,
}
#[derive(Default)]
pub(crate) struct Cache {
    owner: Option<Arc<()>>,
    versions: HashMap<&'static str, Version>,
    counter: u64,
    entries: Vec<Entry>,
    selected: Vec<&'static str>,
}
impl Cache {
    const LIMIT: u64 = 1024 * 1024 * 1024;
    pub(crate) fn bind(&mut self, owner: &Arc<()>) {
        if self
            .owner
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, owner))
        {
            *self = Self {
                owner: Some(owner.clone()),
                ..Default::default()
            };
        }
    }
    fn bytes(&self) -> u64 {
        self.entries
            .iter()
            .map(|entry| entry.value.buffer.size())
            .sum()
    }
    /// Track changed operations and their inputs rather than graph positions,
    /// because those positions may change between renders.
    fn plan(&mut self, graph: &Graph, targets: &[usize]) -> crate::source::Result<Vec<usize>> {
        let required = graph.required(targets)?;
        let initial = self.versions.is_empty();
        let mut dirty = vec![false; graph.outputs.len()];
        let mut local_changes = Vec::new();
        for (id, output) in graph.outputs.iter().enumerate() {
            let inputs = output
                .dependencies
                .iter()
                .map(|input| {
                    let name = graph.outputs[*input].name;
                    (name, self.versions[name].generation)
                })
                .collect();
            let recipe = Recipe {
                parameters: graph.nodes[output.producer].operation.key(output.name),
                data: output.data,
                inputs,
            };
            let old = self.versions.get(output.name);
            let local_changed = old.is_none_or(|old| {
                old.recipe.parameters != recipe.parameters
                    || old.recipe.data != recipe.data
                    || !old
                        .recipe
                        .inputs
                        .iter()
                        .map(|i| i.0)
                        .eq(recipe.inputs.iter().map(|i| i.0))
            });
            dirty[id] = local_changed || output.dependencies.iter().any(|input| dirty[*input]);
            if local_changed && required[id] {
                local_changes.push(id);
            }
            let generation = if let Some(old) = old.filter(|old| old.recipe == recipe) {
                old.generation
            } else {
                self.counter += 1;
                self.counter
            };
            self.versions
                .insert(output.name, Version { recipe, generation });
        }
        if initial {
            // Detail is the usual first edit when reconstruction is on. Keeping its
            // inputs ready avoids a full rerender on that first slider movement.
            let node = graph
                .nodes
                .iter()
                .find(|node| {
                    matches!(node.operation, crate::graph::Operation::Detail(_))
                        && required[node.outputs[0]]
                })
                .unwrap_or(&graph.nodes[graph.outputs[targets[0]].producer]);
            let pivot = node.outputs[0];
            // Detail needs both color and signal inputs. Keeping only one branch
            // would still force the other branch to rerun on every edit.
            for (id, output) in graph.outputs.iter().enumerate() {
                dirty[id] = id == pivot || output.dependencies.iter().any(|input| dirty[*input]);
            }
        }
        if initial || !local_changes.is_empty() {
            let mut frontier = Vec::new();
            for (_, output) in graph
                .outputs
                .iter()
                .enumerate()
                .filter(|(id, _)| required[*id] && dirty[*id])
            {
                for &input in &output.dependencies {
                    if !dirty[input]
                        && required[input]
                        && !frontier.contains(&graph.outputs[input].name)
                    {
                        frontier.push(graph.outputs[input].name);
                    }
                }
            }
            self.selected = frontier;
        }
        self.entries.retain(|entry| {
            self.versions
                .get(entry.name)
                .is_some_and(|version| version.generation == entry.generation)
        });
        Ok(graph
            .outputs
            .iter()
            .enumerate()
            .filter_map(|(id, output)| self.selected.contains(&output.name).then_some(id))
            .collect())
    }
    fn get(&self, name: &str) -> Option<Value> {
        let generation = self.versions.get(name)?.generation;
        self.entries
            .iter()
            .find(|entry| entry.name == name && entry.generation == generation)
            .map(|entry| entry.value.clone())
    }
    fn put(&mut self, name: &'static str, value: &Value) {
        if !self.selected.contains(&name) {
            return;
        }
        let generation = self.versions[name].generation;
        if self
            .entries
            .iter()
            .any(|entry| entry.name == name && entry.generation == generation)
        {
            return;
        }
        if self.bytes() + value.buffer.size() > Self::LIMIT {
            self.entries
                .retain(|entry| self.selected.contains(&entry.name));
        }
        if self.bytes() + value.buffer.size() <= Self::LIMIT {
            self.entries.push(Entry {
                name,
                generation,
                value: value.clone(),
            });
        }
    }
    pub(crate) fn finish(&mut self) {
        self.entries
            .retain(|entry| self.selected.contains(&entry.name));
    }
}

#[derive(Debug)]
pub(crate) struct Superseded;
impl std::fmt::Display for Superseded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("render superseded")
    }
}
impl std::error::Error for Superseded {}
#[derive(Default)]
pub(crate) struct Execution<'a> {
    pub cache: Option<&'a mut Cache>,
    pub cancel: Option<&'a dyn Fn() -> bool>,
    pub checkpoints: Vec<usize>,
}
impl Execution<'_> {
    pub fn check(&self) -> crate::source::Result<()> {
        if self.cancel.is_some_and(|cancel| cancel()) {
            Err(Box::new(Superseded))
        } else {
            Ok(())
        }
    }
    pub fn plan(&mut self, graph: &Graph, targets: &[usize]) -> crate::source::Result<()> {
        if let Some(cache) = &mut self.cache {
            self.checkpoints = cache.plan(graph, targets)?;
        }
        Ok(())
    }
    pub fn seed(&self, name: &str) -> Option<Value> {
        self.cache.as_ref()?.get(name)
    }
    pub fn store(&mut self, name: &'static str, value: &Value) {
        if let Some(cache) = &mut self.cache {
            cache.put(name, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn graph_for_detail(amount: f32) -> (Graph, usize) {
        use crate::{graph::Operation as O, noise::Parameters};
        use bytemuck::Zeroable;
        let mut p = Parameters::zeroed();
        p.source = [64, 64, 1, 1];
        p.tile = [64, 64, 0, 3];
        p.residual[3] = amount;
        let mut graph = Graph::default();
        let input = graph
            .add(
                O::RenderedInput {
                    data: DataType::CalibratedLayers,
                },
                &[],
            )
            .unwrap();
        let base = graph.add(O::NoiseBase(p), &[input]).unwrap();
        let raw = graph.add(O::Column(p), &[input]).unwrap();
        let ratio = graph.add(O::Ratio(p), &[raw, base]).unwrap();
        let prepared =
            crate::nodes::color::pattern::Prepared::new([64, 64, 1, 0], [0.; 4]).unwrap();
        let pattern = graph
            .add(
                O::Pattern {
                    params: p,
                    prepared,
                },
                &[ratio, base + 3],
            )
            .unwrap();
        let detail = graph.add(O::Detail(p), &[raw, base, pattern]).unwrap();
        (graph, detail)
    }
    #[test]
    fn detail_changes_only_its_own_recipe_and_keeps_three_input_versions() {
        let mut cache = Cache::default();
        let (first, root) = graph_for_detail(1.);
        let frontier = cache.plan(&first, &[root]).unwrap();
        assert_eq!(frontier.len(), 3);
        let inputs: Vec<_> = frontier
            .iter()
            .map(|id| {
                (
                    first.outputs[*id].name,
                    cache.versions[first.outputs[*id].name].generation,
                )
            })
            .collect();
        let previous = cache.versions["detail.gain-tiles"].generation;
        let (second, root) = graph_for_detail(1.2);
        cache.plan(&second, &[root]).unwrap();
        for (name, version) in inputs {
            assert_eq!(cache.versions[name].generation, version);
        }
        assert_ne!(cache.versions["detail.gain-tiles"].generation, previous);
    }
    #[test]
    fn continuation_boundary_includes_unchanged_colour_and_signed_signal() {
        use crate::{graph::Operation as O, image::ColorSpace as C};
        use bytemuck::Zeroable;
        let (mut graph, detail) = graph_for_detail(1.);
        let detail_node = graph.outputs[detail].producer;
        let params = match graph.nodes[detail_node].operation {
            O::Detail(p) => p,
            _ => unreachable!(),
        };
        let base = graph.nodes[detail_node].inputs[1];
        let ratio = graph.nodes[detail_node].inputs[2];
        let colour = graph
            .add(
                O::TiledColor {
                    params,
                    color: crate::nodes::sensor::calibration::ColorParams::zeroed(),
                    space: C::Srgb,
                },
                &[base],
            )
            .unwrap();
        let rgb = graph
            .add(
                O::Reconstruct {
                    params,
                    space: C::Srgb,
                },
                &[colour, detail, ratio],
            )
            .unwrap();
        let root = graph
            .add(
                O::Sharpen {
                    amount: 0.75,
                    space: C::Srgb,
                },
                &[rgb, rgb + 1],
            )
            .unwrap();
        let mut cache = Cache::default();
        let frontier = cache.plan(&graph, &[root]).unwrap();
        assert_eq!(frontier.len(), 5);
        assert!(frontier.contains(&colour));
        assert!(frontier.contains(&(rgb + 1)));
        let signal_version = cache.versions[graph.outputs[rgb + 1].name].generation;
        let rgb_version = cache.versions[graph.outputs[rgb].name].generation;
        let mut changed = params;
        changed.residual[3] = 1.4;
        graph.nodes[detail_node].operation = O::Detail(changed);
        assert_eq!(frontier, cache.plan(&graph, &[root]).unwrap());
        assert_eq!(
            signal_version,
            cache.versions[graph.outputs[rgb + 1].name].generation
        );
        assert_ne!(
            rgb_version,
            cache.versions[graph.outputs[rgb].name].generation
        );
    }
    #[test]
    fn shared_uniform_fields_have_stage_specific_keys() {
        use crate::noise::{Parameters, RecipeStage as S};
        use bytemuck::Zeroable;
        let a = Parameters::zeroed();
        let mut b = a;
        b.residual[3] = 1.;
        for stage in [S::Base, S::Column, S::Ratio, S::Geometry, S::Signal] {
            assert!(a.key(stage) == b.key(stage));
        }
        assert!(a.key(S::Detail) != b.key(S::Detail));
        b = a;
        b.filter[3] = 0.9;
        assert!(a.key(S::Base) != b.key(S::Base));
        assert!(a.key(S::Column) == b.key(S::Column));
        b = a;
        b.mono[0] = 2.;
        assert!(a.key(S::Ratio) != b.key(S::Ratio));
        assert!(a.key(S::Base) == b.key(S::Base));
        b = a;
        b.residual[0] = 0.3;
        assert!(a.key(S::Detail) != b.key(S::Detail));
        assert!(a.key(S::Ratio) == b.key(S::Ratio));
    }
    #[test]
    fn coalescing_tracks_parameter_identity_not_recipe_invalidation() {
        let a = Settings::new("Sunlight", "Standard");
        let mut b = a.clone();
        b.exposure = 0.1;
        assert_eq!(a.changed_parameters(&b), vec!["exposure"]);
        b = a.clone();
        b.reconstruction = Some(Default::default());
        assert_eq!(a.changed_parameters(&b), vec!["reconstruction"]);
        let mut c = b.clone();
        c.reconstruction.as_mut().unwrap().detail = 1.1;
        assert_eq!(b.changed_parameters(&c), vec!["detail"]);
    }
}
