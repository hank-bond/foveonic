---
summary: "Merrill previews use the shared working-scale and processing-quality policies while preserving camera controls and native framing."
kind: reference
status: accepted
implementation: incomplete
---

# Merrill previews

Merrill previews follow [processing quality](./processing-quality.md) and the [viewport working-resolution policy](./viewport-workflows.md#processing-quality). Quality and Speed select processing fidelity independently of image resolution. Full, Fast, Draft and Thumb remain internal diagnostic size labels; they are not additional workspace modes. The former Fast and Smart selectors do not define separate Merrill recipes.

Color and Monochrome must honor the requested power-of-two working scale under the shared [small-preview contract](./small-previews.md). Preserve immutable source samples, native framing, camera orientation and authored controls. Partial sensor cells and processing alignment use covered working grids and edge padding. Sensor dimensions and crop alignment must not cause a whole-recipe fallback to native resolution. Invalid source data remains an error.

The selected calibration, repair, highlight recovery, despeckling, row correction, Noise, Detail and ColorDQ processing must remain available at the working scale. Operation order, original clipping evidence, spatial support and noise scaling follow the small-preview contract and each operation's specification. Reduced previews must not disable corrections solely because they use a smaller grid. The camera-derived local or global noise-base reduction remains part of the [reconstruction algorithm](./reduced-reconstruction.md), independently of the photograph's working scale.

Native viewport detail, regional processing and whole-image continuation preserve the selected processing quality. Shared [refinement ownership and priority](./smart-preview.md#refinement-ownership) govern request delivery and continuation. Merrill regional reconstruction must retain the required processing tiles, filtering context, column alignment and seam blending. Whole-image measurements and global noise context may precede regional work where their dependencies require them. Retain reusable prerequisites within the shared session budget, and preserve immutable overview and detail images while newer work runs.

Inspection and photographic/DNG exports use native Quality as specified by the shared policies. Viewport coordinates, scopes and navigator pairing must remain registered across working-scale changes. Processing quality and working resolution remain transient request state outside photographic sidecars.

The archived [initial Fast implementation](../../plans/archived/merrill-fast-preview.md), [noise-control follow-up](../../plans/archived/merrill-fast-noise.md) and [regional implementation](../../plans/archived/merrill-regional-preview.md) record historical validation. Current scale support is tracked in the [full-option scaled-pipeline plan](../../plans/full-option-scaled-pipeline.md); those earlier reports do not override its shared working-grid requirements.
