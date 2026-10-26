---
summary: "Record the verified and published 0.4.2 patch release for thumbnail responsiveness and settings copy/paste."
status: archived
---

# Release 0.4.2

The owner requested a patch release containing the committed browser scheduling fixes and shared keyboard/settings-transfer work, following [release requirements](../../docs/specs/releases.md) and [release guidance](../../docs/releases.md). The owner supplied the introduction and approved publication after verification.

- [x] Review changes since 0.4.1 and prepare the patch version and notes.
- [x] Run workspace checks and verify a version-matched local bundle.
- [x] Commit release preparation, push main and create the annotated release tag.
- [x] Verify hosted checks, Developer ID signing, draft assets and macOS 15 startup.
- [x] Download and verify the signed archive and publish after owner approval.

The release includes `aa4baec` and `5936440`. Photographic processing and Smart preview denoising policy remain unchanged. The quarter-resolution timing experiment is local evidence and introduces no application changes.

All 584 ordinary workspace tests and five artwork tests pass, along with strict all-target Clippy, formatting, documentation indexes and diff checks. The local tagged bundle reports 0.4.2 in both version fields. Its bundle and arm64 executable target macOS 15.0, and the help command starts without launching the GUI.

The release candidate is `98e32a5`, tagged as `v0.4.2`. While its [hosted build](https://github.com/hank-bond/foveonic/actions/runs/40266875096) ran, the owner requested removing the repeated platform/signing footer. The current draft and changelog omit it. Future notes keep that standing guidance in the release guide, mentioning it only when behavior changes. This is a notes-only adjustment and does not change the tagged application candidate.

The owner supplied the exact introduction: “Usability improvements to make importing a better experience.”

The hosted run passed all checks, Developer ID signing, credential cleanup and signed executable startup on macOS 15. The downloaded archive matches SHA256SUMS and its hosted asset digest, `7f56baebb2e4e7dff45fc378f18ed7eef5ba69b629bb681473ad4d581c344ac3`. Its Developer ID signature, hardened runtime, secure timestamp, arm64 architecture, version 0.4.2, macOS 15.0 target and help-command startup verify locally. Interactive macOS 15 display checks remain separate runtime coverage. No app was installed or launched for this preparation.

The [0.4.2 release](https://github.com/hank-bond/foveonic/releases/tag/v0.4.2) is published as Latest. Publication preserved the approved notes and verified assets exactly and retained the `98e32a5` tag target.
