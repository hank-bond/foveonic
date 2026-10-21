---
summary: "Add editable Classic color DNG through shared packaging and measure the embedded appearance against photographic rendering."
status: archived
---

# Classic DNG

Implement the accepted [Classic requirements](../../docs/specs/classic-rendering.md#editable-color-dng) using the existing [DNG output contract](../../docs/specs/output-strategies.md#dng).

## Work

- [x] Confirm owner approval to include Classic DNG before completing the experiment.
- [x] Inventory the current technical-base, appearance and packaging tests.
- [x] Identify the Classic technical base and separate deferred photographic controls.
- [x] Prepare the Classic storage basis and one embedded appearance through the shared DNG interfaces.
- [x] Verify unchanged samples when deferred exposure, tone, saturation and mode settings change.
- [x] Export SD15, DP2 and DP1s originals and check metadata, geometry, finite values and encoded samples.
- [x] Independently decode and render exports where an authorized reader is available.
- [x] Measure appearance differences with defined metrics and retain representative review images.
- [x] Check Merrill regressions, formatting, Clippy, workspace tests and documentation indexes.

## Appearance target

A close photographic match is accepted for this slice. The owner's informal 95% goal expresses visual similarity, not a defined numerical threshold. Report measured errors and representative images rather than claiming a percentage of fidelity. DNG editing latitude takes priority over exact TIFF reproduction. Changes to the storage policy or unsupported Classic coverage require a separate decision.

## Validation approach

Extend the existing technical-base test to Classic fixtures. Selected appearance controls must leave the technical samples unchanged. Supported white-balance and content corrections may change those samples. Retained results must survive later renders.

Reuse the writer's synthetic sample and profile checks. Its optional independent stage-1 TIFF comparison can verify exact uint16 decoding. Container correctness and sample preservation do not establish photographic appearance. Compare reader-rendered output with the photographic route separately, including non-neutral saturation and adjusted exposure or tone.

The existing SDK investigation found camera-white and working-RGB clipping in the reader. Common range scaling and exposure compensation therefore need to be checked together. Integer export still clamps negative samples under the accepted shared policy. Later white-balance edits can expose additional highlight limits.

## Implementation

The Classic recipe shares its technical stages with photographic rendering, selecting uncompensated camera color for DNG. A functional signed linear-detail operation applies spatial detail and restores integral linear codes at 4096 per unit. The profile stores selected exposure, tone, saturation and an approximate matrix effect for color mode. It cannot reproduce the mode's changes to nonlinear layer corrections or reconstruction exactly.

The calibrated color basis before photographic response is not established as standard ROMM. Export back-projects it into camera-like coordinates and supplies an approximate profile mapping. Only the decoded photographic response has an established ROMM interpretation.

Classic restores its common storage scale through the fitted profile curve. Restoring it through positive baseline exposure caused working-RGB clipping before the look in 35 of 713 sampled DP2 pixels. Curve compensation removed that clipping without changing encoded samples. A functional first-crossing inverse also avoids a discontinuity where the tone curve reaches a white plateau. Merrill retains its existing baseline-exposure and inverse-endpoint conventions.

## Numerical evidence

Tests covered Sunlight/Standard, reconstruction enabled, sharpening amount one, and adjusted exposure +0.75, contrast +0.3, shadows +0.4, highlights -0.3, saturation 0.5 with Vivid where available. The DP1s has only Standard. Each comparison samples a 31 by 23 grid in the oriented active image and reports absolute normalized linear-ROMM component errors. These are an independent profile-application model, not an external editor acceptance result.

| Camera | Default mean / maximum | Adjusted mean / maximum |
| --- | --- | --- |
| SD15 | 0.000190 / 0.004145 | 0.002015 / 0.087278 |
| DP2 | 0.000117 / 0.004872 | 0.001177 / 0.099398 |
| DP1s | 0.000102 / 0.000705 | 0.000144 / 0.002123 |

The profile-fitting maximum is below 0.0049 in all six cases. Larger adjusted-color tails mainly reflect the accepted canonical-mode approximation. No default sampled pixel exceeds 0.01 in any channel. In adjusted SD15 and DP2, 0.56% and 0.42% of sampled pixels respectively exceed 0.05 in a channel. These fractions describe this sample grid, not a percentage of visual fidelity.

On all three originals, deferred controls preserve both the technical base and encoded uint16 samples exactly. White balance and sharpening change the base, retained images remain unchanged, and written strips preserve every GPU-encoded sample. Six exports contain 83,635,200 uint16 components in total.

The existing Merrill DNG technical-base and writer tests pass. The full SD15 photographic comparison retains its previous error budget: Stage 2 differs in 661 components by one code, and final TIFF mean absolute difference is 4.5094 of 65535 with maximum 223. DNG separation leaves that photographic behavior unchanged.

## Independent reader result

The macOS system DNG reader opened all six exports at the correct oriented dimensions and rendered them through `sips` with the system sRGB profile. For each camera, the default and adjusted exports produced identical decoded RGB pixels even though their appearance profiles differ. This reader therefore does not establish application of the embedded look. Its brightness and saturation differences from photographic TIFF cannot be treated as profile-fit error or visual acceptance. Representative pairs were retained for review.

Adobe SDK validation and appearance comparisons below complete this reader gate. Lightroom and Capture One acceptance remain pending under the shared DNG plan.

Workspace checks pass with 104 ordinary core tests and 128 GUI tests, plus CLI, documentation and tooling tests. Strict all-target Clippy, formatting and generated indexes pass. Targeted GPU checks cover linear detail, photographic response, all three Classic technical exports and the existing Merrill DNG separation. The owner approved using the existing Adobe validator and previously authorized the prose pass, commit and merge after the Classic slice was complete.

## Adobe SDK validation

Adobe `dng_validate` 1.7.1 (2724) validates all eight exports without warnings: default and adjusted Standard for all three cameras, plus adjusted Vivid for SD15 and DP2. Independent strip decoding matches every SDK stage-1 sample exactly, totaling 111,513,600 uint16 components. Default versus adjusted profiles render differently, confirming that this reader applies the embedded appearance.

Full-resolution comparisons below use SDK `-16 -cs1` output and photographic sRGB TIFFs. Errors are absolute normalized encoded-sRGB component differences, distinct from the sampled linear-ROMM model above. Adjusted Standard uses exposure +0.75, contrast +0.3 and saturation 0.5. Adjusted Vivid also uses shadows +0.4 and highlights -0.3.

| Camera / look | Mean | 95th percentile | Maximum | Pixels with any channel error above 0.05 |
| --- | --- | --- | --- | --- |
| SD15 Standard | 0.000466 | 0.001770 | 0.031312 | 0% |
| SD15 adjusted Standard | 0.000511 | 0.002243 | 0.030762 | 0% |
| DP2 Standard | 0.001228 | 0.003769 | 0.024369 | 0% |
| DP2 adjusted Standard | 0.001785 | 0.006043 | 0.025040 | 0% |
| DP1s Standard | 0.000595 | 0.001816 | 0.013092 | 0% |
| DP1s adjusted Standard | 0.000506 | 0.001556 | 0.019440 | 0% |
| SD15 adjusted Vivid | 0.001902 | 0.005631 | 0.165560 | 0.202% |
| DP2 adjusted Vivid | 0.002773 | 0.006897 | 0.150393 | 0.264% |

The Vivid tails are consistent with the accepted nonlinear-mode approximation. Paired full-image review renders show close photographic appearance, with localized differences. This is measured SDK behavior for the selected originals, not a universal percentage-fidelity claim.

Metadata-only white-balance edits multiplied the neutral's red and blue by 1.1 and 0.9. Separate exposure edits set baseline exposure to -2 EV. All six edited files validate, preserve every stored sample and change the SDK render. Exposure edits lower mean brightness in every camera. This establishes functioning controls, not an arbitrary later-WB highlight envelope.

Representative source DNGs, SDK renders, paired review images and numerical summaries were retained locally. Superseded system-reader and decoded stage-1 image dumps can be discarded after their exact-sample checks. No validator source or binary was imported into the repository.
