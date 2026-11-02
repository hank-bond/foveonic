---
summary: "Separated Tone, Saturation and Color Range with independent methods, presets, bypasses and exports."
status: archived
---

# Saturation closeout

The owner selected separate Tone, Saturation and Color Range sections. Saturation gets its own Reference/Universal selector, Amount, Contrast and an always-visible linked Shadows/Midtones/Highlights group. New settings use defaults when absent. Do not migrate old saturation values or add compatibility rendering paths.

- [x] Decouple saturation activation and presets from Tone.
- [x] Keep Reference saturation on the camera path and Universal gamut-aware Oklab saturation on the shared linear path.
- [x] Move saturation controls into their own section and rename the displayed Color Editor section to Color Range.
- [x] Update independent reset/preset/default, processing-section and inspection behavior.
- [x] Remove the retired perceptual-mode toggle and stale experimental guidance.
- [x] Check independent bypasses, color processing, DNG, preview reuse and the native layout.

The existing tonal saturation masks already overlap in log luminance and feed the same perceptual adjustment as the global slider. Keep their fixed gamut-relative behavior and accepted slider ranges. Photo-relative selection and extended ranges remain rejected. Color Range swatches retain their existing HSL response. Changing that response is outside this completed slice.

Verification passed the workspace tests, warning-free Clippy, formatting, documentation and artwork checks. Seven numerical GPU tests cover gamut protection, neutral and signed values, monotonicity, both control directions and tonal overlap. Original Merrill checks cover neutral and cached/fresh output with Reference Tone and Universal saturation, chromatic DNG reprojection, output-section bypasses and full/regional agreement with active tonal saturation. A separate GPU check covers contrast-only finishing after removing unused shared RGB saturation. Native smoke captures confirm the three headers, visible tonal group, monochrome inactivity and compact readouts. The release bundle was refreshed without replacing the running editor session.
