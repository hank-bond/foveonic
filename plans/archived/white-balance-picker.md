---
summary: "Implement and verify a shared neutral-patch white-balance eyedropper and saved Custom balance."
status: archived
---

# White balance picker

This slice implements the accepted [picker requirements](../../docs/specs/white-balance-picker.md) and [workspace status](../../docs/specs/workspace-status.md). The owner reviewed Custom balance, panel placement and message handling, then approved cleanup and grouped commits.

- [x] Inspect the shared white-balance stage, worker pick queue and external method evidence.
- [x] Add a GPU patch statistic and saved Custom neutral through the existing transform.
- [x] Connect the panel, release gesture, stale-result checks and shared tool routing.
- [x] Route tool communications through one acknowledged toolbar status and session log.
- [x] Verify numerical behavior, persistence, camera families, DNG appearance and native layout.
- [x] Remove redundant notices and tests, separate numerical sampling from image presentation, and review for commit.

## Design and review findings

Darktable samples a neutral patch and derives channel ratios. The private numerical study confirms channel-gain application and exposes picker entry points, without a validated picker body. No external implementation code is imported. Bradford adaptation extends the existing shared transform in Rust/WGSL. The picker retains its camera preset as the calibration anchor, saves a normalized D50 XYZ neutral, and resets independent Warmth/Tint adjustments. Selecting a preset replaces Custom.

The initial 0.98-times-nominal-white rejection falsely classified linear headroom as clipping. The corrected sampler accepts finite positive headroom. Sampling remains at the existing linear base before shared edits. Native sensor-clipping provenance and a sampling boundary before highlight recovery are separate design questions, not changes introduced by this slice.

White Balance and Dehaze publish typed events when their validated result is consumed. Color Range retains typed feedback with its pending gesture, checks settings before publication and discards stale results. This removes text-based event classification and cached notice strings. Review caught a real delivery bug in the earlier polling approach: a reply could change the notice before the next frame took its comparison snapshot. A regression now covers delivery, acknowledgement, no replay and stale settings.

GPU sampling receives only numerical geometry, primaries, code scale and storage. Engine presentation owns image validation and coordinate mapping. All three pickers share crop/orientation mapping. Controls reuse shared square-button geometry and match the Mode selector's measured height. No additional render waits or UI-thread GPU work were added.

## Validation

Numerical checks cover sRGB, Adobe RGB and ROMM neutralization, sampled luminance, settings roundtrip and scoped recipe application. GPU fixtures cover four orientations, retained input ownership, dark/NaN/infinite rejection and valid nominal-white/headroom samples. Bright-patch regressions identify 5×5 regions rejected by the old cutoff on the Incandescent blue sign and SD10, then verify successful selection. DNG checks pass on Merrill, ordinary Quattro and Classic, preserving technical samples while matching Custom and fine adjustments to photographic appearance.

Native white-balance, status and direct-color checks use isolated preferences and disable source sidecar access. They check picking and reset, error acknowledgement and shared message placement without panel layout shifts. Workspace, strict all-target Clippy, formatting, documentation and artwork checks pass. The app is rebuilt and reopened for owner review. Comparison-image work remains outside these commits.
