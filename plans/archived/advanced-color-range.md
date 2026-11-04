---
summary: "Build and evaluate an Advanced Color Range wheel, saved HSL edit list."
status: archived
---

# Advanced color range implementation

Implement the agreed [prototype](../../docs/specs/advanced-color-range.md), informed by the separate [research](../../docs/advanced-color-range-research.md).

- [x] Add bounded saved range controls and one GPU adjustment pass using the shared color guide.
- [x] Connect section scopes, inspection, caching and TIFF/DNG baking.
- [x] Add the wheel, edit list and asynchronous picker alongside Basic.
- [x] Keep selection preview transient and exclude it from browser previews and exports.
- [x] Verify selection boundaries, neutral protection, overlap, Monochrome, saved settings and export baking.
- [x] Build and open the prototype for photographic review without committing or publishing.

The owner removed selective lightness inversion after photographic review. The owner subsequently removed global Tone inversion too.


The implementation uses a fixed table of at most 30 ranges, serialized as the used entries only. The graph boxes the larger uniform table so unrelated operation records stay small. Advanced selections share the established guide basis and a common GPU coordinate function with the picker. One pointwise pass accumulates HSL amounts from immutable guide coordinates. Hue uses the existing ±30-degree scale, saturation uses the shared gamut-aware shoulder and clipping policy described in [Saturation](../../docs/specs/saturation.md#shared-clipping-protection), and positive Lightness lifts the selected range.

Selection preview is a transient rendering request excluded from serialization, recipe snapshots, GUI exports, DNG baking and browser-preview publication. Basic and Advanced share a typed worker pick request/reply with 48 bytes of GPU selection data. The active tool rejects stale photo, recipe and generation replies. A shared eyedropper glyph uses the ordinary icon-button geometry.

Checks cover compact bounded serialization, section scopes, preview exclusion, stale picks, hue-handle dragging through red, neutral protection, untouched pixels, combined lifts, Monochrome and preserved Oklab color components across three working spaces and exposure/code scales. Original-file checks verify DNG reprojection and cached toggles in Color and Monochrome, plus Full/Fast/regional processing. Native input replay creates a range from the photograph and captures range edits, selection preview and Monochrome. Photographic review of sky transitions and usefulness remains with the owner. Changes are uncommitted.

Basic and Advanced now share one gesture handler. Toolbar activation no longer immediately disarms Basic on the same mouse-up. Advanced retains diagonal HSL movement and frozen delayed sampling through pointer release. Empty Advanced panels show the wheel without authoring a range. Its right-aligned toolbar reads +, −, eyedropper.

Advanced Lightness changes Oklab L while retaining a/b, so it can change RGB ratios even when Hue and Saturation are zero. Its Color DNG route now bakes the full RGB transform. Monochrome remains achromatic and can transfer the scalar gain. Original-file regression checks cover Color and Monochrome Advanced lightness.

Current validation passes workspace tests, strict Clippy, formatting, documentation indexes and artwork checks. Original-file GPU checks cover Advanced Full/Fast/regional previews and DNG reprojection in Color and Monochrome. Native replay verifies the empty wheel and simultaneous Advanced Hue/Lightness dragging with release restoring navigation.

Basic/Advanced now share an inline title-row button group. Each entry has a bounded UTF-8 name, preserved by ordinary recipe serialization and scoped copying but excluded from GPU parameters. Double-click name editing commits on Enter or blur. Per-row eye buttons provide exclusive transient preview and replace the global preview checkbox. Browser and range eyes share one glyph implementation.

Native replay commits a renamed entry with Enter, verifies that its image-request revision stays unchanged, and captures named, previewed and Monochrome entries. Unit checks cover double-click editing, click-away commit, exclusive/off eye toggles, saved-name bounds and round trips, deletion, scoped copying, unchanged GPU parameter keys, header alignment and popup width.

The inline mode group uses Advanced when it fits and Adv. on narrower panes, without a second-line fallback. A shared helper measures the padded button against the available width, retaining the header action space. Basic swatches now use a centered shared button group, and the Advanced wheel centers horizontally.

Empty Advanced lists retain disabled adjustment controls and place the picking prompt in the selection area below the divider. The list reserves two control rows so adding the first range keeps the controls and action toolbar stationary.

The owner accepted the app-wired behavior for release closeout. Subsequent structural cleanup is recorded in [release coherence](./release-coherence.md).
