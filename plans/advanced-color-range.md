---
summary: "Build and evaluate an Advanced Color Range wheel, saved edit list and selective lightness inversion."
---

# Advanced color range prototype

Implement the agreed [prototype](../docs/specs/advanced-color-range.md), informed by the separate [research](../docs/advanced-color-range-research.md).

- [x] Add bounded saved range controls and one GPU adjustment pass using the shared color guide.
- [x] Connect section scopes, inspection, caching and TIFF/DNG baking.
- [x] Add the wheel, edit list and asynchronous picker alongside Basic.
- [x] Keep selection preview transient and exclude it from browser previews and exports.
- [x] Verify selection boundaries, neutral protection, overlap, Monochrome, saved settings and export baking.
- [x] Build and open the prototype for photographic review without committing or publishing.

Partial inversion flattens contrast around half coverage. Review feathered sky boundaries and noise before treating the response as final.


The implementation uses a fixed table of at most 30 ranges, serialized as the used entries only. The graph boxes the larger uniform table so unrelated operation records stay small. Advanced selections share the established guide basis and a common GPU coordinate function with the picker. One pointwise pass accumulates HSL amounts and caps inversion coverage. Hue uses the existing ±30-degree scale, saturation uses the existing direct chroma response, and Lightness follows inversion so its positive direction remains a lift.

Selection preview is a transient rendering request excluded from serialization, recipe snapshots, GUI exports, DNG baking and browser-preview publication. Basic and Advanced share a typed worker pick request/reply with 48 bytes of GPU selection data. The active tool rejects stale photo, recipe and generation replies. A shared eyedropper glyph uses the ordinary icon-button geometry.

Checks cover compact bounded serialization, section scopes, preview exclusion, stale picks, hue-handle dragging through red, neutral protection, untouched pixels, inversion overlap, Monochrome and preserved Oklab color components across three working spaces and exposure/code scales. Original-file checks verify DNG reprojection and cached toggles in Color and Monochrome, plus Full/Fast/regional processing. Native input replay creates a range from the photograph and captures inversion, selection preview and Monochrome. Photographic review of sky transitions and usefulness remains with the owner. Changes are uncommitted.
