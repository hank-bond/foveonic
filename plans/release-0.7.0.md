---
summary: "Prepare, verify and publish the reviewed Adaptive, export, crop and workspace release as 0.7.0."
---

# Release 0.7.0

Follow [application releases](../docs/specs/releases.md) and the [release guide](../docs/releases.md). The owner reviewed the combined application changes, requested cleanup and grouped commits, then authorized cutting the release. This includes the release commit, tag, hosted signed build, asset verification and publication as Latest.

- [x] Prepare version 0.7.0 and notes covering the complete changes since 0.6.0.
- [x] Run workspace, documentation, artwork and tagged bundle checks.
- [ ] Commit and push the release preparation and annotated version tag.
- [ ] Verify hosted signing, credential cleanup and macOS 15 startup checks.
- [ ] Download and verify the release archive, then run an isolated native smoke check.
- [ ] Publish the verified release as Latest and record the evidence.

The reviewed changes include Adaptive tone, Dehaze, shared saturation protection, advanced colour ranges, dynamic exposure compensation, denoised Fast/Smart previews, additional photographic output formats, saved cropping, shared UI layout, a session activity log and the final Merrill control refinements. Previous experimental tone paths and retired alpha settings readers are removed. Notes must identify that settings compatibility limit without claiming a migration.

JPEG XL links statically in the local bundle. Its executable has only system dynamic-library dependencies. Verify the hosted archive likewise before publication. Interactive macOS 15 rendering and browser-quarantine acceptance remain outside the process-startup check.

Local checks pass for workspace tests, strict Clippy, formatting, documentation indexes, artwork and changelog extraction. The tagged bundle reports 0.7.0, arm64 and macOS 15.0 in its executable and bundle metadata. Its command-line startup and isolated native status/error-acknowledgment smoke check pass.
