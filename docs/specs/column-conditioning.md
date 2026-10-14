---
summary: "GPU column conditioning corrects raw processing tiles after base filtering and before luminance/detail estimation, retaining native windows, cuts, ordered means, strict eligibility, and clipping."
kind: reference
status: accepted
implementation: conforming
---

# Column conditioning

## Requirements

- Raw processing tiles **must** receive native color-path column conditioning after base filtering and before luminance/detail estimation. Already-filtered base tiles and earlier artifacts **must** remain unchanged.
- Column banding **must** expose five snapping integer presets 0–4, default 2, shared by GUI/CLI and saved image edits. Presets **must** select horizontal windows `15/21/27/33/39` and HI range coefficients `.625/.5/.375/.25/.125` together, retaining native effective-ISO scaling at ISO800. Position 0 **must** remain a preset, not Off; the control **must not** multiply accepted row offsets or change the separate row-correction switch.
- Processing **must** retain 40-row blocks, ordered float32 means, `block_height+1` divisor, strict 6% eligibility, and corrected-sample clamp 0–32766. Filter/correction margins **must** follow the selected window, preserving the asymmetric final scratch-filter column.
- First/last tile-row cuts **must** use the native column rectangle, distinct from the shading/work rectangle. The final corrected column **must** retain the native zero scratch-filter value.
- Photographic filtering, accumulation, and correction **must** execute on GPU. Native extra-row reads and square scratch clears **must** be represented safely without exposing mutable aliases; unused backing is not an image artifact.
- Typed conditioned tiles **must** remain distinct from full-frame layers and filtered-base tiles. Earlier base/edge inspection **must** skip column execution and preparation. Conditioned-layer inspection **must** skip unrelated base/detail/color work.

## Implementation and evidence

`Column` takes full-frame calibrated layers and produces `ConditionedTiles` at the full-resolution tile geometry shared by both reconstruction branches. `Detail` consumes those tiles plus independently filtered base tiles. `column.layers` projects an independently owned visual buffer using the verified native overlap ramps. The Banding noise slider appears under Noise and detail and follows that path's opt-in. Row banding correction remains a separate earlier switch. Reduced base filtering retains its own [geometry and filtering](./reduced-reconstruction.md).

The midpoint component fixture checks ordered blocks, asymmetric filter/correction columns, margins, negative values, headroom/clamping, gain 0/1/1.3, and immutable input. An additional guarded installed-library fixture retains all five window/coefficient pairs on identical synthetic samples. The [slider plan](../../plans/banding-slider-and-settings-defaults.md) tracks implementation checks; the [verification plan](../../plans/reference-banding-verification.md) distinguishes native consumer evidence from unresolved application exports. All six full-sensor identical-input native comparisons pass measured float32 tolerances: mean absolute error ≤`0.0000686` camera codes; ordinary maxima ≤`0.00293`. Native double variance/LUT selection and strict conditional eligibility produce 16 exceptional components exceeding `0.05` among approximately 290 million compared components, with maximum `2.915` camera codes. Inspected exceptional samples straddle the native strict 6% boundary by tiny rounding margins; the threshold is not softened to force equality. This is measured numerical conformance, not bit identity. Owner photographic review and broader camera/high-ISO/cross-platform coverage remain pending.
