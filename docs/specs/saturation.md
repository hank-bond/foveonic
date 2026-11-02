---
summary: "Saturation is independent of Tone and Color Range, with Reference and gamut-aware Universal processing."
kind: reference
status: accepted
implementation: conforming
---

# Saturation

The editor has separate Tone, Saturation and Color Range sections. Moving a section changes its visual order, not processing order. Each section owns its enable switch, reset, scoped presets and defaults. Batch processing independently selects the three sections.

Reference saturation uses the camera family's existing saturation response. Its Amount is neutral at one. Universal saturation uses the gamut-aware Oklab response selected through photographic comparisons, before the camera view rendering. Its Amount and Contrast are neutral at zero. Method selection belongs to Saturation independently of the Tone method and Tone enable switch.

Universal has an always-visible linked group of Shadows, Midtones and Highlights saturation amounts. The selections overlap in log luminance around the existing exposure-aware pivot. They feed the same perceptual adjustment as global saturation and use the existing fixed gamut-relative model. The earlier photo-relative selection and extended ranges are not retained. All displayed values follow the shared numeric formatting policy.

Color Range contains the existing subject-color swatches and Hue, Saturation and Lightness controls. Its backend remains the shared linear Color Editor. Monochrome keeps Color Range lightness while chromatic saturation controls are inactive.

Universal saturation and tonal adjustments must share GUI, CLI, TIFF, DNG, Full, Fast and regional processing. DNG bakes chromatic linear edits through the existing chromatic baking path. Reference saturation remains represented through the existing camera appearance. Tone bypass must not bypass Saturation, and Saturation bypass must not bypass Tone or Color Range.

New settings paths use their ordinary defaults when absent. Do not infer method choices or translate amounts from retired experimental fields. Merely loading a photo must not rewrite its sidecar.

## Universal response

Saturation raises muted colors preferentially when positive and reduces vivid colors preferentially when negative. With the tonal saturation amounts at zero, -1 removes chroma. Sum the global and weighted tonal saturation contributions before applying one adjustment. Saturation Contrast increases differences in colorfulness when positive and compresses them when negative. It must not change photographic brightness contrast. Both Universal sliders at zero, with neutral tonal amounts, must omit processing and retain exact baseline samples. Either slider must work independently and participate in the existing saved settings, presets, bypasses, batch and DNG paths.

Use one working-gamut-relative colorfulness coordinate for both sliders. At fixed Oklab lightness and hue, find the available chroma in the declared working RGB basis. Nominal black and code white bound nonnegative samples. Expand those bounds to retain existing signed components, highlight headroom and the fixed-lightness neutral. Existing chroma beyond the first boundary defines full occupancy rather than triggering an independent RGB clamp. If a changed color lands in a gap along a wide-gamut hue ray, reduce its chroma to the first reachable boundary while retaining hue and lightness. This prevents added working-space excursions rather than replacing final display gamut mapping. The conservative pre-curve boundary remains a photographic limitation for review.

The contrast curve is `x' = x + contrast*x*(1 - x)*(2*x - 1)`, with colorfulness `x` between zero and one. This preserves neutral, half-occupancy and the gamut edge, remains monotone over the slider range and limits contrast's near-neutral amplification to two. Apply Saturation to that shaped colorfulness. For positive amount `s`, use `gain = (1 + s) / (1 + s*x')`. For `-1 < s < 0`, use `gain = (1 + s) / (1 + s - s*x')`. At or below -1 remove chroma. Transfer the resulting chroma ratio to the original Oklab components once. Preserve exact grays, signed samples, code scale, highlight headroom and perceptual hue/lightness. Keep this original two-slider strength rather than extending the endpoint response.

Classify brightness from the immutable input using the selected camera/exposure-derived middle-gray pivot. Shadow and highlight crossovers span -3 to -0.5 and +0.5 to +3 stops relative to that pivot. Midtones fill the remaining weight. Weights must be nonnegative and sum to one. The bands affect only saturation. Equal saturation values should match an equal global saturation contribution. Do not reclassify pixels after their band adjustment. These masks are pointwise, not a second spatial HDR filter. Saturation Contrast retains its fixed midpoint at half the working gamut's available chroma. Keep creative pivots and crossovers tied to these references rather than deriving them from the shot’s histogram.

Full, reduced and regional results must agree for the same input pixels. Both controls are pointwise and need no extra spatial support. The response must be smooth and monotone in colorfulness, avoid reviving chroma in neutral pixels, and prevent positive controls from adding working RGB clipping to nonnegative inputs.

Expose the three tonal masks and the grade's actual input/output through existing inspection. Retain the grade input for responsive edits. Core photographic arithmetic stays on the GPU. Reuse Oklab shader functions with Color Editor rather than creating another copy.

The CLI selects `--saturation-method reference|universal` independently of `--tone-method`. `--saturation` retains the Reference amount. `--universal-saturation`, `--saturation-contrast`, `--saturation-shadows`, `--saturation-midtones` and `--saturation-highlights` set the Universal values without changing the selected method.
