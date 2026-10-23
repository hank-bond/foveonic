---
summary: "An optional Quattro Fast preview reduces rendering resolution while preserving Full exports and native viewport coordinates."
kind: reference
status: accepted
implementation: conforming
---

# Quattro Fast preview

The owner reviewed the six-image Full/Fast comparison, found both appearances good, and approved integrating the toggle into the app. Supported Quattro color photographs include ordinary Quattro and Tiger. The shared [Smart policy](./smart-preview.md) owns progressive refinement and regional Full rendering.

## Requirements

- The viewport header must show two separate button groups, with Preview to the left of Inspect. The Preview label must be vertically centered with its Full/Fast buttons. The app must offer Full and Fast preview choices. [Smart preview](./smart-preview.md) adds a separate progressive policy. Full is the initial default. The choice is a persistent workspace preference, separate from per-photo editing sidecars.
- Fast must retain HIGH calibration identity, average calibrated blue 2×2 onto the existing R/G grid, use same-resolution reconstruction and adjust spatial finishing to that grid. It may omit the high-resolution blue repairs as demonstrated in the reviewed experiment. Tiger must retain blue phase-detection correction before averaging to prevent visible sensor-site dots. RGB repairs remain enabled.
- Fast normally produces half the full width and height. Full and Fast must preserve photographic framing and orientation. Switching quality must preserve the viewport's native-image zoom and pan, including the focused region after double-click zoom. The information bar must continue to use native photograph coordinates. Viewport scopes must sample the corresponding region of the actual rendered image.
- Editing controls must operate in either preview quality. Active-edit caches must distinguish quality-dependent parameters, geometry and dependencies. Retained images must stay immutable through quality switches.
- Fast applies to supported Quattro color final previews. Inspection, input comparisons, monochrome and unsupported families use Full. The UI must communicate this availability and identify the displayed quality. Preserve the application-wide choice during loading and resolve unsupported architectures from capture metadata without overwriting that preference.
- GUI TIFF and DNG exports must use Full and current requested photographic edits, independently of the preview preference. Completing export must restore the requested preview quality.
- Source decoding and rendering must remain on the bounded background worker. Existing image-first publication, cancellation, scope pairing and navigator behavior remain applicable.

## Validation

Check Full against the committed complete photographic baselines. Check Fast against the reviewed prototype with any numerical differences documented. Compare fresh and cached Fast results across edits and mode switches. Exercise portrait and landscape views, viewport scopes, preference persistence, sidecar exclusion, Full exports under a Fast preference and the native GUI toggle. The [performance plan](../../plans/archived/quattro-performance-and-fast-preview.md) retains the experiment and implementation evidence.

## Architecture

The Quattro recipe must own camera eligibility, calibration identity and working-grid selection. Source metadata and decoded samples remain immutable. Shared operations receive numerical geometry and controls without camera-family or GUI policy branches. Spatial finishing must distinguish physical coordinates from the reduced working grid. Numerically significant preview constants must name their purpose and distinguish recovered reference controls from the reviewed Fast approximations.
