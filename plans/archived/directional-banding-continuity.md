---
summary: "The selected residual Coarse banding fit and Fine strength correction are integrated and validated."
status: archived
---

# Directional banding continuity

Requirements: [Merrill banding correction](../../docs/specs/merrill-banding.md).

## Closure

The owner selected Diagonal 640 after photographic review and approved closing the Detail and banding work. Commit `697540d` integrates the selected Coarse fit and Fine strength correction. Reference remains the application default. Directional Fine starts at strength one and size 27. Coarse remains optional at zero strength and size 119.

- [x] Reproduce block-boundary stair steps and separate actual broad bands from real scene structure.
- [x] Compare continuous, overlapping, robust and axis-coherent estimates on known signals and photographs.
- [x] Complete both Fine axes before measuring residual Coarse offsets.
- [x] Integrate the selected fit, remove trial choices and share native results across Full, Fast and regional views.
- [x] Correct Fine strength above one without changing its unit-strength clipping.
- [x] Verify photographic output, caching, regional alignment and repository checks.
- [x] Remove the completed exploratory harness from the source tree and preserve its evidence privately.

## Findings and selected behavior

The original estimator chose independent offsets in consecutive 40-pixel perpendicular blocks. Enlarging Coarse strength exposed those discontinuities. Its wide-minus-Fine estimate could also counteract accepted Fine cleanup. Direct residual measurement after both Fine axes avoids subtracting the earlier estimate again, but still follows some out-of-focus scene features.

The selected Coarse fit pools local residuals at fixed native row or column coordinates with a triangular radius of 640 pixels. Diagonal profile agreement reduces confidence where a shifted profile matches better than an axis-aligned one. Interpolation between fitted estimates avoids abrupt block selection. Both axes retain the same formulation, with joint three-layer acceptance. Genuine low-contrast scene features aligned with the sensor remain ambiguous to an image-only estimator. The comparisons do not establish a general visual advantage for diagonal rejection over ordinary pooling.

Fine previously extrapolated the complete unit-strength correction above one, including unequal channel black-floor adjustments. Constant signed layers could gain a color cast without any measured band. Additional strength now scales only the accepted offset after the unit correction. The path through strength one retains its reviewed appearance.

Full, Fast and regional views share complete native band measurements. Regional packing preserves each tile's halo and overlap phase. Fast reduces corrected native layers after estimation. Separate measurement and application identities permit nonzero strength edits to reuse compact offsets. Fine edits invalidate the subsequent residual Coarse stages. Zero Coarse strength dispatches no Coarse filtering.

## Validation and examples

The final native comparison covers 18 retained photographic crops. Fifteen match the selected trial exactly. Three at Coarse strength three differ by at most approximately 0.000046 calibrated code. Thirteen targeted banding checks pass on GPU, including signed behavior, unit compatibility, axis transposition, native regional packing and cached strength edits. A photographic Full/regional comparison also passes. The isolated committed snapshot passes workspace tests, strict Clippy and documentation checks. Formatting and five artwork checks pass.

The wider investigation included 928 photographic region comparisons, 310 known-signal cases and separate estimated/applied correction fields. Those measurements and rejected prototypes remain private research evidence. The production source retains only the selected processing and its regression checks. No training code is removed.

The low-light spider original is available in the supplemental [example collection](https://github.com/hank-bond/foveonic/releases/tag/example-photographs) as `low-light-spider-and-web.x3f`. It covers deep-shadow noise, broad bands, web strands and out-of-focus detail. Its original bytes were verified before publication. No editing sidecar is distributed.

The separate Detail black-bar follow-up is closed in [Detail filter borders](./detail-filter-border.md).
