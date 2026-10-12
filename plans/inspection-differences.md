---
summary: "Make Inspect groups expand inline and add amplified signed layer/channel comparisons against explicit operation inputs."
---

# Inspection differences

Requirements: [pipeline inspection](../docs/specs/pipeline-inspection.md), [render resources](../docs/specs/render-resources.md), and [GUI workspace](../docs/specs/gui-workspace.md).

## Steps

- [x] Expand group rows inline with right-aligned arrows and pointer tests.
- [x] Define settings-aware comparison inputs for compatible layer/RGB operations.
- [ ] Retain paired GPU views from one render configuration and subtract before display mapping.
- [ ] Add Difference, display-only amplification, explicit baseline labels, and unavailable explanations.
- [ ] Validate mapping, paired worker requests, GPU presentation, and documentation checks.

## Scope

First slice: signed grayscale layer and individual RGB-channel differences. Combined RGB change-magnitude heatmaps and branching scalar diagnostics remain a subsequent slice. Original sensor readings, color conversion, tone's cross-unit conversion, final composite/export geometry, and diagnostic side outputs do not have an enabled difference in this slice. Scopes continue analyzing the selected output, not the display-only difference.

## Progress

Menu disclosure and comparison-input catalog tests pass: `cargo test --offline -p foveonic-gui inspect::tests` and `cargo test --offline -p foveonic inspection::tests` (four tests). Native menu review remains pending. Comparison integration is paused at the owner's request until the concurrent responsiveness pass finishes; the worker is actively being rewritten. No worker/presentation changes have been applied by this pass.

Preserve the concurrent interactive rendering work. Comparisons use multiple roots in one request, not separate settings snapshots or a second photographic processing path. Reconstruction compares with its RGB base; noise and column layers each compare with their common pre-filter layer input, not with one another.
