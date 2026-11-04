//! Checks native sparse-sensor correction before reduced interchangeable-lens previews.

use super::*;
use crate::{
    inspection::Content,
    profile::Profile,
    render::{ControlCapabilities, PreviewQuality, Rendering},
};

#[test]
#[ignore = "requires GPU and QUATTRO_X3F sd Quattro or Quattro H original"]
fn interchangeable_native_phase_correction_precedes_reduced_reconstruction() {
    let source = Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    assert!(calibration::is_interchangeable(&source.metadata));
    let engine = Engine::new().unwrap();
    let cap = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let mut settings = cap
        .editing_defaults(&source.metadata, Rendering::Color)
        .unwrap();
    settings.white_balance = cap.default_white_balance(&source.metadata).unwrap();
    settings.color_mode = cap.default_color_mode(&source.metadata).unwrap();
    let mut expected = None;
    for quality in [PreviewQuality::Full, PreviewQuality::Fast] {
        settings.preview_quality = quality;
        let mut profile = Profile::default();
        let mut execution = Execution::default();
        let recipe = build(
            &engine,
            &source,
            &settings,
            &["final"],
            &mut profile,
            &mut execution,
        )
        .unwrap();
        let phase = recipe
            .graph
            .outputs
            .iter()
            .position(|o| o.name == "phase-detection.blue")
            .expect("every grid needs native blue-plane sensor correction");
        let reconstruct = recipe
            .graph
            .nodes
            .iter()
            .find(|n| matches!(n.operation, Operation::BlueGuidedReconstruct(_)))
            .unwrap();
        // Follow the blue history specifically, so an orphan correction node
        // cannot satisfy this check while reconstruction uses uncorrected blue.
        let mut pending = vec![reconstruct.inputs[3]];
        let mut reaches_correction = false;
        while let Some(id) = pending.pop() {
            if id == phase {
                reaches_correction = true;
                break;
            }
            pending.extend(&recipe.graph.nodes[recipe.graph.outputs[id].producer].inputs);
        }
        assert!(
            reaches_correction,
            "{quality:?} must reconstruct corrected blue"
        );
        let mut artifacts = engine
            .collect_artifacts(
                &source,
                &settings,
                &recipe.graph,
                recipe.final_id,
                &["phase-detection.blue", "extraction.blue"],
                recipe.params,
                &mut profile,
                &mut execution,
            )
            .unwrap();
        let Content::Image(raw) = artifacts.pop().unwrap().content else {
            panic!("raw blue")
        };
        let Content::Image(corrected) = artifacts.pop().unwrap().content else {
            panic!("corrected blue")
        };
        assert_eq!(
            corrected.description().extent,
            [source.metadata.width as u32, source.metadata.height as u32]
        );
        let pixels = engine.readback_image(&corrected).unwrap();
        let raw = engine.readback_image(&raw).unwrap();
        assert!(pixels != raw, "fixture must exercise the sparse correction");
        if let Some(expected) = &expected {
            assert!(
                pixels == *expected,
                "{quality:?} must use exactly the Full native correction"
            );
        } else {
            expected = Some(pixels);
        }
    }
}
