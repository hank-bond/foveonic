---
summary: "GPU column conditioning corrects raw processing tiles after base filtering and before luminance/detail estimation, retaining native windows, cuts, ordered means, strict eligibility, and clipping."
kind: reference
status: accepted
implementation: conforming
---

# Column conditioning

## Requirements

- Raw processing tiles **must** receive native color-path column conditioning after base filtering and before luminance/detail estimation. Already-filtered base tiles and earlier artifacts **must** remain unchanged.
- Processing **must** retain horizontal window 27, default column preset 2, native effective-ISO blend selection, 40-row blocks, ordered float32 means, `block_height+1` divisor, strict 6% eligibility, and corrected-sample clamp 0–32766.
- First/last tile-row cuts **must** use the native column rectangle, distinct from the shading/work rectangle. The final corrected column **must** retain the native zero scratch-filter value.
- Photographic filtering, accumulation, and correction **must** execute on GPU. Native extra-row reads and square scratch clears **must** be represented safely without exposing mutable aliases; unused backing is not an image artifact.
- Typed conditioned tiles **must** remain distinct from full-frame layers and filtered-base tiles. Earlier base/edge inspection **must** skip column execution and preparation. Conditioned-layer inspection **must** skip unrelated base/detail/color work.

## Implementation and evidence

`Column` takes full-frame calibrated layers and produces `ConditionedTiles` for the existing full-resolution reconstruction branch. `Detail` consumes those tiles plus independently filtered base tiles. `column.layers` projects an independently owned visual buffer using the verified native overlap ramps. The stage follows reconstruction opt-in; it does not add another default correction switch. Later reduced-resolution reconstruction work is tracked separately in the [completion plan](../../plans/complete-reference-algorithms.md).

A native component fixture checks ordered blocks, asymmetric filter/correction columns, margins, negative values, headroom/clamping, gain 0/1/1.3, and immutable input. All six full-sensor identical-input native comparisons pass measured float32 tolerances: mean absolute error ≤`0.0000686` camera codes; ordinary maxima ≤`0.00293`. Native double variance/LUT selection and strict conditional eligibility produce 16 exceptional components exceeding `0.05` among approximately 290 million compared components, with maximum `2.915` camera codes. Inspected exceptional samples straddle the native strict 6% boundary by tiny rounding margins; the threshold is not softened to force equality. This is measured numerical conformance, not bit identity. Owner photographic review and broader camera/high-ISO/cross-platform coverage remain pending.
