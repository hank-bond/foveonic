---
summary: "Add a camera-relative highlight threshold while preserving default exports."
---

# Adjust the highlight restoration threshold

Requirements: [highlight restoration](../docs/specs/highlight-restoration.md) and [editing sidecars](../docs/specs/editing-sidecars.md).

## Steps

- [x] Add the optional EV offset, validation, CLI parsing, preparation, and change tracking.
- [x] Add the automatic/manual GUI row, enable checkbox, and help.
- [x] Test defaults, sidecars, pointer input, GPU arithmetic, and cached/fresh renders.
- [x] Check default exports, workspace tests, strict Clippy, release builds, and documentation.

## Findings and checks

Auto follows the camera threshold. Manual −2 to +2 EV scales it by `2^EV`; 0 matches Auto and +1 doubles it. Only neutralization onset changes. Donors, blue correction, transition width, and original saturation flags are unchanged. Disabling restoration preserves the threshold choice. Older sidecars omit the field and use Auto. TIFF and DNG technical processing share the setting.

All seven default TIFFs match the clean `7742f3a` build byte-for-byte. Manual +1 changes the flower export. Host tests verify parameter isolation and manual-zero identity. Existing native component fixtures pass; no new native full-sensor capture was made.

GPU tests cover offsets and gains 0/1/1.3, density-only inspection, immutable inputs, and cached/fresh flower edits with reconstruction off/on. The six-photo interactive regression and manual-threshold sidecar replay pass. DNG tests verify the correction is baked into technical samples.

GUI tests cover enablement, retained values, Auto/manual switching, dragging, reset, typing, and nudging. Workspace tests, strict all-target Clippy, release builds, formatting, whitespace, and explicit-root documentation checks pass. Evidence is retained in `.local/highlight-threshold/`. Photographic review remains separate from these checks.
