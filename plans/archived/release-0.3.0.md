---
summary: "Prepare and verify the signed v0.3.0 macOS release with Import, cache scheduling and Merrill Smart previews."
status: archived
---

# Release 0.3.0

The owner requested a release and closure of completed plans. This completed preparation follows [release requirements](../../docs/specs/releases.md) and [release guidance](../../docs/releases.md). Version 0.3.0 includes the three completed feature commits after 0.2.0. The changelog preserves older owner-written prose and leaves the new introduction available for review.

- [x] Review repository and published release state. Select the next minor version.
- [x] Archive nine completed plans, retaining unresolved research and coverage limitations.
- [x] Validate notes, lockfile, workspace, documentation, artwork and native tagged bundle.
- [x] Commit release preparation and push main and a new annotated v0.3.0 tag.
- [x] Confirm hosted checks, Developer ID signing, draft assets and credential cleanup succeed.
- [x] Download and verify checksums, signature, bundle version, architecture and isolated native behavior.
- [x] Present the verified draft for owner publication review.

Published tags stay fixed. CI creates a draft and does not publish automatically. Intel, Sequoia support and notarization remain outside this release. Existing unresolved GPU-cache research and missing camera/editor coverage must remain explicit.

Local release preparation passes all 529 workspace tests, strict all-target Clippy, formatting, five artwork tests, documentation-index checks and whitespace checks. The native tagged bundle reports version 0.3.0 in both version fields and minimum macOS 26.0. Only the two application package versions change in Cargo.lock. The changelog extractor omits the owner-introduction placeholder and preserves prior release prose.

Release preparation is commit `5a77db4`. The annotated v0.3.0 tag points to that commit and was pushed after main. The [hosted build](https://github.com/hank-bond/foveonic/actions/runs/40068472581) passed every step, including signing, upload and temporary credential cleanup. An editable GitHub draft was created with the changelog's change list so the owner could add an introduction while CI ran. The workflow preserved the existing draft's title and notes.

The downloaded `Foveonic-v0.3.0-macos-arm64.zip` verifies against SHA256SUMS with SHA-256 `2f91b592d0b6abcb5e0fcdbb124709a7e4befd3ee8d8e0efb9fd13977bc85c4d`. Its executable is arm64, and both bundle version fields report 0.3.0, with minimum macOS 26.0. Developer ID verification confirms the expected Apple certificate chain, hardened runtime and a secure timestamp. The app's help command runs.

Isolated native checks use the downloaded signed app, independent preferences and temporary decode caches. Merrill Smart retains Noise and Detail on by default, shows Fast through 100%, refines to Full at 200%, preserves the same Full revision on pan, and handles edits and cache reopening. The Import sequence verifies repeated DNG processing without output replacement, original copying, destination sidecars and import history. Original hashes remain unchanged, no source sidecars are written, and the signature remains valid after both launches. The overview capture was inspected. This CLI download does not test browser-applied quarantine or the per-app Open Anyway flow.

The release remains a verified draft for the owner's introduction and publication review. That manual publication is separate from this completed preparation slice. No published release, fixed tag, installed app or existing owner photograph was replaced.
