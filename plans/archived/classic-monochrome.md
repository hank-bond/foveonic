---
summary: "Support reference-derived Classic monochrome rendering and editable DNG with independent color-mode controls."
status: archived
---

# Classic monochrome

Implement the accepted [Classic monochrome requirements](../../docs/specs/classic-rendering.md#monochrome-rendering) and [family controls](../../docs/specs/family-controls.md).

- [x] Prepare the monochrome mixture, neutral, spatial gains and functional low/high-ISO stage choices.
- [x] Preserve WB preferences while selecting effective monochrome calibration.
- [x] Keep supported color-mode selection active and validate its multiplication order against the reference implementation.
- [x] Use shared Classic finishing for photographic output and neutral linear DNG storage with embedded response.
- [x] Validate native technical and photographic boundaries, DNG reader behavior and UI interactions.
- [x] Complete workspace checks and rebuild the review app.

## Numerical evidence

Native-size SD9 monochrome technical output has mean absolute error 0.000000583 code and maximum one. DP2 matches its technical boundary exactly. Photographic mean absolute errors are 3.27 and 4.27 sRGB16 codes respectively. DP2's source carries a 90-degree orientation, so native pixel comparisons explicitly match the diagnostic request's unrotated presentation and assert dimensions. Production retains source orientation. DP2x ISO3200 technical mean is 0.272 code, with maximum 45. Photographic mean is 22.28 sRGB16 codes. Its deterministic diffusion retains the documented scheduling difference from the reference implementation.

Eight monochrome DNGs from SD9, DP2 and DP2x ISO3200 cover Standard, adjusted tone/exposure and available Vivid looks. Adobe SDK validation reports no warnings. All 104,210,496 stored components decode exactly. SDK sRGB mean differences from paired photographic TIFFs range from 0.000181 to0.000507, with maximum 0.003052. Stored RGB channels are exactly neutral. WB does not change samples. Exposure and tone change presentation while preserving samples. Selected color modes change the baked mixture. Lightroom and Capture One appearance remain unreviewed.

Monochrome GPU/DNG contract tests also pass acquired SD10, SD14, SD15, DP1, DP1s and DP2x ISO100 originals. All eight acquired models are covered. DP1x and DP2s genuine originals remain unavailable, as in ordinary color coverage.

## Organization

The existing Classic recipe composes both rendering selections. Metadata preparation supplies the singular mixture and direct matrix scale. Numerical nodes retain functional contracts and shared shaders. The DNG encoder replicates the prepared green signal through its existing storage matrix, while profile interpretation remains invertible. The GUI keeps Classic color modes, disables WB/highlight controls in monochrome and preserves their values for Color.

Native Vivid validation exposed a previous matrix composition error. Classic now composes camera*mode, matching the reference rows. Standard outputs remain unchanged. Exact SD9/DP2 captured matrix coefficients, nonlinear lookup tables and selected rows validate the preparation. The visible WB catalogue excludes Monochrome because rendering selects it separately. Unsupported Merrill monochrome overrides are normalized from sidecars and rejected in explicit requests.

DP2 monochrome Vivid matches the native technical boundary exactly and has photographic mean error 4.65 sRGB16 codes. Its 196,608 tone-table entries match an independent reference replay exactly. Validation also corrected missing selected-mode contrast compensation in the shared Classic photographic response. Both fixes apply to ordinary color modes as well as monochrome. Standard rendering remains unchanged.

All 317 ordinary workspace tests pass. Targeted native, GPU and Adobe SDK checks pass. Merrill color/monochrome TIFF and DNG exports remain byte-identical to main. Strict Clippy, formatting, documentation checks and the release app build pass. CLI monochrome rendering also succeeds without an inactive WB argument. Request validation now resolves required camera choices through the same capability contract as GUI controls.
