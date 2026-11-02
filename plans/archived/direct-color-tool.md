---
summary: "Added guide-based color picking, simultaneous HSL dragging and persistent gesture preferences."
status: archived
---

# Direct color tool

Implement the owner-requested on-image Color Range interaction. Use the retained local reference implementation only to understand selection, gesture routing and control updates. Record findings separately from the selected behavior.

- [x] Trace and retain bounded cursor-method decompilation with evidence limits.
- [x] Add lazy GPU picking from the existing neutral color guide with shared range weights.
- [x] Add the Color Range tool button and D/Escape shortcuts, fixed selection during dragging, and immediate slider feedback.
- [x] Use horizontal Hue, upward Lightness, and Option + upward Saturation, preserving ordinary navigation outside the tool.
- [x] Reject stale asynchronous picks and photo/recipe changes. Reuse ordinary edit persistence and DNG processing.
- [x] Verify photographic sampling, axis mapping, overlap, neutral selection, geometry, responsiveness and native interaction.

Requirements live in [direct color editing](../../docs/specs/direct-color-editing.md). This is an interaction addition, not a new pixel adjustment or a new color-selection model.

The owner clarified that both axes must remain available in one held gesture. The implementation accumulates independent horizontal and vertical contributions, including live Option changes, against the initial controls. It retains a fixed sampled selection. Configurable axis mappings and sensitivity are application preferences. Gesture state stays transient.

Checks passed the workspace tests, warning-free Clippy, formatting, documentation checks and artwork tests. Six Color Editor GPU tests verify unchanged photographic response, overlapping selections, neutral protection and crop-clamped sampling. A separate GPU test covers cropped coordinates at all four rotations and a 32-byte readback. Original-file worker checks cover neutral/active edits, reduced preview choices, As Shot and Monochrome. Native input replay captures D activation, a diagonal drag changing Hue and Lightness together, affected swatches, the right-click settings and Escape. Picking prepares the native guide once per press and transfers eight weights. It adds no work to ordinary renders or pointer movement. The unoptimized original-file test measured approximately 0.3–0.8 seconds per pick including guide preparation. This is not an isolated release latency benchmark.

The release bundle was refreshed. The local research archive retains bounded application methods, selector annotations, slice-matcher pseudocode and binary identity. Static findings and limitations are summarized in the [research catalog](../../docs/color-and-tone-research.md#direct-color-cursor-inspection).
