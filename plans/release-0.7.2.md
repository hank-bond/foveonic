---
summary: "Prepare and verify the 0.7.2 Smart preview, Merrill editing and licensing release."
---

# Release 0.7.2

Follow the [release requirements](../docs/specs/releases.md) and [release guide](../docs/releases.md). The release covers changes since 0.7.1. The owner reviews the notes and release preparation before tagging or publication.

- [x] Set the application version and prepare notes for review.
- [x] Run workspace, documentation, artwork, release notes and local bundle checks.
- [ ] Review and commit the release preparation.
- [ ] Create and push the approved tag and verify the hosted signed draft and macOS 15 startup job.
- [ ] Verify downloaded assets and perform native checks before publication.

Local packaging does not install or launch the app. Hosted signing, native GUI and Metal checks, and publication remain separate steps.

Workspace tests, strict Clippy, formatting, documentation indexes, artwork tests and release-note extraction pass. The local tagged bundle reports version 0.7.2, arm64 and a macOS 15.0 minimum. It was not installed or launched.
