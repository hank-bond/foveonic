---
summary: "Pipeline inspection requests named node outputs, executes their dependencies, and retains only selected artifacts and checkpoints."
kind: reference
status: accepted
implementation: incomplete
---

# Pipeline inspection

## Requirements

- The GUI **must** allow inspecting selected visual image buffers throughout the pipeline. The image inspection selector **must** be limited to these buffers. Diagnostics for node data, including black levels and dark-shield histograms, are deferred. Graph wiring remains internal.
- Nodes **must** be able to expose multiple named outputs, including sensor-layer images, masks, and other useful side outputs. Outputs **must** identify their layout, processing meaning, and geometry.
- Execution and settings preparation **must** be limited to requested outputs and their dependencies.
- Each output **must** remain alive until its last required consumer has submitted its read. Displayed artifacts, explicitly retained references, and interaction checkpoints **must** remain immutable and excluded from reusable scratch.
- Materialization **should** be limited to required outputs where the algorithm permits. Outputs needed internally or downstream **may** occupy temporary storage released after their last required consumer.
- Switching to an unretained output **may** recompute its dependencies. Results **must** preserve the source, settings, and algorithm context needed to avoid mixing artifacts from different configurations.
- Inspect **must** group processing buffers and their component views. RGB stages offer RGB/Red/Green/Blue, sensor-layer stages offer Layer 1/2/3, and scalar stages have a single entry. Component-only changes **must** reuse the retained buffer and texture without processing a new image. Scopes continue analyzing the full buffer independently of the display component.
- Visualization **must** distinguish sensor layers from RGB and scalar fields. Diagnostic display mapping **must** preserve buffer samples and use the selected output's processing space.
- Interactive scheduling **must** bound in-flight render work and prevent stale results from replacing the current selection.

## Implementation direction

Use egui with eframe's wgpu backend. Initially use embedded JPEGs for browser thumbnails and the processing pipeline for the selected image. Retain requested outputs selectively.

Output dependency tracking and temporary lifetimes govern execution. Interaction checkpoints determine which results remain available for reuse. General multi-image caching and the comparison interface remain deferred. Multiple roots and independent output lifetimes must support future branch comparisons and flickering.

## Conformance

An earlier sensor-stage image and a named side output can each be selected for inspection. Execution follows the requested dependencies, transient outputs become reusable after their last consumer, and retained artifacts stay immutable across subsequent renders. Recomputed artifacts belong to the requested configuration.
