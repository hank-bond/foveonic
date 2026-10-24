---
summary: "Bypass optional Merrill Noise and Detail in Fast while preserving requested controls for Full."
status: archived
---

# Merrill Fast Noise and Detail

The owner requested Noise and Detail off in Smart's Fast overview and on for Full refinement. The [Merrill Fast specification](../../docs/specs/merrill-fast-preview.md) preserves the requested checkbox and controls, so Full honors an intentionally disabled checkbox too. Standalone Fast shares the same recipe policy.

- [x] Bypass optional reconstruction and ColorDQ only in reduced photographic rendering.
- [x] Remove unused reduced noise parameters and their obsolete geometry constraint.
- [x] Update the comparison tool's explanation to match the current recipe.
- [x] Verify actual GPU stages, saved controls, immutable cached refinement and native inspection/DNG geometry.
- [x] Exercise native Smart refinement with Noise and Detail enabled and verify Full TIFF/DNG exports.
- [x] Run workspace, formatting, strict Clippy, documentation and artwork checks.

Calibration, repairs, highlights and requested despeckling retain their existing native behavior. Separate sharpening remains enabled when requested. Full, inspection, monochrome and exports keep their existing processing. The earlier comparison's timings describe the previous reduced reconstruction recipe and are historical evidence, not measurements of this bypass.

The original-file GPU test confirms omitted submissions, unchanged encoded sidecars and exact fresh/cached Full restoration. Fast matches the same reduced render with Noise and Detail explicitly disabled. Explicit ColorDQ overrides also remain bypassed. TIFF and DNG worker exports match independent Full files with reconstruction enabled for both Merrill and Quattro.

Native app checks exercise Smart at Fit, 100%, 200%, panning, editing and cache reopening with Noise and Detail enabled. Merrill preserves Fast through 100% and delivers whole-frame Full above it. Panning retains the same Full image revision. Quattro still delivers its viewport tile before whole-frame Full. All 527 ordinary workspace tests, five artwork tests, targeted GPU/export checks, strict Clippy, formatting and documentation checks pass. The native app bundle is rebuilt. No source sidecars or installed app are changed by these isolated checks.
