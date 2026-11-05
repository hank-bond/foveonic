---
summary: "Prepare, verify and publish the reviewed Adaptive, export, crop and workspace release as 0.7.0."
---

# Release 0.7.0

Follow [application releases](../docs/specs/releases.md) and the [release guide](../docs/releases.md). The owner reviewed the combined application changes, requested cleanup and grouped commits, then authorized cutting the release. The release commit, tag, hosted signed build and asset verification are authorized. The owner requested an editable draft during CI. Leave publication pending the owner’s notes review and approval.

- [x] Prepare version 0.7.0 and notes covering the complete changes since 0.6.0.
- [x] Run workspace, documentation, artwork and tagged bundle checks.
- [x] Commit and push the release preparation and annotated version tag.
- [x] Verify hosted signing, credential cleanup and macOS 15 startup checks.
- [x] Download and verify the release archive, then run an isolated native smoke check.
- [ ] Publish the verified release as Latest and record the evidence.

The reviewed changes include Adaptive tone, Dehaze, shared saturation protection, advanced colour ranges, dynamic exposure compensation, denoised Fast/Smart previews, additional photographic output formats, saved cropping, shared UI layout, a session activity log and the final Merrill control refinements. Previous experimental tone paths and retired alpha settings readers are removed. Notes must identify that settings compatibility limit without claiming a migration.

JPEG XL links statically in the local bundle. Its executable has only system dynamic-library dependencies. Verify the hosted archive likewise before publication. Interactive macOS 15 rendering and browser-quarantine acceptance remain outside the process-startup check.

Local checks pass for workspace tests, strict Clippy, formatting, documentation indexes, artwork and changelog extraction. The tagged bundle reports 0.7.0, arm64 and macOS 15.0 in its executable and bundle metadata. Its command-line startup and isolated native status/error-acknowledgment smoke check pass.

Release preparation commit `0863843` and annotated tag `v0.7.0` are pushed. Hosted verification runs in [the release workflow](https://github.com/hank-bond/foveonic/actions/runs/41964977783).

The editable [0.7.0 draft](https://github.com/hank-bond/foveonic/releases/tag/untagged-cadf1c6ad7d0fd3fb8f6) was created while CI builds. The existing workflow preserves its title and prose when attaching assets.

Both hosted jobs passed: the complete checks, optimized Apple Silicon bundle, Developer ID signing, credential cleanup, asset upload and signed-app process startup on macOS 15. The owner is editing the draft notes. Publication remains pending.

The downloaded signed archive passes SHA256SUMS, strict Developer ID signature verification, hardened-runtime and secure-timestamp checks, matching Team ID, version 0.7.0, arm64 and macOS 15.0 deployment metadata. Its dependencies are system libraries only, and command-line startup succeeds. An isolated native run of the downloaded app passes crop dragging, release, Hand/crop restoration, Fast and Smart transitions and regional-tile coordinates. Source sidecar writes remain disabled during the capture. The verified assets are attached to the editable draft. It remains unpublished for owner prose review.
