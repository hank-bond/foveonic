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
    pub(crate) source: Arc<Source>,
    pub(crate) cache: Cache,
    pub(crate) last_settings: Option<Settings>,
}
impl RenderSession {
    pub fn new(source: impl Into<Arc<Source>>) -> Self {
        Self {
            source: source.into(),
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
        changed!(
            self.preview_quality != next.preview_quality,
            "preview-quality"
        );
        changed!(self.preview_region != next.preview_region, "preview-region");
        changed!(self.rendering != next.rendering, "rendering");
        changed!(self.photographic != next.photographic, "photographic");
        changed!(self.monochrome.mix != next.monochrome.mix, "mono-mix");
        changed!(self.monochrome.noise != next.monochrome.noise, "mono-noise");
        changed!(
            self.monochrome_smoothness != next.monochrome_smoothness,
            "mono-smoothness"
        );
        changed!(
            self.monochrome.banding != next.monochrome.banding,
            "mono-banding"
        );
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
        changed!(self.highlight_mode != next.highlight_mode, "highlight-mode");
        changed!(
            self.highlight_amount != next.highlight_amount,
            "highlight-amount"
        );
        changed!(self.color_field != next.color_field, "color-field");
        changed!(self.despeckle != next.despeckle, "despeckle");
        changed!(self.row_correction != next.row_correction, "row-correction");
        changed!(
            self.color_dq_gain.map(f32::to_bits) != next.color_dq_gain.map(f32::to_bits),
            "color-dq"
        );
        changed!(
            self.sharpen_amount.map(f32::to_bits) != next.sharpen_amount.map(f32::to_bits),
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
        changed!(
            self.tonal_ranges.blacks != next.tonal_ranges.blacks,
            "tonal-blacks"
        );
        changed!(
            self.tonal_ranges.shadows != next.tonal_ranges.shadows,
            "tonal-shadows"
        );
        changed!(
            self.tonal_ranges.highlights != next.tonal_ranges.highlights,
            "tonal-highlights"
        );
        changed!(
            self.tonal_ranges.whites != next.tonal_ranges.whites,
            "tonal-whites"
        );
        changed!(self.output_size != next.output_size, "output-size");
        changed!(
            self.tiff_color_space != next.tiff_color_space,
            "tiff-color-space"
        );
        changed!(self.tone_enabled != next.tone_enabled, "tone-enabled");
        changed!(
            self.sharpening_enabled != next.sharpening_enabled,
            "refinement-enabled"
        );
        match (self.reconstruction, next.reconstruction) {
            (Some(a), Some(b)) => {
                changed!(a.noise_enabled != b.noise_enabled, "noise-enabled");
                changed!(a.detail_enabled != b.detail_enabled, "detail-enabled");
                changed!(a.detail.to_bits() != b.detail.to_bits(), "detail");
                changed!(
                    a.texture.fine_size.to_bits() != b.texture.fine_size.to_bits(),
                    "fine-size"
                );
                changed!(
                    a.texture.coarse_size.to_bits() != b.texture.coarse_size.to_bits(),
                    "coarse-size"
                );
                changed!(
                    a.texture.fine.to_bits() != b.texture.fine.to_bits(),
                    "fine-detail"
                );
                changed!(
                    a.texture.coarse.to_bits() != b.texture.coarse.to_bits(),
                    "coarse-detail"
                );
                changed!(a.chroma != b.chroma, "chroma-noise");
                changed!(a.luminance != b.luminance, "luma-noise");
                changed!(a.banding != b.banding, "banding-noise");
            }
            (None, None) => {}
            _ => changes.push("reconstruction"),
        }
        changes
    }
}

#[derive(PartialEq, Eq)]
struct Recipe {
    operation: std::mem::Discriminant<crate::graph::Operation>,
    parameters: Key,
    data: DataType,
    inputs: Vec<(String, u64)>,
}
struct Version {
    recipe: Recipe,
    generation: u64,
}
struct Entry {
    name: String,
    generation: u64,
    value: Value,
}
#[derive(Default)]
pub(crate) struct Cache {
    owner: Option<Arc<()>>,
    versions: HashMap<String, Version>,
    counter: u64,
    entries: Vec<Entry>,
    selected: Vec<String>,
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
            .enumerate()
            .filter(|(i, entry)| {
                !self.entries[..*i]
                    .iter()
                    .any(|prior| prior.value.buffer.shares_storage(&entry.value.buffer))
            })
            .map(|(_, entry)| entry.value.buffer.size())
            .sum()
    }
    fn room_for(&self, value: &Value) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.value.buffer.shares_storage(&value.buffer))
            || self.bytes() + value.buffer.size() <= Self::LIMIT
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
                    let name = graph.outputs[*input].cache_name();
                    (name.clone(), self.versions[&name].generation)
                })
                .collect();
            let recipe = Recipe {
                operation: std::mem::discriminant(&graph.nodes[output.producer].operation),
                parameters: graph.nodes[output.producer].operation.key(output.name),
                data: output.data,
                inputs,
            };
            let old = self.versions.get(&output.cache_name());
            let local_changed = old.is_none_or(|old| {
                old.recipe.operation != recipe.operation
                    || old.recipe.parameters != recipe.parameters
                    || old.recipe.data != recipe.data
                    || !old
                        .recipe
                        .inputs
                        .iter()
                        .map(|i| &i.0)
                        .eq(recipe.inputs.iter().map(|i| &i.0))
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
                .insert(output.cache_name(), Version { recipe, generation });
        }
        if initial {
            // Keep the inputs of adjustable reconstruction ready for its first edit.
            // Guided reconstruction also owns the luminance filter selection.
            let node = graph
                .nodes
                .iter()
                .find(|node| {
                    matches!(
                        node.operation,
                        crate::graph::Operation::Texture { .. }
                            | crate::graph::Operation::BlueGuidedReconstruct(_)
                    ) && required[node.outputs[0]]
                })
                .unwrap_or(&graph.nodes[graph.outputs[targets[0]].producer]);
            let pivot_producer = graph.outputs[node.outputs[0]].producer;
            // Preserve all input branches, including those consumed after the pivot.
            // Mark its side outputs dirty too: they must share the restart boundary.
            for (id, output) in graph.outputs.iter().enumerate() {
                dirty[id] = output.producer == pivot_producer
                    || output.dependencies.iter().any(|input| dirty[*input]);
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
                        && !frontier.contains(&graph.outputs[input].cache_name())
                    {
                        frontier.push(graph.outputs[input].cache_name());
                    }
                }
            }
            if initial {
                // Exposure and tone usually change before reconstruction controls.
                // Also retain their linear input so that first edit needs no warm-up.
                // The ordinary bounded cache admission still applies to both boundaries.
                for node in &graph.nodes {
                    if matches!(
                        node.operation,
                        crate::graph::Operation::Tone { .. }
                            | crate::graph::Operation::TonalRanges { .. }
                            | crate::graph::Operation::SignedPhotographic(_)
                            | crate::graph::Operation::CodeResponse { .. }
                    ) && required[node.outputs[0]]
                    {
                        for &input in &node.inputs {
                            let name = graph.outputs[input].cache_name();
                            if !frontier.contains(&name) {
                                frontier.push(name);
                            }
                        }
                    }
                }
            }
            self.selected = frontier;
        }
        for &id in &graph.session_outputs {
            let name = graph.outputs[id].cache_name();
            if required[id] && !self.selected.iter().any(|n| n == &name) {
                self.selected.push(name);
            }
        }
        // Explicit downstream editing boundaries take admission priority over
        // speculative initial frontiers. Later declarations are closer to the edit.
        self.selected.sort_by_key(|name| {
            graph
                .session_outputs
                .iter()
                .rev()
                .position(|id| graph.outputs[*id].cache_name() == *name)
                .unwrap_or(usize::MAX)
        });
        self.entries.retain(|entry| {
            self.versions
                .get(&entry.name)
                .is_some_and(|version| version.generation == entry.generation)
        });
        // An editing boundary can make earlier checkpoints unnecessary. If an
        // earlier entry was evicted, asking for it as another root would rerun
        // upstream processing just to refill a cache that already serves this edit.
        let cached: Vec<_> = graph
            .outputs
            .iter()
            .map(|o| self.get(&o.cache_name()).is_some())
            .collect();
        let needed = graph.required_until(targets, &cached)?;
        Ok(graph
            .outputs
            .iter()
            .enumerate()
            .filter_map(|(id, output)| {
                (needed[id] && self.selected.contains(&output.cache_name())).then_some(id)
            })
            .collect())
    }
    fn get(&self, name: &str) -> Option<Value> {
        let generation = self.versions.get(name)?.generation;
        self.entries
            .iter()
            .find(|entry| entry.name == name && entry.generation == generation)
            .map(|entry| entry.value.clone())
    }
    fn put(&mut self, name: &str, value: &Value) {
        if !self.selected.iter().any(|n| n == name) {
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
        if !self.room_for(value) {
            self.entries
                .retain(|entry| self.selected.contains(&entry.name));
        }
        if !self.room_for(value) {
            let priority = self.selected.iter().position(|n| n == name).unwrap();
            while !self.room_for(value) {
                let candidate = self
                    .entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, entry)| {
                        self.selected
                            .iter()
                            .position(|n| *n == entry.name)
                            .filter(|rank| *rank > priority)
                            .map(|rank| (i, rank))
                    })
                    .max_by_key(|(_, rank)| *rank);
                let Some((index, _)) = candidate else {
                    break;
                };
                self.entries.remove(index);
            }
        }
        if self.room_for(value) {
            self.entries.push(Entry {
                name: name.to_owned(),
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
    pub progress: Option<&'a dyn Fn(crate::render::RenderProgress)>,
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
    pub fn store(&mut self, name: &str, value: &Value) {
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
        p.reduction[0] = 1.;
        p = p.full_frame();
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
        let raw = graph
            .add(
                O::Column {
                    params: p,
                    enabled: true,
                },
                &[input],
            )
            .unwrap();
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
        let detail = graph
            .add(
                O::Texture {
                    params: p,
                    prepared: crate::nodes::color::texture::Prepared::new(
                        crate::noise::Controls {
                            texture: crate::noise::TextureControls {
                                fine: amount,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        1.,
                    )
                    .unwrap(),
                },
                &[raw, base, pattern],
            )
            .unwrap();
        (graph, detail)
    }
    #[test]
    fn recipe_restart_boundaries_supplement_the_adjustable_frontier() {
        let (mut graph, root) = graph_for_detail(1.);
        let original = graph.nodes[0].outputs[0];
        let mut cache = Cache::default();
        assert!(!cache.plan(&graph, &[root]).unwrap().contains(&original));
        graph.retain_for_session(original);
        let frontier = cache.plan(&graph, &[root]).unwrap();
        assert!(frontier.contains(&original));
        assert!(
            frontier.len() > 1,
            "recipe hints must retain the editing frontier too"
        );
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
        let previous = cache.versions["texture.layers-tiles"].generation;
        let (second, root) = graph_for_detail(1.2);
        cache.plan(&second, &[root]).unwrap();
        for (name, version) in inputs {
            assert_eq!(cache.versions[name].generation, version);
        }
        assert_ne!(cache.versions["texture.layers-tiles"].generation, previous);
    }
    #[test]
    fn texture_edits_invalidate_color_and_sharpening_but_reuse_filtering() {
        use crate::{graph::Operation as O, image::ColorSpace as C};
        use bytemuck::Zeroable;
        let (mut graph, detail) = graph_for_detail(1.);
        let detail_node = graph.outputs[detail].producer;
        let O::Texture { params, .. } = graph.nodes[detail_node].operation else {
            unreachable!()
        };
        let color = graph
            .add(
                O::TiledColor {
                    params,
                    color: crate::nodes::sensor::calibration::ColorParams::zeroed(),
                    space: C::Srgb,
                },
                &[detail],
            )
            .unwrap();
        let rgb = graph
            .add(
                O::ProjectColorTiles {
                    params,
                    space: C::Srgb,
                },
                &[color],
            )
            .unwrap();
        let signal = graph
            .add(O::SharpSignal { space: C::Srgb }, &[rgb])
            .unwrap();
        let sharpened = graph
            .add(
                O::Sharpen {
                    amount: 0.75,
                    space: C::Srgb,
                },
                &[rgb, signal],
            )
            .unwrap();
        let final_id = graph
            .add(
                O::Tone {
                    curve: vec![0., 1.],
                    exposure: 0.,
                    space: C::Srgb,
                },
                &[sharpened],
            )
            .unwrap();
        let mut cache = Cache::default();
        let inputs = graph.nodes[detail_node].inputs.clone();
        let frontier = cache.plan(&graph, &[final_id]).unwrap();
        assert!(inputs.iter().all(|id| frontier.contains(id)));
        let old_signal = cache.versions["detail.signal"].generation;
        graph.nodes[detail_node].operation = O::Texture {
            params,
            prepared: crate::nodes::color::texture::Prepared::new(
                crate::noise::Controls {
                    texture: crate::noise::TextureControls {
                        fine: 1.4,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                1.,
            )
            .unwrap(),
        };
        let frontier = cache.plan(&graph, &[final_id]).unwrap();
        assert!(inputs.iter().all(|id| frontier.contains(id)));
        assert_ne!(old_signal, cache.versions["detail.signal"].generation);
        graph.nodes[graph.outputs[final_id].producer].operation = O::Tone {
            curve: vec![0., 1.],
            exposure: 0.5,
            space: C::Srgb,
        };
        assert_eq!(cache.plan(&graph, &[final_id]).unwrap(), vec![sharpened]);
    }
    #[test]
    fn guided_reconstruction_keeps_both_planes_and_the_later_mask_branch() {
        use crate::{
            graph::Operation as O,
            nodes::color::{blue_guided_reconstruct, byte_mask_double, residual_finish},
        };
        let mut graph = Graph::default();
        let rgb = graph
            .add(
                O::RenderedInput {
                    data: DataType::CorrectedSensorRgb,
                },
                &[],
            )
            .unwrap();
        graph.outputs[rgb].name = "test.rgb";
        let blue = graph
            .add(
                O::RenderedInput {
                    data: DataType::CorrectedSensorBlue,
                },
                &[],
            )
            .unwrap();
        graph.outputs[blue].name = "test.blue";
        let original_mask = graph
            .add(
                O::RenderedInput {
                    data: DataType::HighlightMask,
                },
                &[],
            )
            .unwrap();
        graph.outputs[original_mask].name = "test.mask";
        let reconstructed = graph
            .add(
                O::BlueGuidedReconstruct(Arc::new(
                    blue_guided_reconstruct::Prepared::new([8, 8], 0., [0.; 3], None).unwrap(),
                )),
                &[rgb, rgb, rgb, blue],
            )
            .unwrap();
        let residual = graph
            .named_output(reconstructed, "reconstruction.residual")
            .unwrap();
        let mask = graph
            .add(
                O::ByteMaskDouble(byte_mask_double::Params::new([8, 8]).unwrap()),
                &[original_mask],
            )
            .unwrap();
        let finish = |detail| O::ResidualFinish {
            params: residual_finish::Params::new([16, 16], detail, 0., 0, 0., 0.).unwrap(),
            with_mask: true,
        };
        let root = graph
            .add(finish(1.), &[reconstructed, residual, mask])
            .unwrap();
        let mut cache = Cache::default();
        let frontier = cache.plan(&graph, &[root]).unwrap();
        assert_eq!(frontier, vec![rgb, blue, mask]);
        let previous = cache.versions["residual.rgb"].generation;
        graph.nodes[graph.outputs[root].producer].operation = finish(1.2);
        // A detail edit can use the initial checkpoints before reconstruction.
        assert_eq!(
            cache.plan(&graph, &[root]).unwrap(),
            vec![reconstructed, residual, mask]
        );
        assert_ne!(cache.versions["residual.rgb"].generation, previous);
        graph.nodes[graph.outputs[reconstructed].producer].operation = O::BlueGuidedReconstruct(
            Arc::new(blue_guided_reconstruct::Prepared::new([8, 8], 0.5, [0.; 3], None).unwrap()),
        );
        assert_eq!(cache.plan(&graph, &[root]).unwrap(), frontier);
    }

    #[test]
    fn shared_uniform_fields_have_stage_specific_keys() {
        use crate::noise::{Parameters, RecipeStage as S};
        use bytemuck::Zeroable;
        let a = Parameters::zeroed();
        let mut b = a;
        b.filter[3] = 0.9;
        assert!(a.key(S::Base) != b.key(S::Base));
        assert!(a.key(S::Column) == b.key(S::Column));
        b = a;
        b.mono[0] = 2.;
        assert!(a.key(S::Ratio) != b.key(S::Ratio));
        assert!(a.key(S::Base) == b.key(S::Base));
    }

    #[test]
    fn regional_keys_separate_global_context_tiles_and_projected_views() {
        use crate::{graph::Operation as O, noise::Parameters};
        use bytemuck::Zeroable;
        let mut p = Parameters::zeroed();
        p.source = [256, 256, 4, 4];
        p.tile = [192, 192, 64, 11];
        p.reduction[0] = 0.5;
        p = p.full_frame();
        let a = p.for_region([10, 10, 20, 20]).unwrap();
        let b = p.for_region([20, 20, 30, 30]).unwrap();
        let distant = p.for_region([200, 200, 210, 210]).unwrap();
        assert!(
            O::NoiseContext(a).key("noise.global-context")
                == O::NoiseContext(distant).key("noise.global-context")
        );
        assert!(O::NoiseBase(a).key("noise.tiles") == O::NoiseBase(b).key("noise.tiles"));
        assert!(O::NoiseBase(a).key("noise.layers") != O::NoiseBase(b).key("noise.layers"));
        assert!(O::NoiseBase(a).key("noise.tiles") != O::NoiseBase(distant).key("noise.tiles"));
        let prepared = crate::nodes::color::texture::Prepared::new(Default::default(), 1.).unwrap();
        assert!(
            O::Texture {
                params: a,
                prepared
            }
            .key("texture.layers-tiles")
                == O::Texture {
                    params: b,
                    prepared
                }
                .key("texture.layers-tiles")
        );
        assert!(
            O::Texture {
                params: a,
                prepared
            }
            .key("texture.change")
                != O::Texture {
                    params: b,
                    prepared
                }
                .key("texture.change")
        );
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
    #[test]
    #[ignore = "requires a GPU"]
    fn cached_aliases_count_one_allocation() {
        let gpu = crate::gpu::Gpu::new().unwrap();
        let buffer = gpu.upload("cache alias", &[0_f32; 16]).unwrap();
        let value = Value {
            buffer,
            description: None,
        };
        let cache = Cache {
            entries: vec![
                Entry {
                    name: "original".into(),
                    generation: 0,
                    value: value.clone(),
                },
                Entry {
                    name: "alias".into(),
                    generation: 0,
                    value: value.clone(),
                },
            ],
            ..Default::default()
        };
        assert_eq!(cache.bytes(), value.buffer.size());
        assert!(cache.room_for(&value));
    }

    #[test]
    #[ignore = "requires a GPU"]
    fn cached_edit_boundary_does_not_refill_evicted_upstream_checkpoints() {
        use crate::{
            color_space::ColorSpace, graph::Operation, nodes::finish::photographic::Params,
        };
        let gpu = crate::gpu::Gpu::new().unwrap();
        let recipe = |amount| {
            let mut graph = Graph::default();
            let input = graph
                .add(
                    Operation::RenderedInput {
                        data: DataType::ToneMappedRgb(ColorSpace::Srgb),
                    },
                    &[],
                )
                .unwrap();
            let mut params = Params::new([1, 1], ColorSpace::Srgb);
            let base = graph
                .add(
                    Operation::ImageAlias {
                        data: DataType::ToneMappedRgb(ColorSpace::Srgb),
                        output: "finish.base",
                    },
                    &[input],
                )
                .unwrap();
            params.values = [amount, 1., 0., 0.];
            let target = graph
                .add(
                    Operation::PhotographicTone {
                        params,
                        space: ColorSpace::Srgb,
                    },
                    &[base],
                )
                .unwrap();
            graph.retain_for_session(input);
            graph.retain_for_session(base);
            (graph, input, base, target)
        };
        let mut cache = Cache::default();
        let (graph, input, base, target) = recipe(0.4);
        assert!(cache.plan(&graph, &[target]).unwrap().contains(&input));
        cache.put(
            "finish.base",
            &Value {
                buffer: gpu.upload("retained edit input", &[0.3_f32; 3]).unwrap(),
                description: None,
            },
        );
        let (graph, _, _, target) = recipe(0.5);
        let checkpoints = cache.plan(&graph, &[target]).unwrap();
        assert!(checkpoints.contains(&base));
        assert!(!checkpoints.contains(&input), "an uncached input behind the retained boundary should not become another execution root");
        // Switching roots can need that upstream work again.
        assert!(cache.plan(&graph, &[input]).unwrap().contains(&input));
    }
}
