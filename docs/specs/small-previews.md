---
summary: "Draft and Thumb preview experiments reduce calibrated sensor grids before reconstruction while preserving native photographic geometry."
kind: reference
status: accepted
implementation: conforming
---

# Small previews

The opt-in experiment must offer Draft at one quarter of native width and height and Thumb at one eighth. These names describe dimension ratios, not fixed megapixel counts. A 16 MP photograph yields approximately 1 MP in Draft and 0.25 MP in Thumb. Keep the release defaults unchanged until image quality and speed have been reviewed.

Reduction must filter calibrated sensor layers before expensive reconstruction. Preserve original source samples and native sensor-coordinate corrections. Do not expose a resize of an already reconstructed photograph as sensor-level reconstruction support. Existing Fast and Full processing must remain available.

Recorded framing, camera orientation, saved composition and native viewport coordinates must remain consistent. Sensor and processing alignment constraints must produce an established fallback rather than corrupt geometry. Edits retain their requested values. Spatial controls must use the appropriate working-grid scale, while noise approximations require validation at each size.

Smart detail promotion, inspection and exports must use Full. Explicit Draft/Thumb remain workspace preview choices. Preview quality must not become an image sidecar edit. Validate completed reconstruction-changing edits separately from cached exposure edits. Compare appearance on original photographs before enabling a default.

Each camera family should use its existing processing recipe with scale-aware stages. Do not introduce a separate recipe that disables corrections solely to meet a Thumb timing target. A completed cached Thumb render near 50 ms and eight-photo edit completion near 400 ms are investigation goals, to be measured on distinct retained sessions rather than inferred from single-photo repetition.

Supported paths must be explicit. The experiment covers Merrill, ordinary Quattro HIGH and validated full-resolution Classic color paths. Unsupported geometry, high-ISO Classic range reconstruction, reduced Classic RAW inputs, Tiger, Quattro LOW and monochrome must retain their established fallback. Full/Fast output must stay unchanged. See the [initial experiment](../../plans/archived/rotation-and-small-preview.md) and [optimization plan](../../plans/archived/scaled-editing-optimization.md) for validation and remaining limits.

Entering, leaving or committing Crop/Rotate must reuse compatible photographic pixels and preserve their actual working resolution. These presentation-only changes must not force Full or cancel matching in-flight pixel work. Explicit quality changes and photographic edits remain numerical processing requests.

## Reduced working-set ownership

A warm reduced-photo set should retain corrected reduced sensor inputs and useful small editing checkpoints within one explicit working-set budget. Native decoded CPU samples and full-resolution GPU checkpoints should be releasable for inactive photographs. Full detail and exports must recover authoritative native input on demand. Preserve the active Smart refinement path's reusable native work.

Reduced checkpoint validity must include source identity, working grid and all effective upstream correction choices. Native calibration, WB-dependent correction, highlight recovery, repairs and noise choices are not assumed to be fixed. Downstream edits should reuse a valid checkpoint. An upstream change must regenerate affected preparation rather than return stale pixels. Reduced buffers must keep explicit native-to-working geometry instead of being passed off as original sensor samples.

Shared preparation and post-reconstruction editing should remain usable by future camera families. This work does not require a new decoder family or a general camera plugin framework. The coordinated implementation is tracked in [scaled editing optimization](../../plans/archived/scaled-editing-optimization.md).
