---
summary: "Draft and Thumb preview experiments reduce calibrated sensor grids before reconstruction while preserving native photographic geometry."
kind: reference
status: accepted
implementation: incomplete
---

# Small previews

The opt-in experiment must offer Draft at one quarter of native width and height and Thumb at one eighth. These names describe dimension ratios, not fixed megapixel counts. A 16 MP photograph yields approximately 1 MP in Draft and 0.25 MP in Thumb. Keep the release defaults unchanged until image quality and speed have been reviewed.

Reduction must filter calibrated sensor layers before expensive reconstruction. Preserve original source samples and native sensor-coordinate corrections. Do not expose a resize of an already reconstructed photograph as sensor-level reconstruction support. Existing Fast and Full processing must remain available.

Recorded framing, camera orientation, saved composition and native viewport coordinates must remain consistent. Sensor and processing alignment constraints must produce an established fallback rather than corrupt geometry. Edits retain their requested values. Spatial controls must use the appropriate working-grid scale, while noise approximations require validation at each size.

Zoomed detail, inspection and exports must use Full. Preview quality remains workspace policy and must not become an image sidecar edit. Validate completed reconstruction-changing edits separately from cached exposure edits. Compare appearance on original photographs before enabling a default.

Each camera family should use its existing processing recipe with scale-aware stages. Do not introduce a separate recipe that disables corrections solely to meet a Thumb timing target. A completed cached Thumb render near 50 ms and eight-photo edit completion near 400 ms are investigation goals, to be measured on distinct retained sessions rather than inferred from single-photo repetition.

The first prototype targets Merrill. Other camera families must retain their established preview behavior until their sensor grids and reconstruction paths are validated. See the [experiment plan](../../plans/rotation-and-small-preview.md) for current scope and findings.
