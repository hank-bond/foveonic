//! Compares optional Tiger numerical methods on ordinary Quattro without changing application recipes.

use super::*;
use crate::{
    inspection::Content,
    profile::{Mode, Profile},
};
use std::{path::PathBuf, sync::Arc};

// Ordinary Quattro's existing sensor-code clipping contract. Tiger's digital
// gains, channel ceilings, phase sites and preservation weights are not portable.
const CODE_CEILING: f32 = 14000.;
const GREEN_LEVEL: f32 = 13000.;

fn experiment(
    mut built: BuiltRecipe,
    normalization: f32,
    recovery: bool,
    adaptive: bool,
) -> BuiltRecipe {
    let old = built.graph;
    let mut graph = Graph::default();
    let mut remap = vec![0; old.outputs.len()];
    let mut recovered_channels = None;
    for node in old.nodes {
        let mut inputs: Vec<_> = node.inputs.iter().map(|&i| remap[i]).collect();
        let operation = match node.operation {
            Operation::HighlightRestoration(prepared) if recovery => {
                Operation::HighlightRestoration(Arc::new(
                    (*prepared)
                        .clone()
                        .with_channel_limits(
                            [CODE_CEILING / normalization; 3],
                            GREEN_LEVEL / normalization,
                            true,
                        )
                        .unwrap(),
                ))
            }
            Operation::RawColorBlend(prepared) if recovery => {
                inputs.push(recovered_channels.expect("preceding highlight recovery"));
                let prepared = Arc::try_unwrap(prepared)
                    .unwrap_or_else(|_| panic!("experiment owns color preparation"));
                Operation::RawColorBlend(Arc::new(
                    prepared
                        .with_masked_highlights([CODE_CEILING; 3], 1.)
                        .unwrap(),
                ))
            }
            Operation::BlueGuidedReconstruct(prepared) if adaptive => {
                Operation::BlueGuidedReconstruct(Arc::new(
                    (*prepared).clone().with_adaptive_range(true),
                ))
            }
            operation => operation,
        };
        let extended = matches!(&operation, Operation::HighlightRestoration(_) if recovery);
        let output = graph.add(operation, &inputs).unwrap();
        for (slot, &id) in node.outputs.iter().enumerate() {
            remap[id] = if slot == 0 {
                output
            } else {
                graph.named_output(output, old.outputs[id].name).unwrap()
            };
        }
        if extended {
            recovered_channels = Some(
                graph
                    .named_output(output, "highlight-restoration.recovery-flags")
                    .unwrap(),
            );
        }
    }
    built.final_id = remap[built.final_id];
    built.graph = graph;
    built
}

#[test]
#[ignore = "requires GPU, QUATTRO_X3F and QUATTRO_TIGER_EXPERIMENT_DIR; writes optional review TIFFs"]
fn compare_portable_tiger_methods_on_ordinary_quattro() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    assert!(fast_available(&source.metadata));
    let root = PathBuf::from(std::env::var_os("QUATTRO_TIGER_EXPERIMENT_DIR").unwrap());
    std::fs::create_dir_all(&root).unwrap();
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.highlight_amount = Some(1.);
    settings.tiff_color_space = Some(crate::render::TiffColorSpace::Srgb);
    let normalization = calibration::normalization(&source.metadata).unwrap();
    let mut baseline = Vec::<f32>::new();
    let mut receipt = Vec::new();
    for (name, recovery, adaptive) in [
        ("baseline", false, false),
        ("adaptive", false, true),
        ("recovery", true, false),
        ("combined", true, true),
    ] {
        let mut execution = Execution::default();
        let built = build(&engine, &source, &settings, &["final"], &mut execution).unwrap();
        let built = experiment(built, normalization, recovery, adaptive);
        let mut profile = Profile::new(Mode::Gpu);
        let artifacts = engine
            .collect_artifacts(
                &source,
                &settings,
                &built.graph,
                built.final_id,
                &["final"],
                built.params,
                &mut profile,
                &mut execution,
            )
            .unwrap();
        let Content::Image(image) = &artifacts[0].content else {
            panic!("photograph")
        };
        let bytes = engine.readback_image(image).unwrap();
        let values: &[f32] = bytemuck::cast_slice(&bytes);
        assert!(values
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v)));
        if baseline.is_empty() {
            baseline = values.to_vec();
        }
        assert_eq!(values.len(), baseline.len());
        let mut maximum = 0_f32;
        let mut absolute = 0_f64;
        let mut squares = 0_f64;
        let mut changed = 0_usize;
        for (&a, &b) in values.iter().zip(&baseline) {
            let difference = (a - b).abs();
            maximum = maximum.max(difference);
            absolute += f64::from(difference);
            squares += f64::from(difference).powi(2);
            changed += usize::from(difference != 0.);
        }
        receipt.push(serde_json::json!({
            "variant": name, "extent": image.view().extent, "components": values.len(),
            "changed": changed, "maximum": maximum, "mean_absolute": absolute / values.len() as f64,
            "rms": (squares / values.len() as f64).sqrt(), "profile": profile,
        }));
        engine
            .write_tiff(
                image,
                &root.join(format!("{name}.tiff")),
                &mut Profile::default(),
            )
            .unwrap();
        eprintln!(
            "{name}: max {maximum}, mean {}",
            absolute / values.len() as f64
        );
    }
    std::fs::write(
        root.join("comparison.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
}
