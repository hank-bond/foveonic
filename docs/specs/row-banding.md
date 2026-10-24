---
summary: "GPU row-banding correction fits the native geometric patch banks, rejects scene-correlated residuals, and subtracts bounded additive per-row sensor bias before reconstruction."
kind: reference
status: accepted
implementation: conforming
---

# Row banding correction

## Requirements

- The engine **must** estimate native additive per-row layer bias after despeckle and before reconstruction/color conversion.
- Geometric 31/73/151-pixel polynomial fits **must** follow the reference patch banks, transposed sample order, correlation rejection, stable row contributions, count floor, and WB-scaled discard limits. Native row+1 consumption and asymmetric application bounds **must** be preserved.
- Photographic fitting, residuals, statistics, and correction **must** execute on GPU. Host fitting-matrix preparation **may** use fixed geometry only, with verified equivalence to the native selected bases.
- Original input and retained artifacts **must** remain immutable.
- Row correction **must** default on with an independent shared GUI/CLI control. Inspection **must** provide corrected layers; the engine **must** make small row diagnostics available without requiring reconstruction/color/tone execution.
- Merrill Fast preview **must** omit row estimation without changing the requested editing setting. Full photographs, inspection and DNG processing retain that setting.

## Implementation and evidence

`banding.layers` retains corrected sensor layers. Engine-only `banding.rows` has four columns: applied red, green, and blue bias in sensor-code units, then native contribution count. Image row y consumes diagnostics row y+1, retaining the reference convention. GUI inspection continues to expose images only. CLI switches are `--row-correction` and `--no-row-correction`.

Double-precision geometric Gram solves produce the same least-squares weights as the reference SVD for the three selected full-rank bases. No image values enter those host solves. GPU fitting runs independent coefficients in parallel while each RGB recurrence retains its ordered float32 FMA. Compensated float32 patch statistics approximate native double sums. A workgroup distributes each patch across lanes and merges compensated partial sums in fixed order. Parallel patches feed stable metadata-derived row links, so final row accumulation preserves native order and UInt16 count behavior. The residual workspace is reused by patch size to remain below a full-frame-sized GPU binding. Its private row-major layout lets adjacent GPU lanes access adjacent residuals. Row means use the corresponding index while retaining their original summation order.

Synthetic matrix/patch-bank tests cover all sizes, ordered contributions, counts, native application bounds, and immutable input. All six full-sensor identical-input native comparisons have identical accepted contribution counts. Maximum applied bias error is `0.000000477`; maximum corrected-image error is `0.000244` camera codes. The owner accepted the implemented photographic results. Broader camera/high-ISO coverage and cross-platform runtime remain unverified. See the [completion plan](../../plans/complete-reference-algorithms.md).
