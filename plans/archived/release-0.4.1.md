---
summary: "Record the verified and published 0.4.1 release with Merrill noise/detail controls and processing fixes."
status: archived
---

# Release 0.4.1

The owner requested a cleanup, commit and patch release preparation under the [release requirements](../../docs/specs/releases.md) and [release guidance](../../docs/releases.md). After saving the introduction, the owner explicitly approved publication as Latest.

- [x] Clean up and commit Merrill reconstruction, shared editing controls and their specifications.
- [x] Verify workspace tests, strict Clippy, formatting, documentation, artwork and native GPU processing.
- [x] Prepare the patch version, notes and tagged local bundle.
- [x] Commit release preparation and push main and the annotated tag.
- [x] Verify hosted signing, draft assets and Sequoia executable startup.
- [x] Verify the downloaded archive and present the draft for review.

Local checks pass 567 workspace tests and five artwork tests. GPU checks cover filtering and overlap projection, texture separation and change inspection, cached/fresh edits, retained ownership, regional Smart refinement, all four orientations and DNG technical-base behavior. Cleanup preserves the accepted portrait's noise-base and final floating-point samples exactly. Earlier measurements on an M1 Pro found adjusted Detail-only rendering fell from about 885 to 700 ms, with larger savings for neutral Detail. This is fixture-specific, not a hardware-wide latency guarantee.

The implementation keeps camera preparation, numerical GPU nodes, recipe composition and GUI widgets in their established modules. Retired residual restoration, linked overrides UI, unused core-only projection and iteration-only timing code are removed. Saved-setting migration remains at the serialization boundary.

The local tagged bundle reports 0.4.1 in both version fields. The bundle and Apple Silicon executable target macOS 15.0, and the help command starts successfully without launching the GUI.

## Signed draft verification

Merrill reconstruction and UI cleanup are committed as `3775c84`. The first release candidate was `94677aa`. Its [hosted run](https://github.com/hank-bond/foveonic/actions/runs/40252167818) passed workspace checks, Developer ID signing, archive upload, credential cleanup and signed executable startup on macOS 15.

That candidate's downloaded archive matched both SHA256SUMS and its hosted asset digest, `565bd975e4e7bf48c0e5af5bf7928badd20722b3b7200c2228003c7d039844fe`. Its Developer ID signature, hardened runtime, secure timestamp, arm64 architecture, version 0.4.1, macOS 15.0 deployment target and help-command startup verified locally. Interactive macOS 15 display behavior, multi-monitor behavior and Gatekeeper first-launch acceptance remain separate user checks. No app was installed or launched for this preparation.

## Shortcut correction

The owner requested Option/Alt+arrow for preserving the viewport so Shift+arrow remains dedicated to range selection. Commit `9e79847` implements the change and passes 27 browser tests, GUI all-target Clippy, formatting, documentation checks and local app packaging. The owner approved replacing the draft candidate, and `v0.4.1` now targets that commit. The draft's shortcut note was updated while preserving its other prose.

The replacement [hosted run](https://github.com/hank-bond/foveonic/actions/runs/40253479231) passed every check, signed packaging and macOS 15 startup. Its downloaded archive matches SHA256SUMS and the hosted digest, `05835ef8083c552519e2b7e29aad6f4a5002e5355a6703405f5c4db4056735f7`. Developer ID signature, hardened runtime, timestamp, arm64 architecture, version 0.4.1, macOS 15.0 deployment target and executable startup verify locally. The draft assets now contain the corrected shortcut.

The [0.4.1 release](https://github.com/hank-bond/foveonic/releases/tag/v0.4.1) is published as Latest. Publication preserved the owner's saved notes exactly and retained the verified replacement assets and `9e79847` tag target.
