---
summary: "Separate quarter-resolution Fast controls from Smart's reconstruction-bypassing Draft recipe before updating the release draft."
---

# Fast preview controls

The owner requested that standalone Fast honor Noise and Detail at quarter resolution, while Smart may keep its lighter overview. The owner approved Draft as the recipe name. Implements [Merrill previews](../docs/specs/merrill-fast-preview.md) and [Smart previews](../docs/specs/smart-preview.md).

- [x] Separate Fast and Draft at the recipe boundary while preserving saved edits.
- [x] Map Merrill noise geometry to the reduced grid and retain Quattro's existing noise controls.
- [x] Verify actual GPU stages, reduced pixels, cache reuse and immutable Full/Draft results.
- [x] Run repository checks and isolated native Fast/Smart checks.
- [ ] Update the unpublished 0.4.0 draft, preserve owner notes and verify its replacement signed archive.

Fast and Draft share half-width, half-height working geometry. Fast runs requested noise reconstruction and ColorDQ on that grid. Camera-coordinate bounds map to the reduced samples, while filtering support remains in working pixels. Native source metadata and saved controls remain unchanged. Merrill row correction continues to be omitted for reduced rendering as previously agreed. Quattro's mandatory reconstruction and noise controls are already active in reduced rendering and remain unchanged.

The photographic GPU check confirms Fast submits the requested reconstruction and ColorDQ stages while Draft omits them. Reduced results remain finite, controls change the pixels, cached and fresh Fast agree, and earlier Full, Fast and Draft images remain immutable. Sidecar encodings preserve the requested controls. Workspace tests, strict Clippy, formatting, documentation and artwork checks pass. The local tagged bundle completed seven Fast and twelve Smart native checkpoints, including Draft overview, viewport detail, Full continuation and cache reopening.
