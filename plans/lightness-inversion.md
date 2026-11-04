---
summary: "Prototype a saved, exportable lightness inversion that preserves color components and keeps Monochrome achromatic."
---

# Lightness inversion prototype

The owner requested luminance inversion with retained chroma and selected a saved/exportable editing effect. In Monochrome it must only invert tones.

The concrete prototype contract is recorded in [Lightness inversion](../docs/specs/lightness-inversion.md).

Use a default-off Invert lightness checkbox in Tone, available with Reference and Universal. The first prototype reverses Oklab lightness around nominal exposed black/white while retaining its two color components. Monochrome uses the corresponding scalar response and remains exactly achromatic. Preserve signed samples and headroom for subsequent camera rendering and display gamut handling. Appearance and placement remain for photographic feedback.

- [x] Add one shared GPU operation after ordinary linear edits and before camera appearance.
- [x] Carry the checkbox through saved settings, Tone presets/defaults, bypass, CLI and processing sections.
- [x] Bake the edited RGB through the existing DNG basis mapping, including Monochrome and exact black.
- [x] Check mathematical endpoints, retained color components, grayscale, working spaces and code/exposure scales.
- [x] Check cached/fresh, saved settings, photographic/DNG paths and the native UI.
- [x] Build and open the prototype for owner review without committing or publishing it.

Retaining the color components can produce colors outside a destination gamut near black or white. Keep the arithmetic unclipped and retain the existing export/display handling. This is not RGB complement inversion or a new denoising operation.

Classic's pre-response monochrome signal is in green, while Merrill and Quattro replicate it. The grayscale branch reads that signal and writes equal RGB channels. Classic's monochrome DNG storage transform can be singular. Its neutral-axis scale still defines how to bake newly generated gray values without a full matrix inverse or a gain from black.

All 716 ordinary workspace tests, strict all-target Clippy, formatting, documentation indexes and five artwork checks pass. GPU checks verify fixed chromatic components, two inversions restoring the input, signed/headroom samples, all three working spaces, code/exposure scales, exact grayscale endpoints and immutable input. An original-file test verifies Color and Monochrome DNG reprojection plus cached/fresh toggling. Native captures verify the checkbox and both renderings on the speaker photograph. The clean-copy native run avoids an existing legacy experimental sidecar beside the research original. That sidecar was not changed.

Actual CLI writes produce a 320×480 RGB16 TIFF with an ICC profile and a 3136×4704 RGB16 DNG with version tags. Sampled stored DNG pixels retain equal RGB channels with nonconstant tones. Third-party editor appearance remains a photographic review task. The application bundle is rebuilt. Response and placement remain for owner feedback before any release.
