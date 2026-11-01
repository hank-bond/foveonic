---
summary: "Optional Merrill multiscale layer filtering combines chroma cleanup with fixed shield-based row or row-and-column correction."
kind: reference
status: accepted
implementation: conforming
---

# Merrill multiscale noise

This specification extends [Detail and Noise](./detail-and-noise.md) for Merrill color and monochrome rendering through the shared GUI and CLI engine. The owner approved retaining both shield correction choices alongside the reference implementation. Reference remains the default.

## Appearance priorities

The filter should reduce broad green/magenta blotches and sensor-aligned color stripes while preserving useful color variation, fine texture, edges, and natural brightness grain. Maximum noise suppression is not the objective. Comparisons must assess introduced color shifts, desaturation, muddiness, and edge artifacts alongside the noise improvement. Acceptable fine brightness grain must not hide a regression in broad color consistency.

## Processing

The optional filter must read the three native sensor layers jointly before camera color conversion. It must use their shared spatial structure and layer noise estimates at three scales. Image arithmetic must run on the GPU. Rust owns parameter preparation, fixed calibration data, and graph construction. Filtering preserves the native three-layer sum. Rendered luminance also depends on subsequent calibration and color conversion.

Shield correction must use the current photograph's shield measurements with shipped calibration coefficients. The calibration data describes a fixed fitted relationship between shield measurements and active-sensor offsets. It is not evidence that every Merrill body, ISO, exposure time, or sensor condition shares that relationship. Further calibration may replace these coefficients when independent validation supports it.

The two multiscale choices must share one filtering implementation. Rows selects the universal row model without its broadest band. Rows + Columns selects the broader row and column models, including their broadest band. The second choice may alter broad shadow tint, so both choices must remain available for photographic comparison. Existing repair, white balance, color conversion, Detail, and finishing controls retain their own responsibilities.

Whole-frame sensor measurements and corrections must retain native sensor coordinates independently of orientation, output crop, and viewport. Regional Smart results must agree with the corresponding region of whole-frame Full processing. [Preview policies](./merrill-fast-preview.md) must preserve the selected method and all saved Noise and Detail values.

TIFF and editable DNG must use the same selected technical noise path before their output-specific processing. Inspection and interactive reuse must retain immutable source samples and account for the effective method and calibration parameters in operation identities.

## Controls and persistence

`Settings::merrill_noise_method` must offer `Reference`, `MultiscaleRows`, and `MultiscaleRowsAndColumns`, serialized as snake_case. Missing fields in older recipes must select Reference. The new choices apply to Merrill color and monochrome. Other families must reject explicit nondefault methods during processing validation. GUI compatibility normalization must restore Reference for those families while preserving compatible edits.

The Merrill Noise section must contain a Method dropdown with Reference, Multiscale: Rows, and Multiscale: Rows + Columns. The shared Noise enable state controls the optional method. Disabling Noise must preserve the selection and leave Detail independent. Changing rendering mode must preserve the selection. Monochrome retains its dedicated brightness noise amount and downstream monochrome reconstruction. Independent banding choices follow [Merrill banding](./merrill-banding.md). This feature adds no further adjustment sliders or calibration capture interface.

Noise-scoped recipes must copy the method for Merrill. Detail-scoped recipes must preserve the target's method. Existing sidecar, default-recipe, comparison, and processing-section behavior remains authoritative.

The CLI must accept `--merrill-noise-method reference|rows|rows-columns`. Selecting a method must enable reconstruction without resetting explicit noise strengths. Unknown values must fail validation.

## Validation

CPU checks must cover old-recipe defaults, strict enum decoding, round trips, CLI choices, interactive parameter identity, scoped recipes, and inactive-control preservation. GPU checks must cover layer geometry, shader validity, finite output, brightness-sum preservation, and agreement with a small independent calculation. Integration checks must cover cached and fresh processing, regional boundaries, unchanged reference output, Noise bypass, TIFF/DNG technical agreement, and retained source ownership.

Photographic review must include shadows, neutral and colored surfaces, fine hair or fabric, sensor edges, and multiple Merrill bodies. Review must consider both multiscale choices against the same reference settings. A numerical reduction in noise alone does not establish an aesthetic improvement or generalization beyond the tested captures.
