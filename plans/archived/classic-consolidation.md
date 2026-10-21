---
summary: "Consolidate Classic stage selection and preparation while enforcing signal-scale boundaries."
status: archived
---

# Consolidate Classic recipes

Implement the agreed organization cleanup following [Classic monochrome](./classic-monochrome.md). Preserve the [Classic rendering requirements](../../docs/specs/classic-rendering.md), numerical behavior and lazy inspection.

- [x] Share route selection between composition and the inspection catalogue.
- [x] Separate calibration, camera color, reconstruction and appearance preparation.
- [x] Distinguish normalized intermediate RGB codes from codes ready for root/detail.
- [x] Check representative color, monochrome, bypass and DNG routes.
- [x] Review the consolidated implementation and run workspace checks.

Route selectors validate metadata when their stage is reached. Early sensor inspection must not require later color or reconstruction parameters. Numerical operations remain camera unaware. The disabled-FIR native route intentionally retains its direct coefficients and requires no subsequent gain restoration.

Keep engine-owned white-balance preparation and caching unchanged during this cleanup. Genuine DP1x/DP2s coverage and manual DNG-reader review remain open in the [coverage plan](../classic-coverage.md).

## Validation and review

All 321 ordinary workspace tests pass. Real-file checks cover DP2 color inspection and DNG, inactive WB in monochrome, monochrome DNG appearance separation, SD14 reconstruction bypass and photographic/DNG brightness, SD9/DP2 native monochrome comparisons, and low/high-ISO inspection catalogues. Native comparisons retain the preceding measurements: DP2 technical codes are exact, SD9 differs by at most one code, and photographic sRGB16 means remain 4.27 and 3.27 codes respectively.

Graph tests reject normalized codes entering root/detail, repeated gain restoration and ready codes entering logarithmic reconstruction. The disabled-FIR test preserves matrix coefficients and nonlinear tables while declaring the direct route ready. Formatting, strict Clippy and documentation checks pass.

A second review corrected selector error handling and aligned red-detail selection with its integer conversion. Selectors now report malformed metadata only when needed. Calibration retains tightly coupled WB gains and correction preparation. Further splitting has no demonstrated benefit. The family UI, numerical operations and engine-owned cache remain intact.
