---
summary: "Review the combined editor, browser and Merrill changes and cut the signed 0.6.0 release."
---

# Release 0.6.0 final review

The owner requested cleanup of all combined changes and a release. Follow [application releases](../docs/specs/releases.md), [saturation](../docs/specs/saturation.md), [direct color editing](../docs/specs/direct-color-editing.md), [browser previews](../docs/specs/browser-previews.md) and [Merrill speckles](../docs/specs/merrill-speckles.md).

- [x] Review the outstanding changes against existing UI, worker, recipe and GPU ownership.
- [x] Correct integration issues and retire stale exploratory or release guidance.
- [x] Update versioned changes to match the complete release rather than the earlier candidate.
- [x] Run workspace checks, focused GPU/original-file checks and native smoke validation.
- [x] Commit reviewed work, push the release tag and verify the hosted signed assets and Sequoia startup check.
- [ ] Complete the release handoff following the owner publication workflow.

Version 0.6.0 is already assigned but has no application tag or GitHub release. The prior release candidate predates noise-profile training, Detail refinements, independent saturation, thumbnail memory controls and direct color editing. Its archived validation does not cover this final combined release.

## Frame-path review

The destination warning originally called `is_dir` while drawing each toolbar frame. Directory checks now use the existing batch worker with one replaceable pending path, a bounded result and a two-second refresh. Results for another destination are ignored. No new thread or photographic stage was added.

Import completion now rebuilds visibility only when history or receipts change. Thumbnail memory is charged on insert, replacement and eviction instead of recounting every texture each frame. The duplicate toolbar sidecar refresh was removed.

Comparison view previously disabled the active photo's interaction along with ordinary navigation. Its active photo now accepts direct color gestures through the same availability gate as single-photo editing. Other photos retain focus-selection behavior and ordinary comparison navigation stays on the canvas.

The audit traced recurring metadata, ratings, sidecar, cache-readiness, calibration-availability, navigator and preview jobs to their background workers. Their I/O and GPU waits occur outside UI-owned request locks. Window color tagging checks the native layer, reusing its color space and updating only a replacement layer. Presets remain in-memory application data. Preference persistence and flushing pending sidecar/rating writes are save/shutdown operations. Folder opening still discovers paths synchronously on the explicit open action. Large or slow directory trees can pause that action. Moving folder discovery off-thread is a separate follow-up.

## Local validation

All 713 ordinary workspace tests pass, along with strict all-target Clippy, formatting, documentation indexes and the five artwork checks. Forty-four selected GPU/original-file checks cover perceptual saturation, Color Range selection and HSL, clipped speckles, Directional Banding, Detail weak-color protection, profile training/application, photographic versus DNG reprojection, section bypasses and worker picking. The destination probe check exercises disappearance, reappearance and switching paths without drawing-thread I/O.

The local bundle and executable report 0.6.0 and target macOS 15.0 on Apple Silicon. Command-line startup passes. Interactive macOS 15 and third-party raw-editor appearance remain separate photographic/runtime review tasks.

Native smoke checks pass in both single-photo and eight-photo comparison views. Both exercise simultaneous Hue/Lightness dragging, affected swatches, right-click configuration and Escape. Preferences and captures are isolated, with source sidecar writes disabled. The local review bundle passes ad-hoc signature verification. Distribution signing remains the hosted workflow's responsibility.

## Hosted release

Commit `548dea0` and annotated tag `v0.6.0` are pushed. The [release workflow](https://github.com/hank-bond/foveonic/actions/runs/41455313438) passed its complete checks, Developer ID signing, signature verification, asset upload, credential cleanup and signed-app startup on macOS 15. The release draft contains the 0.6.0 changelog section, Apple Silicon ZIP and SHA256SUMS.

The downloaded archive passes its checksum and strict signature verification. Its signature includes hardened runtime and a secure timestamp. Version, Apple Silicon architecture and macOS 15.0 deployment target match the release. The downloaded executable starts and passes the isolated native eight-photo comparison editing smoke test. It is signed and unnotarized as specified. Browser-quarantine acceptance and interactive macOS 15 rendering were not tested by these checks.

Public publication and Latest selection await the owner's final decision under the documented release workflow.
