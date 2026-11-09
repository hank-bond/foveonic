---
summary: "An optional Quattro Fast preview reduces rendering resolution while preserving Full exports and native viewport coordinates."
kind: reference
status: accepted
implementation: conforming
---

# Quattro Fast preview

The owner reviewed the initial six-image Full/Fast comparison and approved the application toggle. The later [small-preview contract](./small-previews.md) extends this feature to numeric working grids and complete selected processing. It also owns the LOW-style Draft/Thumb choice. Ordinary Quattro and Tiger share these requirements. The [Smart policy](./smart-preview.md) owns progressive refinement and regional Full rendering.

## Requirements

- The viewport header must show two separate button groups, with Preview to the left of Inspect. The Preview label must be vertically centered with its Full/Fast buttons. The app must offer Full and Fast preview choices. [Smart preview](./smart-preview.md) adds a separate progressive policy. Full is the initial default. The choice is a persistent workspace preference, separate from per-photo editing sidecars.
- Fast must retain the source calibration identity. HIGH-origin Fast keeps its current two-resolution color/blue reconstruction, with each component reduced by divisor two and the native blue repair history preserved. Native LOW retains its equal-resolution source representation. Draft and Thumb use the LOW-style working representation specified in small previews. The initial same-resolution Fast experiment remains historical evidence rather than the current Fast numerical contract.
- Fast normally produces half the full width and height. Full and Fast must preserve photographic framing and orientation. Switching quality must preserve the viewport's native-image zoom and pan, including the focused region after double-click zoom. The information bar must continue to use native photograph coordinates. Viewport scopes must sample the corresponding region of the actual rendered image.
- Editing controls must operate in either preview quality. Active-edit caches must distinguish quality-dependent parameters, geometry and dependencies. Retained images must stay immutable through quality switches.
- Reduced Quattro previews must support Color and Monochrome and the native RAW layouts covered by small previews. Inspection and input comparisons use their authoritative native outputs. The UI must identify the displayed quality. Preserve the application-wide choice during loading without overwriting it from capture metadata.
- GUI TIFF and DNG exports must use Full and current requested photographic edits, independently of the preview preference. Completing export must restore the requested preview quality.
- Source decoding and rendering must remain on the bounded background worker. Existing image-first publication, cancellation, scope pairing and navigator behavior remain applicable.

## Validation

Check Full and Fast against frozen preceding-build photographic baselines, with any numerical differences documented. Compare fresh and cached results across edits and mode switches. Exercise portrait and landscape views, viewport scopes, preference persistence, sidecar exclusion, Full exports under a reduced preference and the native GUI controls. The [initial performance plan](../../plans/archived/quattro-performance-and-fast-preview.md) retains the original experiment and implementation evidence. The [completed Quattro migration](../../plans/archived/quattro-early-reduction.md) records the later representation and timing comparisons.

## Architecture

The Quattro recipe must own camera eligibility, calibration identity and working-grid selection. Source metadata and decoded samples remain immutable. Shared operations receive numerical geometry and controls without camera-family or GUI policy branches. Spatial finishing must distinguish physical coordinates from the reduced working grid. Numerically significant preview constants must name their purpose and distinguish recovered reference controls from the reviewed Fast approximations.
