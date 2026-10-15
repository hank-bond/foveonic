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
- Inspect **must** group processing buffers and their component views. RGB stages offer RGB/Red/Green/Blue, sensor-layer stages offer Layer 1/2/3, and scalar stages have a single entry. Component-only changes **must** reuse the retained buffer and texture without processing a new image. Scopes analyze all buffer channels independently of the display component. Their input region follows [GUI workspace](./gui-workspace.md).
- Group rows **must** have a right-aligned arrow; clicking anywhere on the row **must** expand or collapse their component choices inline without selecting an image.
- A Difference toggle beside Inspect **must** compare compatible layer and individual RGB-channel outputs with their explicitly identified operation inputs, using the same source and settings. Signed grayscale **must** use middle gray for unchanged, lighter for increased, and darker for decreased values. Display-only amplification and a visible baseline/legend **must** explain small changes without altering processing or export. Unsupported comparisons **must** be disabled with an explanation. Combined RGB magnitude heatmaps and scalar/branch-specific comparisons are subsequent work.
- An Input/Output toggle beside Difference **must** show the selected operation's actual input or output, with Output as the default. It **must** preserve the channel, zoom, pan, and image bounds when switching. After the compatible pair is prepared, switching **must** reuse the retained GPU views without photographic processing. Compatible combined RGB views **must** support Input/Output even where signed Difference is unavailable.
- Difference **must** temporarily override and disable Input/Output without losing its selection. Turning Difference off **must** restore that selection. Unsupported input comparisons **must** be disabled with an explanation. A pending input **must** show an explicit output fallback rather than an old or mismatched input.
- Difference subtraction **must** precede display mapping and preserve immutable source samples. Component, side, and amplification changes **must** reuse retained GPU views. Scopes **must** continue analyzing the selected output, clearly distinguished from the display-only input or difference. The navigator **must** follow the displayed side.
- Visualization **must** distinguish sensor layers from RGB and scalar fields. Diagnostic display mapping **must** preserve buffer samples and use the selected output's processing space.
- Interactive scheduling **must** bound in-flight render work and prevent stale results from replacing the current selection.

## Implementation direction

Use egui with eframe's wgpu backend. Initially use embedded JPEGs for browser thumbnails and the processing pipeline for the selected image. Retain requested outputs selectively.

Output dependency tracking and temporary lifetimes govern execution. Interaction checkpoints determine which results remain available for reuse. General multi-image caching and arbitrary branch comparisons remain deferred. Operation-input flickering and differences share selectively retained pairs of named outputs from one render configuration.

## Conformance

An earlier sensor-stage image and a named side output can each be selected for inspection. Execution follows the requested dependencies, transient outputs become reusable after their last consumer, and retained artifacts stay immutable across subsequent renders. Recomputed artifacts belong to the requested configuration.

Inline group rows have pointer-driven expansion tests. Signed layer/channel comparisons retain paired views from one render configuration; GPU fixtures cover neutral, positive/negative, amplified, signed and over-range samples before display mapping. A Merrill worker fixture checks eight operation/input pairs and settings-dependent baseline changes. Native visual review and combined RGB/scalar comparison work are tracked in the [inspection plan](../../plans/inspection-differences.md).
