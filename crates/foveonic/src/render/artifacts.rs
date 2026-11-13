//! Collects requested graph outputs as owned images or diagnostic tables.

use super::{recipes::RecipeInput, Engine, Settings};
use crate::{
    graph::{DataType, Graph},
    image::{RenderedImage, View},
    interactive::Execution,
    profile::Profile,
    source::Result,
};

impl Engine {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn collect_artifacts(
        &self,
        source: RecipeInput<'_>,
        settings: &Settings,
        graph: &mut Graph,
        final_id: usize,
        names: &[&str],
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<Vec<crate::inspection::Artifact>> {
        use crate::inspection::{Artifact, Content};
        let targets: Vec<_> = names
            .iter()
            .map(|name| {
                if *name == "final" {
                    Ok(final_id)
                } else {
                    graph
                        .outputs
                        .iter()
                        .position(|o| o.name == *name)
                        .ok_or_else(|| format!("unavailable output {name}"))
                }
            })
            .collect::<std::result::Result<_, _>>()?;
        if targets
            .iter()
            .enumerate()
            .any(|(i, id)| targets[..i].contains(id))
        {
            return Err("duplicate requested output".into());
        }
        execution.check()?;
        let mut roots = targets.clone();
        let estimate = if settings.dehaze_estimate && settings.dehaze_auto() {
            graph
                .outputs
                .iter()
                .position(|o| o.name == "dehaze.analysis")
        } else {
            None
        };
        if let Some(id) = estimate {
            if !roots.contains(&id) {
                roots.push(id);
            }
        }
        profile.measure("resolve graph regions", || {
            let demands: Vec<_> = roots
                .iter()
                .map(|&id| {
                    let demand = if id == final_id && settings.preview_region.is_some() {
                        graph.outputs[id]
                            .view
                            .map_or(crate::graph::Demand::Whole, |view| {
                                crate::graph::Demand::Region(view.crop)
                            })
                    } else {
                        crate::graph::Demand::Whole
                    };
                    (id, demand)
                })
                .collect();
            graph.resolve_regions(&demands, |graph, id| execution.can_reuse(graph, id))
        })?;
        profile.measure("plan interactive checkpoints", || {
            execution.plan(graph, &roots)
        })?;
        for &id in &execution.checkpoints {
            if !roots.contains(&id) {
                roots.push(id);
            }
        }
        let values =
            self.execute_controlled(source.context(), graph, &roots, profile, execution)?;
        let dehaze_estimate = if let Some(id) = estimate {
            let value = &values[roots.iter().position(|root| *root == id).unwrap()];
            let bytes = self.gpu.readback(&value.buffer)?;
            let table: &[f32] = bytemuck::cast_slice(&bytes);
            (table.len() == 8 && table[3] > 0.).then(|| [table[0], table[1], table[2]])
        } else {
            None
        };
        values
            .into_iter()
            .zip(targets)
            .map(|(value, id)| {
                let output = &graph.outputs[id];
                let mut actual = settings.clone();
                actual.working_scale = value.working_scale;
                let settings = &actual;
                let content = if let Some(description) = value.description {
                    Content::Image(Box::new(RenderedImage {
                        buffer: value.buffer,
                        description,
                        view: if description.view_baked {
                            View {
                                crop: [0, 0, description.extent[0], description.extent[1]],
                                rotation: 0,
                                extent: description.extent,
                            }
                        } else if description.space.uses_full_sensor_view() {
                            let extent = description.extent;
                            let rotation = value
                                .view
                                .map_or(source.metadata().rotation, |view| view.rotation);
                            View {
                                crop: [0, 0, extent[0], extent[1]],
                                rotation,
                                extent: if matches!(rotation, 90 | 270) {
                                    [extent[1], extent[0]]
                                } else {
                                    extent
                                },
                            }
                        } else {
                            value
                                .view
                                .ok_or("camera image output is missing framing provenance")?
                        },
                        metadata: source.metadata().clone(),
                        settings: settings.clone(),
                        dehaze_basis: match &graph.nodes[output.producer].operation {
                            crate::graph::Operation::ImageAlias { dehaze_basis, .. } => {
                                *dehaze_basis
                            }
                            _ => None,
                        },
                        owner: self.owner.clone(),
                        regional: None,
                    }))
                } else {
                    let bytes = self.gpu.readback(&value.buffer)?;
                    let values = match output.data {
                        DataType::Histogram => bytemuck::cast_slice::<u8, u32>(&bytes)
                            .iter()
                            .map(|v| *v as f32)
                            .collect(),
                        DataType::BlackLevels | DataType::RowTable | DataType::DehazeAnalysis => {
                            bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
                        }
                        _ => return Err("unsupported diagnostic output".into()),
                    };
                    Content::Table {
                        columns: match output.data {
                            DataType::Histogram => 4096,
                            DataType::RowTable | DataType::DehazeAnalysis => 4,
                            _ => 1,
                        },
                        values,
                    }
                };
                Ok(Artifact {
                    name: output.name,
                    dehaze_estimate,
                    metadata: source.metadata().clone(),
                    settings: settings.clone(),
                    content,
                })
            })
            .collect()
    }
}
