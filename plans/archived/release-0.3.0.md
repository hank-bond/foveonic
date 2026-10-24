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

Initial release preparation was commit `5a77db4`. The unpublished draft tag originally pointed to that commit and was pushed after main. The [hosted build](https://github.com/hank-bond/foveonic/actions/runs/40068472581) passed every step, including signing, upload and temporary credential cleanup. An editable GitHub draft was created with the changelog's change list so the owner could add an introduction while CI ran. The workflow preserved the existing draft's title and notes.

The initial macOS 26-only draft archive `Foveonic-v0.3.0-macos-arm64.zip` verified against SHA256SUMS with SHA-256 `2f91b592d0b6abcb5e0fcdbb124709a7e4befd3ee8d8e0efb9fd13977bc85c4d`. Its executable is arm64, and both bundle version fields report 0.3.0, with minimum macOS 26.0. Developer ID verification confirms the expected Apple certificate chain, hardened runtime and a secure timestamp. The app's help command runs.

Isolated native checks use the downloaded signed app, independent preferences and temporary decode caches. Merrill Smart retains Noise and Detail on by default, shows Fast through 100%, refines to Full at 200%, preserves the same Full revision on pan, and handles edits and cache reopening. The Import sequence verifies repeated DNG processing without output replacement, original copying, destination sidecars and import history. Original hashes remain unchanged, no source sidecars are written, and the signature remains valid after both launches. The overview capture was inspected. This CLI download does not test browser-applied quarantine or the per-app Open Anyway flow.

This first checkpoint left a verified draft for the owner's introduction and publication review. The later Sequoia replacement and publication are recorded below. No published release, fixed tag, installed app or existing owner photograph was replaced.

## Sequoia replacement and publication

The owner requested macOS 15 targeting before publishing, saved an introduction and authorized publication when ready. The [Sequoia plan](../sequoia-compatibility.md) records the deployment changes, corrected CI configuration and bounded runtime checks. Under that request, the still-unpublished tag moved to `9ffeeae`. Its [signed build](https://github.com/hank-bond/foveonic/actions/runs/40069873574) passes both the macOS 26 packaging job and the signed executable's startup check on macOS 15.7.9.

The replacement archive's SHA-256 is `49e0c1c8b1a3eababe82f12e6d7cb4fa41e907d1668dc909ff01d30b1a0d83d4`. Executable and bundle target 15.0. Signature, resources, version and isolated Smart/Import behavior verify on the downloaded signed app. This supersedes the initial 26-only draft checksum above. Interactive GUI/Metal behavior on Sequoia still awaits the user's trial.

[v0.3.0](https://github.com/hank-bond/foveonic/releases/tag/v0.3.0) is published as Latest under the owner's approval. The saved introduction and change list remain intact. Only the compatibility footer changed to macOS 15. Public checksum download, asset digest, source tag and Latest designation verify. The published v0.3.0 tag is now fixed.
