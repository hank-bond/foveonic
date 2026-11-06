---
summary: "Prepare and verify the 0.8.0 white balance, rotation and reduced RAW draft release."
---

# Release 0.8.0

Follow the [release requirements](../docs/specs/releases.md) and [release guide](../docs/releases.md). The owner requested a minor release after reviewing and committing reduced RAW support. Version 0.8.0 covers committed changes since 0.7.2. Unrelated help-example and previous release working changes are outside this release. The signed workflow creates an editable draft for owner review. Publication remains separate.

- [x] Prepare version, notes and release commit.
- [x] Run workspace, documentation, artwork, release-note and local bundle checks.
- [ ] Push the release commit and annotated tag and verify the hosted signed draft and macOS 15 startup job.
- [ ] Verify downloaded assets and the signed native app.
- [ ] Provide the editable draft link for owner review.

Reduced Classic and Quattro originals remain unverified. Preserve that validation limit in the changelog. Merrill LOW/MED use validated originals. Interactive macOS 15 GPU behavior and browser-quarantine acceptance remain separate from hosted process-startup verification.

## Local checks

Version 0.8.0 uses the existing application bundle identity. Workspace tests, strict Clippy, formatting, documentation indexes, five artwork checks and release-note extraction pass. The optimized local bundle reports version 0.8.0, Apple Silicon arm64 and macOS 15.0 in bundle metadata and executable load commands. Its help command runs. Owner review confirmed smooth interaction in the optimized build. Reduced RAW and framing checks are recorded in the [implementation review](./archived/reduced-raw-review.md).
