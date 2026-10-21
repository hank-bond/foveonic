---
summary: "Separate family editing panels and centralize processing control constraints before reviewing broader organization."
status: archived
---

# Family editing panels

Implement [family editing controls](../../docs/specs/family-controls.md). Keep this change focused on GUI composition and the shared processing control contract. Broader Classic preparation and graph organization remain a later review.

- [x] Share processing-owned capabilities between normalization and request validation.
- [x] Separate Classic and Merrill panel composition and reuse common widgets.
- [x] Preserve camera preset choices, pending metadata and compatible saved settings.
- [x] Verify family layouts, interactions, defaults and processing constraints.
- [x] Run workspace and documentation checks, then review the resulting boundaries.

## Result

The GUI dispatcher selects a Classic or Merrill composition function. Classic omits unsupported controls. Merrill preserves existing widget scopes and behavior. Shared rows implement presets, tone, refinement and correction toggles. The processing-owned ControlCapabilities contract normalizes incompatible saved edits and validates Classic requests. Effective sharpening defaults remain in the shared Settings resolver used by both GUI and processing.

All 305 ordinary workspace tests pass, including family layout, pointer interaction, metadata changes and individual unsupported adjustment checks. The release app bundles successfully. Broader recipe preparation and graph-contract organization remain unchanged for the next owner review.

## Early camera identification

The panel originally waited for the render reply's full metadata, although the capture-properties reader had already identified the camera for the viewport information bar. Family selection and compatibility normalization now use that same lightweight result until full metadata arrives. An unidentified image shows shared controls rather than a temporary Merrill panel. A capture-only regression covers the Classic panel before any rendered frame.
