---
summary: "Saturation is independent of Tone and Color Range, with Reference and gamut-aware Universal processing."
kind: reference
status: accepted
implementation: conforming
---

# Saturation

The editor has separate Tone, Saturation and Color Range sections. Moving a section changes its visual order, not processing order. Each section owns its enable switch, reset, scoped presets and defaults. Batch processing independently selects the three sections.

Reference saturation uses the camera family's existing saturation response. Its Amount is neutral at one. Universal saturation uses the gamut-aware Oklab response selected through photographic comparisons, before the camera view rendering. Its Amount and Contrast are neutral at zero. Method selection belongs to Saturation independently of the Tone method and Tone enable switch.

Universal has an always-visible linked group of Shadows, Midtones and Highlights saturation amounts. The selections overlap in log luminance around the existing exposure-aware pivot. They feed the same perceptual adjustment as global saturation and use the existing fixed gamut-relative model. Creative selections remain fixed rather than photo-relative. Global, tonal and Color Range saturation controls display a balanced -1 through +1 scale with neutral at the center. Negative control values use the same processing amount. Positive control values map to twice that amount, so +0.5 uses processing +1 and +1 uses processing +2. Stored recipes, presets and CLI quantities retain the established -1 through +2 processing scale. Drawing controls must not rewrite stored precision or change existing looks. Direct color dragging must use the same balanced coordinates. Contrast remains -1 to +1 and its mapping is unchanged. All displayed values follow the shared numeric formatting policy.

Color Range contains the existing subject-color swatches and Hue, Saturation and Lightness controls. Its backend remains the shared linear Color Editor. Monochrome keeps Color Range lightness while chromatic saturation controls are inactive.

Universal saturation and tonal adjustments must share GUI, CLI, TIFF, DNG, Full, Fast and regional processing. DNG bakes chromatic linear edits through the existing chromatic baking path. Reference saturation remains represented through the existing camera appearance. Tone bypass must not bypass Saturation, and Saturation bypass must not bypass Tone or Color Range.

New settings paths use their ordinary defaults when absent. Do not infer method choices or translate amounts from retired experimental fields. Merely loading a photo must not rewrite its sidecar.

## Universal response

Saturation raises muted colors preferentially when positive and reduces vivid colors preferentially when negative. With the tonal saturation amounts at zero, -1 removes chroma. Sum the global and weighted tonal saturation contributions before applying one adjustment. Saturation Contrast increases differences in colorfulness when positive and compresses them when negative. It must not change photographic brightness contrast. Both Universal sliders at zero, with neutral tonal amounts, must omit processing and retain exact baseline samples. Either slider must work independently and participate in the existing saved settings, presets, bypasses, batch and DNG paths.

Use one working-gamut-relative colorfulness coordinate for both sliders. At fixed Oklab lightness and hue, find the available chroma in the declared working RGB basis. Nominal black and code white bound nonnegative samples. Expand those bounds to retain existing signed components, highlight headroom and the fixed-lightness neutral. Existing chroma beyond the first boundary defines full occupancy rather than triggering an independent RGB clamp. If a changed color lands in a gap along a wide-gamut hue ray, reduce its chroma to the first reachable boundary while retaining hue and lightness. This prevents added working-space excursions rather than replacing final display gamut mapping. The conservative pre-curve boundary remains a photographic limitation for review.

The contrast curve is `x' = x + contrast*x*(1 - x)*(2*x - 1)`, with colorfulness `x` between zero and one. This preserves neutral, half-occupancy and the gamut edge, remains monotone over the slider range and limits contrast's near-neutral amplification to two. Apply Saturation to that shaped colorfulness. For positive amount `s`, use `gain = (1 + s) / (1 + s*x')`. For `-1 < s < 0`, use `gain = (1 + s) / (1 + s - s*x')`. At or below -1 remove chroma. Transfer the resulting chroma ratio to the original Oklab components once. Preserve exact grays, signed samples, code scale, highlight headroom and perceptual hue/lightness. Retain this strength mapping without remapping existing values. The newly available +2 endpoint permits up to threefold near-neutral chroma gain before gamut restraint, or up to fivefold when a +2 global and +2 tonal amount combine. Contrast retains its original response.

Classify brightness from the immutable input using the selected camera/exposure-derived middle-gray pivot. Shadow and highlight crossovers span -3 to -0.5 and +0.5 to +3 stops relative to that pivot. Midtones fill the remaining weight. Weights must be nonnegative and sum to one. The bands affect only saturation. Equal saturation values should match an equal global saturation contribution. Do not reclassify pixels after their band adjustment. These masks are pointwise, not a second spatial HDR filter. Saturation Contrast retains its fixed midpoint at half the working gamut's available chroma. Keep creative pivots and crossovers tied to these references rather than deriving them from the shot’s histogram.

Full, reduced and regional results must agree for the same input pixels. Both controls are pointwise and need no extra spatial support. The response must be smooth and monotone in colorfulness and avoid reviving chroma in neutral pixels. With protection enabled, positive saturation must not add working RGB clipping to nonnegative inputs.

Expose the three tonal masks and the grade's actual input/output through existing inspection. Retain the grade input for responsive edits. Core photographic arithmetic stays on the GPU. Reuse Oklab shader functions with Color Editor rather than creating another copy.

The CLI selects `--saturation-method reference|universal` independently of `--tone-method`. `--saturation` retains the Reference amount. `--universal-saturation`, `--saturation-contrast`, `--saturation-shadows`, `--saturation-midtones` and `--saturation-highlights` set the Universal values without changing the selected method.

## Shared clipping protection

Universal global/tonal saturation and Basic/Advanced Color Range must use one shared perceptual saturation operation and boundary policy. Keep their existing selection masks and processing order. Selected ranges supply weighted amounts to the same operation rather than directly multiplying chroma. Advanced saturation must use the same pre-exposure code-white coordinates as the other saturation paths.

Offer one saved **Prevent saturation clipping** checkbox in the Editing hamburger menu, on by default. It governs Reference, Universal global/tonal and Basic/Advanced Color Range saturation across Classic, Merrill and Quattro. Editing presets/defaults/reset own this shared policy. Saturation and Color Range presets/reset must not change it. Missing data defaults to on without a migration or sidecar rewrite. Carry it through frozen settings, previews, CLI and TIFF/DNG export. Neutral edits must remain an exact bypass regardless of this choice.

With protection on, retain the existing hue-ray gamut boundary, shoulder and final gamut guard. Successive saturation additions approach that same working-gamut ceiling. Preserve existing signed input and highlight headroom. This does not undo clipping introduced by Hue, Lightness or later tone operations.

With protection off, positive saturation uses `x'/(1 + x')` in the positive gain denominator instead of `x'`, where `x'` is the existing contrast-shaped gamut occupancy. This preserves the muted-color preference while permitting movement at the old boundary. Skip the final gamut guard for positive saturation in this mode. Preserve desaturation and Contrast-only behavior. Finite output and exact achromatic protection remain required, but later clipping and displayed hue shifts are permitted. With protection off, Reference saturation retains its original arithmetic.

Reference retains its native saturation direction and processing position. With protection on, shoulder positive additions toward that operation’s native RGB bounds. Preserve existing signed samples and highlight headroom. Merrill and Quattro use their weighted RGB neutral after camera tone. Classic uses its equal-channel neutral before its final photographic response table and guards against the table’s white plateau. Neutral and negative saturation retain the native path. DNG appearance fitting must sample the same guarded operation before conversion to the profile basis.

No additional image buffers, analysis passes or per-range processing passes belong in this change. Share one WGSL implementation, and validate protected/unlocked behavior, maximum and overlapping amounts, signed/headroom samples, code-scale independence, Color Range equivalence, shared settings and native controls.
