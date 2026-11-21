//! Resolves output demands backward, then selects supported regional storage before cache planning.

use super::{Graph, Operation};
use crate::source::Result;

/// Bounds use the coordinate domain of the output they request.
/// A whole table and a whole photograph have different domains.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Demand {
    #[default]
    Unused,
    Region([u32; 4]),
    Whole,
}

impl Demand {
    pub fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Whole, _) | (_, Self::Whole) => Self::Whole,
            (Self::Unused, other) | (other, Self::Unused) => other,
            (Self::Region(a), Self::Region(b)) => Self::Region([
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]),
        }
    }
}

impl Graph {
    pub(crate) fn demands(&self, roots: &[(usize, Demand)]) -> Result<Vec<Demand>> {
        self.propagate_demands(roots, |operation, output, demand, slot| {
            operation.input_demand(output, demand, slot)
        })
    }

    fn propagate_demands(
        &self,
        roots: &[(usize, Demand)],
        input_demand: impl Fn(&Operation, &str, Demand, usize) -> Result<Demand>,
    ) -> Result<Vec<Demand>> {
        let mut demands = vec![Demand::Unused; self.outputs.len()];
        for &(id, demand) in roots {
            let target = demands
                .get_mut(id)
                .ok_or("requested output does not exist")?;
            *target = target.union(demand);
        }
        for node in self.nodes.iter().rev() {
            for &id in &node.outputs {
                if demands[id] == Demand::Unused {
                    continue;
                }
                let output = &self.outputs[id];
                // Dependencies identify upstream outputs. When one output is bound
                // to several slots, visit every matching slot conservatively.
                for (slot, &input) in node
                    .inputs
                    .iter()
                    .enumerate()
                    .filter(|(_, input)| output.dependencies.contains(input))
                {
                    let required = input_demand(&node.operation, output.name, demands[id], slot)?;
                    demands[input] = demands[input].union(required);
                }
            }
        }
        Ok(demands)
    }

    /// Views participate in cache identity, so each storage candidate must be
    /// probed with its own concrete framing rather than the authored crop.
    fn resolve_output_views(
        &mut self,
        index: usize,
        storage: &[Option<[u32; 4]>],
        views: &[Option<crate::image::View>],
    ) {
        let node = &self.nodes[index];
        for (&id, &view) in node.outputs.iter().zip(views) {
            let bounds = node
                .operation
                .output_storage(self.outputs[id].name)
                .or_else(|| node.operation.inherited_storage(storage));
            self.outputs[id].view = match (view, bounds) {
                (Some(view), Some(bounds)) => {
                    Some(crate::nodes::finish::image_region::map_view(view, bounds))
                }
                _ => view,
            };
        }
    }

    /// Demand is independent of storage: a retained broader result can satisfy it.
    /// Resolve a freshly assembled graph once, before normal cache identity and
    /// frontier planning. The resulting concrete graph is consumed by execution.
    pub(crate) fn resolve_regions(
        &mut self,
        roots: &[(usize, Demand)],
        reusable: impl Fn(&Self, usize) -> bool,
    ) -> Result<Vec<Demand>> {
        let demands = self.demands(roots)?;
        let mut output_storage = vec![None; self.outputs.len()];
        for index in 0..self.nodes.len() {
            let node = &self.nodes[index];
            if node.outputs.iter().all(|&id| demands[id] == Demand::Unused) {
                continue;
            }
            let views: Vec<_> = node
                .outputs
                .iter()
                .map(|&id| self.outputs[id].view)
                .collect();
            let storage: Vec<_> = node.inputs.iter().map(|&id| output_storage[id]).collect();
            self.nodes[index]
                .operation
                .resolve_input_storage(&storage)?;
            let output_demands: Vec<_> = self.nodes[index]
                .outputs
                .iter()
                .map(|&id| (self.outputs[id].name, demands[id]))
                .collect();
            if let Some(candidate) = self.nodes[index]
                .operation
                .regional_output(&output_demands)?
            {
                let original = std::mem::replace(&mut self.nodes[index].operation, candidate);
                let main = self.nodes[index].outputs[0];
                self.resolve_output_views(index, &storage, &views);
                if !reusable(self, main) {
                    let candidate = std::mem::replace(&mut self.nodes[index].operation, original);
                    self.resolve_output_views(index, &storage, &views);
                    if !reusable(self, main) {
                        self.nodes[index].operation = candidate;
                    }
                }
            }
            self.resolve_output_views(index, &storage, &views);
            let operation = &self.nodes[index].operation;
            for &id in &self.nodes[index].outputs {
                let bounds = operation
                    .output_storage(self.outputs[id].name)
                    .or_else(|| operation.inherited_storage(&storage));
                output_storage[id] = bounds;
            }
            let main = self.nodes[index].outputs[0];
            if !self.sensor_data_outputs.contains(&main) {
                if let (Some(priority), Some(slot)) = (
                    self.session_outputs.iter().position(|&id| id == main),
                    self.nodes[index].operation.regional_restart_input(),
                ) {
                    let input = self.nodes[index].inputs[slot];
                    self.session_outputs.remove(priority);
                    match self.session_outputs.iter().position(|&id| id == input) {
                        Some(existing) if existing < priority => {
                            self.session_outputs.remove(existing);
                            self.session_outputs.insert(priority - 1, input);
                        }
                        Some(_) => {}
                        None => self.session_outputs.insert(priority, input),
                    }
                }
            }
        }
        Ok(demands)
    }
}

#[cfg(test)]
#[path = "regions_test.rs"]
mod tests;
