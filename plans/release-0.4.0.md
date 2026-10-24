---
summary: "Prepare and verify the signed v0.4.0 release with Merrill viewport-first Smart previews and managed display colors."
---

# Release 0.4.0

The owner requested a release of the Merrill Smart improvements and color management. Follow [release requirements](../docs/specs/releases.md) and [release guidance](../docs/releases.md). Preserve the owner's existing release prose and leave the new introduction for draft review. CI creates a signed draft and does not publish it.

- [x] Review changes since v0.3.0 and select the next minor version.
- [x] Prepare version 0.4.0 and its changelog section.
- [x] Run workspace, documentation, artwork and tagged bundle checks.
- [ ] Commit reviewed changes and push main and an annotated v0.4.0 tag.
- [ ] Verify hosted signing, draft assets and Sequoia startup checks.
- [ ] Download the signed archive and verify its checksum, signature, deployment target and isolated native behavior.
- [ ] Present the verified draft for the owner's introduction and publication review.

The release includes the committed Merrill regional processing and complete-source cache work, together with the reviewed display integration. Display checks cover GPU color mapping, linear preview filtering, UI transparency and switching all four gamut choices without a RAW render. Cross-monitor and macOS 15 interactive GUI checks remain explicit acceptance work. The existing Sequoia CI job verifies signed executable startup only.

Local preparation passes 543 workspace tests, strict all-target Clippy, formatting, documentation-index checks, five artwork tests and release-note extraction. The tagged bundle reports 0.4.0 in both version fields. Its executable and bundle both target macOS 15.0, and the help command runs. The final integration smoke check completed all eight display checkpoints with the same photographic and thumbnail revision across every gamut choice.
