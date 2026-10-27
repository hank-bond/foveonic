---
summary: "Shared eight-range Color Editor, saved-edit migration and chromatic DNG baking are ready for photographic review."
status: archived
---

# Implement Color Editor

Requirements: [Color Editor](../../docs/specs/color-editor.md) and [photographic finishing](../../docs/specs/photographic-finishing.md).

- [x] Commit the reviewed pipeline cleanup separately.
- [x] Rename the shared node and inspection outputs to Color Editor.
- [x] Add eight-range HSL controls and migrate saved six-color brightness coefficients.
- [x] Add shared swatch UI with three compact sliders and transient selection.
- [x] Retain Monochrome sensor mixing and its Lightness response.
- [x] Implement chromatic DNG baking using the inverse existing profile basis.
- [x] Verify numerical, persistence, UI, cache, region and DNG contracts.
- [x] Run repository checks and build the optimized review app.

The existing DNG gain transfer is restricted to scalar edits. Chromatic baking instead maps the selected pre-tone result back through the profile's linear sample interpretation. The reader restores that interpretation once. This uses the existing numerical camera-profile adapters and does not require an inverse tone curve. Selected nonlinear reconstruction is baked on that route. Reference HSL numerical equivalence is not claimed.


## Checks and remaining review

The four GPU editor fixtures pass neutral/signed/headroom checks across sRGB, Adobe RGB and ROMM, shared Lightness between Color and Monochrome, and isolated selection of all eight ranges. The isolation fixture caught reciprocal/log rounding in neutral Lightness; zero range amounts now produce exact zero gain. Lightness-only processing retains scalar scratch buffers. Only active chromatic edits allocate the wider control map.

Original-file reprojection checks pass on Merrill, Classic, Quattro and Quattro H. DNG samples transformed through the profile basis reproduce the same edited pre-tone RGB within Float32 tolerance. DNG ignores transient preview crop, quality and output size. Spatial checks on Merrill, Quattro and Quattro H cover Hue/Saturation together with Lightness, HDR, Local Contrast and sharpening; Full/viewport results agree and Smart continuation reuses sensor work. Cached/fresh Color and Monochrome results agree on the Merrill portrait.

Workspace tests, strict Clippy, formatting, documentation indexes and artwork checks pass. UI tests cover swatch selection without edits/renders, retained values across rendering-mode changes and three-significant-digit readouts. The optimized review bundle builds. These are numerical and integration checks; photographic response and downstream-editor appearance remain for owner review. The HSL work remains uncommitted.
