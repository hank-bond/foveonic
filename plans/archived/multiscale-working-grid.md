---
summary: "Moves Merrill Multiscale filtering onto resized calibrated layers with explicit repair evidence and preserved native processing."
status: archived
---

# Merrill Multiscale on resized layers

Implements [small previews](../../docs/specs/small-previews.md) and [Merrill Multiscale noise](../../docs/specs/merrill-multiscale-noise.md). Remaining native work stays in the [early-reduction plan](../early-sensor-reduction.md).

The owner approved moving Multiscale first, with native Full unchanged. Keep native black/shield correction, calibration and mapped repairs. Reduced Color and Monochrome run Multiscale after their sensor pool and before highlight recovery. Guarded rows remain native in this slice.

- [x] Adapt the filter's signal basis, noise variance and spatial supports beside the node. Reuse calibration gain evaluation rather than duplicate its equations.
- [x] Preserve original clipping evidence on the working grid and remove the filter's native image dependencies in reduced recipes.
- [x] Verify independent equations, tiny/partial grids, actual filtering, immutable inputs and retained/fresh edits.
- [x] Compare native Full floats and reduced photographs, then measure resident-source preparation separately from retained editing.
- [x] Collect owner feedback. The owner reviewed app responsiveness, approved cleanup and requested committing this slice before guarded-row work.

The reduced path uses the calibration gain at each pooled cell's center to express calibrated means in a local sensor-code basis. It filters in that basis and adds only the converted correction to the original calibrated mean. This is an approximation when gains vary within a cell and requires photographic review. Independent-noise variance follows the pooled sample count approximation. The three native physical supports project onto the working grid with a one-pixel minimum. Full keeps its existing input preparation and final clipping gate.

Eight native Full float comparisons are byte-identical. Four cover Local/Global Color Field on coast and speaker. The other four cover coast and low-light speaker in Color/Rows and Monochrome/Rows + Columns. Nine Fast/Draft/Thumb color comparisons cover coast, speaker and yellow flowers, with additional Thumb Monochrome and Rows + Columns cases. Review sheets preserve broadly consistent brightness and color, with some fine-detail differences. Temporary float/TIFF payloads are deleted after hashes and display comparisons are retained. Persistent photographic preview identity advances for the changed reduced result.

Full Adaptive Tone is a separate qualification: its reduced analysis branch now uses resized Multiscale too. Explicit Universal/Natural and Universal/Strong comparisons change Full mean RGB by +0.0055% and +0.0031% on coast, and Universal/Natural changes speaker by -0.00055%. Maximum component differences are below 0.00035 on the nominal 0–1 final scale. These fits satisfy the owner's previously accepted one-percent Adaptive analysis tolerance, but are not bit-exact Full outputs. The native layer filtering and camera-response comparisons above remain exact. No analysis-only filtering exception is introduced.

Three warmed fresh-render samples from each resident decoded source put Thumb medians at 104–112 ms, versus 199–203 ms for the matched Color/Rows and Monochrome/Rows comparisons. Coast Rows + Columns measures 198 → 107 ms. These runs exclude source opening, decoding, export and output readback, but reuse engine metadata preparation. Alternating build order and workload sampling find no competing app/compiler processes. These are headless preparation measurements, not application input-to-display latency. A separate synchronized coast Thumb diagnostic attributes 66.5 → 2.85 ms to Multiscale GPU passes. Its cumulative storage allocations fall from about 1.75 GB to 50.3 MB. Those allocation totals are not peak resident memory, and synchronized diagnostics are not baseline render timings.

Color Field previously inferred repair provenance by comparing its current numerical input with the pre-repair image. A filter inserted between repair and recovery would falsely mark noise-filter changes as repairs. Its inputs now explicitly separate the pre/post-repair pair from the current numerical image. Reduced recipes reuse their existing pooled repair output, and Full aliases its unchanged input. No additional image operation is added. A focused GPU regression checks that changed numerical values do not invent repaired pixels.

Source-free cases cover both Multiscale methods, Monochrome, combined Directional/Adaptive controls and both Color Field solvers at divisors two, eight and sixty-four. Ninety edits match fresh pixels and release native source storage. The combined case also switches off resized filtering without requiring original input when the native shield/row prefix is unchanged. White-balance changes still require native input before any numerical dispatch. A parallel diagnostic test's one-shot emission-lock probe was changed to a bounded retry so unrelated brief emissions do not masquerade as a scope retaining that lock. Production recording is unchanged.

A separate two-photo Thumb exposure run with Universal/Natural and source-free sessions reuses Multiscale in all six measured edits. Only finishing operations execute, and both final results match fresh rendering. Workspace tests, strict all-target Clippy, formatting, documentation/index checks and artwork tests pass. The owner accepted the review build and requested the next Merrill slice.
