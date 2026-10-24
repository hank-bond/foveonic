---
summary: "Prepare and verify the signed v0.3.0 macOS release with Import, cache scheduling and Merrill Smart previews."
---

# Release 0.3.0

The owner requested a release and closure of completed plans. Follow [release requirements](../docs/specs/releases.md) and [release guidance](../docs/releases.md). Version 0.3.0 includes the three completed feature commits after 0.2.0. The changelog preserves older owner-written prose and leaves the new introduction available for review.

- [x] Review repository and published release state. Select the next minor version.
- [x] Archive nine completed plans, retaining unresolved research and coverage limitations.
- [x] Validate notes, lockfile, workspace, documentation, artwork and native tagged bundle.
- [ ] Commit release preparation and push main and a new annotated v0.3.0 tag.
- [ ] Confirm hosted checks, Developer ID signing, draft assets and credential cleanup succeed.
- [ ] Download and verify checksums, signature, bundle version, architecture and isolated native behavior.
- [ ] Present the verified draft for owner publication review.

Published tags stay fixed. CI creates a draft and does not publish automatically. Intel, Sequoia support and notarization remain outside this release. Existing unresolved GPU-cache research and missing camera/editor coverage must remain explicit.

Local release preparation passes all 529 workspace tests, strict all-target Clippy, formatting, five artwork tests, documentation-index checks and whitespace checks. The native tagged bundle reports version 0.3.0 in both version fields and minimum macOS 26.0. Only the two application package versions change in Cargo.lock. The changelog extractor omits the owner-introduction placeholder and preserves prior release prose.
