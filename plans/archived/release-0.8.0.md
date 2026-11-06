---
summary: "Version 0.8.0 has a verified signed draft covering white balance, rotation and reduced RAW support."
status: archived
---

# Release 0.8.0

Follow the [release requirements](../../docs/specs/releases.md) and [release guide](../../docs/releases.md). The owner requested a minor release after reviewing and committing reduced RAW support. Version 0.8.0 covers committed changes since 0.7.2. Unrelated help-example and previous release working changes are outside this release. The signed workflow creates an editable draft for owner review. Publication remains separate.

- [x] Prepare version, notes and release commit.
- [x] Run workspace, documentation, artwork, release-note and local bundle checks.
- [x] Push the release commit and annotated tag and verify the hosted signed draft and macOS 15 startup job.
- [x] Verify downloaded assets and the signed native app.
- [x] Provide the editable draft link for owner review.

Reduced Classic and Quattro originals remain unverified. Preserve that validation limit in the changelog. Merrill LOW/MED use validated originals. Interactive macOS 15 GPU behavior and browser-quarantine acceptance remain separate from hosted process-startup verification.

## Local checks

Version 0.8.0 uses the existing application bundle identity. Workspace tests, strict Clippy, formatting, documentation indexes, five artwork checks and release-note extraction pass. The optimized local bundle reports version 0.8.0, Apple Silicon arm64 and macOS 15.0 in bundle metadata and executable load commands. Its help command runs. Owner review confirmed smooth interaction in the optimized build. Reduced RAW and framing checks are recorded in the [implementation review](./reduced-raw-review.md).

## Verified draft

Release preparation commit `522d366` and annotated tag `v0.8.0` were pushed. Both jobs in the [hosted release workflow](https://github.com/hank-bond/foveonic/actions/runs/42136038634) passed, including signed executable startup on macOS 15.

The downloaded archive matches SHA256SUMS with SHA-256 `f75791c7da62ae86cb4ff9b81b577b8473509ffac07afea25c1ab239294a8f51`. Strict Developer ID signature verification passes for team `UBLDN6TFD6`, with hardened runtime and a secure timestamp. Bundle and executable report version 0.8.0, arm64 and macOS 15.0 minimum. Command-line help runs.

The downloaded signed app completes an isolated native GUI sequence on the provided DP3 Merrill MED original on macOS 26.6.2. It covers final rendering, zoom, histogram/parade/waveform, and sensor/calibration inspection. Preferences remain isolated and source sidecar writes are disabled. Interactive macOS 15 GPU behavior and browser-quarantine acceptance remain unverified.

The [editable draft](https://github.com/hank-bond/foveonic/releases/tag/untagged-6304e1ff88345c111336) contains the signed application archive and checksum. Existing draft prose was preserved during upload. The release is not published. Owner publication remains a separate action. Unrelated help-example and earlier-release edits remain untouched.
