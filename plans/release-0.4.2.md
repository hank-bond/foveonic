---
summary: "Prepare a signed 0.4.2 patch draft for thumbnail responsiveness and settings copy/paste."
---

# Release 0.4.2

The owner requested a patch release containing the committed browser scheduling fixes and shared keyboard/settings-transfer work, following [release requirements](../docs/specs/releases.md) and [release guidance](../docs/releases.md). Preserve the owner's opportunity to add introductory prose before publication.

- [x] Review changes since 0.4.1 and prepare the patch version and notes.
- [x] Run workspace checks and verify a version-matched local bundle.
- [ ] Commit release preparation, push main and create the annotated release tag.
- [ ] Verify hosted checks, Developer ID signing, draft assets and macOS 15 startup.
- [ ] Download and verify the signed archive and present the draft for owner review.

The release includes `aa4baec` and `5936440`. Photographic processing and Smart preview denoising policy remain unchanged. The quarter-resolution timing experiment is local evidence and introduces no application changes.

All 584 ordinary workspace tests and five artwork tests pass, along with strict all-target Clippy, formatting, documentation indexes and diff checks. The local tagged bundle reports 0.4.2 in both version fields. Its bundle and arm64 executable target macOS 15.0, and the help command starts without launching the GUI.
