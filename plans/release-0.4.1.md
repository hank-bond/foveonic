---
summary: "Prepare the signed 0.4.1 draft with independent Merrill noise/detail controls and processing fixes."
---

# Release 0.4.1

The owner requested a cleanup, commit and patch release preparation. Follow [release requirements](../docs/specs/releases.md) and [release guidance](../docs/releases.md). Leave the introduction for owner review and keep the release a draft.

- [x] Clean up and commit Merrill reconstruction, shared editing controls and their specifications.
- [x] Verify workspace tests, strict Clippy, formatting, documentation, artwork and native GPU processing.
- [x] Prepare the patch version, notes and tagged local bundle.
- [ ] Commit release preparation and push main and the annotated tag.
- [ ] Verify hosted signing, draft assets and Sequoia executable startup.
- [ ] Verify the downloaded archive and present the draft for review.

Local checks pass 567 workspace tests and five artwork tests. GPU checks cover filtering and overlap projection, texture separation and change inspection, cached/fresh edits, retained ownership, regional Smart refinement, all four orientations and DNG technical-base behavior. Cleanup preserves the accepted portrait's noise-base and final floating-point samples exactly. Earlier measurements on an M1 Pro found adjusted Detail-only rendering fell from about 885 to 700 ms, with larger savings for neutral Detail. This is fixture-specific, not a hardware-wide latency guarantee.

The implementation keeps camera preparation, numerical GPU nodes, recipe composition and GUI widgets in their established modules. Retired residual restoration, linked overrides UI, unused core-only projection and iteration-only timing code are removed. Saved-setting migration remains at the serialization boundary.

The local tagged bundle reports 0.4.1 in both version fields. The bundle and Apple Silicon executable target macOS 15.0, and the help command starts successfully without launching the GUI.
