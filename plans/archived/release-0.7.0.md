---
summary: "Records the verified 0.7.0 release and its standalone photographic comparison tooling."
status: archived
---

# Release 0.7.0

Follow [application releases](../../docs/specs/releases.md) and the [release guide](../../docs/releases.md). The owner reviewed the combined application changes, requested cleanup and grouped commits, then authorized cutting the release. The owner requested an editable draft during CI, added the release introduction and comparisons, then approved publication.

- [x] Prepare version 0.7.0 and notes covering the complete changes since 0.6.0.
- [x] Run workspace, documentation, artwork and tagged bundle checks.
- [x] Commit and push the release preparation and annotated version tag.
- [x] Verify hosted signing, credential cleanup and macOS 15 startup checks.
- [x] Download and verify the release archive, then run an isolated native smoke check.
- [x] Publish the verified release as Latest and record the evidence.

The reviewed changes include Adaptive tone, Dehaze, shared saturation protection, advanced colour ranges, dynamic exposure compensation, denoised Fast/Smart previews, additional photographic output formats, saved cropping, shared UI layout, a session activity log and the final Merrill control refinements. Previous experimental tone paths and retired alpha settings readers are removed. Notes must identify that settings compatibility limit without claiming a migration.

JPEG XL links statically in the local bundle. Its executable has only system dynamic-library dependencies. Verify the hosted archive likewise before publication. Interactive macOS 15 rendering and browser-quarantine acceptance remain outside the process-startup check.

Local checks pass for workspace tests, strict Clippy, formatting, documentation indexes, artwork and changelog extraction. The tagged bundle reports 0.7.0, arm64 and macOS 15.0 in its executable and bundle metadata. Its command-line startup and isolated native status/error-acknowledgment smoke check pass.

Release preparation commit `0863843` and annotated tag `v0.7.0` are pushed. Hosted verification runs in [the release workflow](https://github.com/hank-bond/foveonic/actions/runs/41964977783).

The editable draft was created while CI built. The workflow preserved its title and prose when attaching assets.

Both hosted jobs passed: the complete checks, optimized Apple Silicon bundle, Developer ID signing, credential cleanup, asset upload and signed-app process startup on macOS 15.

The downloaded signed archive passes SHA256SUMS, strict Developer ID signature verification, hardened-runtime and secure-timestamp checks, matching Team ID, version 0.7.0, arm64 and macOS 15.0 deployment metadata. Its dependencies are system libraries only, and command-line startup succeeds. An isolated native run of the downloaded app passes crop dragging, release, Hand/crop restoration, Fast and Smart transitions and regional-tile coordinates. Source sidecar writes remain disabled during the capture.

The owner approved publishing [Foveonic v0.7.0](https://github.com/hank-bond/foveonic/releases/tag/v0.7.0) on November 5, 2026. GitHub confirms it is public, not a prerelease, and the Latest application release. Publication preserved the owner's title and notes and the verified archive and checksum assets. The local and remote tag still resolve to `0863843afc359fd7434b105fb89dabcf627135c7`. The archive SHA-256 is `258cfbc828f794875a87c302d7f66cadf0ca1b6b7291b0ce5b42c78aaec4523b`.

## Release illustrations

The owner supplied `_P3M0027.X3F`, published unchanged as `sunlit-metal-latch.x3f` in the supplemental example collection. Its recorded dial/net compensation is +0.70 EV with no bracket offset, so Auto Exposure with Dial or Both resolves to −0.70 EV.

The [standalone creator](../../scripts/help-examples/examples.py) shares rendering, profile conversion and grid composition with help comparisons. The [Adaptive/Exposure preset](../../scripts/help-examples/adaptive-exposure.json) renders Disabled across the top row and Natural across the bottom row, with EV 0, EV A (-0.7EV) and EV −2 as columns. Daylight WB, Standard colour, default reconstruction and Local highlight recovery stay fixed. All frames are rendered before identical display resizing, without hand-tuned per-variant corrections or GUI sidecars. The candidate PNG remains outside Git pending owner review.

The creator’s Python tests verify all six CLI calls, shared settings, the clockwise panel positions, tagged sRGB output and untouched originals. Existing help generation tests also pass. The first concurrent workspace run hit two asynchronous cache-test timeouts. Repeating the unchanged Rust suite serially after compilation passes. Strict Clippy, formatting, documentation and artwork checks pass.
