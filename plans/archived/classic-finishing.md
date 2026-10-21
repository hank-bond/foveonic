---
summary: "Separate Classic photographic response from shared wide-gamut output conversion and measure the reference differences."
status: archived
---

# Shared Classic finishing

Implements the agreed finishing boundary in [Classic rendering](../../docs/specs/classic-rendering.md). The owner requested using generic finishing where possible and measuring the differences before reviewing the experiment.

## Work

- [x] Compare signed Classic finishing with the existing floating photographic nodes.
- [x] Identify the actual source profile response instead of assuming standard ROMM encoding.
- [x] Produce normalized linear ROMM and reuse shared TIFF and display conversion.
- [x] Exercise all three working spaces and verify preserved wide-gamut values.
- [x] Measure bounded and original-file reference differences and preserve Merrill behavior.

## Findings and selected boundary

Classic finishing takes signed root-domain integer codes. It adds detail in that domain, applies exponential/root tables, then per-channel tone tables and a rounded equal-channel saturation matrix. The existing floating tone node instead samples a metadata-derived sigmoid/Bezier curve. Its saturation matrix uses brightness weights of 0.299, 0.587 and 0.114. Those photographic operations are not interchangeable without changing the image. The selected experiment preserves the signed operation instead of adding modes to unrelated floating nodes.

The table called gamma includes a lifted shadow toe and an additional highlight shoulder. It is not simply a standard output encoding. On a captured 512-pixel fixture, dropping it changes output by up to 1,864 RGB16 codes, with mean absolute difference 619.445.

The reference implementation's embedded ROMM profile uses a pure-power transfer with gamma 461/256, or 1.80078125. Standard ROMM uses a linear shadow toe and a power of 1.8. Substituting that standard inverse produces a full SD15 mean difference of 23.937 and maximum 576 RGB16 codes. At the photographic response's nominal zero, the standard inverse gives about 56 linear16 codes instead of the reference profile's 29. This accounts for the larger shadow differences.

The recipe now supplies the functional transfer definition `Power(461/256)`. Preparation composes that inverse with the existing photographic response into a linear response table. The GPU preserves the integer detail, tone and saturation arithmetic, then emits normalized linear ROMM. It no longer uploads or runs the legacy destination matrix/shapers, clips through sRGB, or quantizes an intermediate sRGB image. The original photographic response table's RGB16 quantization remains.

Generic output converts the declared working primaries to sRGB, Adobe RGB or ROMM and applies the destination encoding. Generic display mapping also accepts ROMM. Scopes retain source-space channel histograms with ROMM luminance weights. Default TIFF selection follows the camera metadata when present and falls back to sRGB when absent, independently of the working primaries.

The input response and output profile are deliberately separate. A standard ROMM export uses the existing standard ICC profile and its shadow toe, even though the reference photographic response was decoded with a pure power. This preserves the represented linear colors.

## Numerical evidence

The captured finishing fixture differs from the legacy profile result by up to 8 RGB16 codes with mean 4.243 under an independent scalar evaluation. Remaining differences primarily come from the slightly different legacy colorants/matrix and sampled output response. Applying the legacy matrix as well reduces the bounded maximum to 3 and mean to 0.596. Exact legacy numerical compatibility would therefore require profile choices beyond the camera-independent space name.

The complete SD15 output compared with the reference implementation differs in 12,668,524 of 13,939,200 RGB16 components, with mean absolute difference 4.50943 and maximum 223. The numerous small differences are expected from replacing a quantized profile transform. This is an approximation with a measured bound, rather than bit-exact output. Before changing finishing, the maximum was already 221 from rare one-code reconstruction differences. The unchanged reconstruction boundary still differs in 661 of 14,450,688 signed words, each by one code.

Compared directly with the previous experiment TIFFs, the new output differs as follows. Values are absolute RGB16 code differences.

| Camera | Maximum | Mean | 99th percentile |
| --- | ---: | ---: | ---: |
| SD15 | 54 | 4.508 | 17 |
| DP2 | 100 | 5.419 | 22 |
| DP1s | 46 | 4.450 | 19 |

In the SD15 ROMM export, 375,828 pixels have all ROMM components strictly between zero and one but convert outside the bounded sRGB channel range. This includes both channel-limit headroom and gamut differences. Negative sRGB components also occur. The original wide-gamut values remain available until the selected destination clips them. This comparison uses native-size TIFFs from the selected baseline request, rather than arbitrary saved settings.

## Checks and limitations

Workspace tests, selected GPU export tests across all source/destination combinations, scopes and GUI presentation tests pass. Display tests verify conversion and clipping while retaining the unchanged ROMM floating source, including values above one. A 600 by 400 Merrill color-field rendering remains byte-identical to the retained main output. The experiment stays uncommitted for review.

This slice shares output finishing while preserving the photographic behavior demonstrated to differ. It does not claim that every Classic tone/detail operation should become an option on the Merrill nodes. Exact legacy profiles, broader camera coverage and arbitrary saved adjustments remain outside the measured claim.
