---
summary: "Sign macOS release builds with Developer ID and upload unnotarized archives to draft GitHub Releases."
status: archived
---

# Sign application releases

Requirements: [application releases](../../docs/specs/releases.md).

## Steps

- [x] Add temporary-keychain import, Developer ID signing and signature verification to release CI.
- [x] Preserve and restore the runner's keychain search list during signing and cleanup.
- [x] Owner reviewed and approved the signing migration, selected 0.2.0, added release prose and approved committing and tagging.
- [x] Push release preparation and the approved keychain fix. Confirm real hosted signing succeeds.
- [x] Owner selected signing-only distribution without notarization.
- [x] Remove notarization and its credential requirements, update release guidance, and validate the signing-only workflow.
- [x] Include the merged Quattro H rendering and Smart preview support in the 0.2.0 release notes.
- [x] Rewrite the README for end users, keep a small Mac build section at the end, and move contributor checks to their developer guide.
- [x] Refresh the screenshot from the current app using the existing capture recipe.
- [x] Owner reviewed and approved pushing the release preparation, including the README, screenshot, CLI examples and Quattro H support, and releasing 0.2.0.
- [x] Commit release preparation `e95dcd2` and move the still-unpublished v0.2.0 tag under the owner's approval.
- [x] Complete the signing-only hosted draft build.
- [x] Verify the downloaded archive and smoke-test the app, then publish 0.2.0 as Latest under the owner's approval.
- [x] Owner requested closing completed plans while preparing the next release, approving this evidence record for documentation commit and archival.

## Decisions

Use the certificate export and its password from GitHub Actions secrets, with the Team ID in an Actions variable. Select exactly one valid Developer ID Application identity for that team. Sign the single-executable app with hardened runtime and a secure timestamp, without additional entitlements. Verify the signature before archiving. Missing credentials, ambiguous identities, signing failures and invalid signatures must block uploads without an unsigned fallback.

Keep signing in release CI, separate from ordinary Cargo packaging. Snapshot the original keychain search list before creating the signing keychain, add the temporary keychain alongside those entries and restore the original list during cleanup. Keep the default keychain unchanged. Always remove temporary keys, certificates and search-list snapshots.

The owner chose unnotarized distribution for this niche app and accepts a per-app Gatekeeper override on first launch. Remove notarization, stapling, Gatekeeper-acceptance gating and Apple Account credential requirements rather than maintaining optional paths. Document **Open Anyway** without recommending that users disable Gatekeeper globally. Releases stay draft-only until owner review and publication approval. Previously published releases remain unchanged.

## Validation and discoveries

The initial migration passed workspace tests, strict all-target Clippy, formatting, five artwork tests, documentation generation/read-only checks and workflow YAML/shell parsing. Temporary local mocks covered signing, notarization, archive creation, draft-only uploads and failure cleanup. These mocks were local validation, not checked-in tests. Stale `docs-index` and `xtask` binaries retained old checkout paths. Clearing those packages' build artifacts under owner approval resolved their failures without source changes. The upstream `block` future-incompatibility warning remains.

The local 0.2.0 release bundle builds with the correct plist version and an arm64 executable whose help command runs. The owner supplied the changelog introduction. Release preparation was pushed as `b626b38` with tag v0.2.0.

The [first hosted run](https://github.com/hank-bond/foveonic/actions/runs/39901620770) passed checks and native packaging, then stopped because the Team ID had been entered as a secret rather than a variable. After the owner corrected that setting, certificate import succeeded, but signing reported `no identity found`. The `codesign` manual requires the identity's keychain to be on the user's search list even when selected with `--keychain`.

The owner approved preserving/restoring that list and moving the still-unpublished tag to fix commit `c6a68b0`. Local mocks were extended to require search-list membership and verify restoration with paths containing spaces, an empty original list, snapshot/add failures and cleanup after restoration failure. Retagging this unpublished release was an explicit exception, not ordinary release practice.

The [corrected hosted run](https://github.com/hank-bond/foveonic/actions/runs/39966655130) passed real Developer ID signing and signature verification, then waited on Apple's notary service. The owner chose signing-only distribution and the GitHub wait was canceled. The app had already been uploaded to Apple. Canceling CI does not retract that upload or stop Apple's processing.

Signing-only workflow YAML and shell syntax, workspace tests, strict Clippy, formatting, five artwork tests and documentation checks pass. Updated local mocks verify the signed marker and SHA-256 in the final archive, draft-only uploads, signature-failure gating, search-list restoration and cleanup. They also reject any calls to notarization or Gatekeeper tools and confirm that Apple Account secrets are absent from the workflow. These remain local mocks, not checked-in tests.

The canceled hosted run completed credential cleanup successfully. The owner subsequently merged Quattro H rendering and Smart preview support into main and requested that both ship in 0.2.0. User-facing copy uses **Quattro H**, not the internal **Tiger** architecture name. The updated notes preserve the owner's introduction and replace the old inspection-only limitation.

The owner requested a friendly, capability-focused README for end users with only a small Mac build section at the end. Implementation and validation details were removed, and contributor checks moved to the contribution guide with existing links migrated. The fixed flowers/stroller screenshot recipe was rerun against the merged app. Online sync found seven cached examples, the capture produced a 2650×2194 lossless WebP, and all fourteen original/sidecar hashes remained unchanged. The owner also requested two concise CLI examples before the final build section. Both TIFF and DNG commands were checked against the cached stroller original and produced native-size outputs. Explicit Sunlight/Standard options avoid depending on camera-family-specific CLI defaults. Image and prose await owner review. Combined validation passes all 484 workspace tests, strict all-target Clippy, formatting, five artwork tests, signing-only workflow mocks, documentation generation/read-only checks and whitespace checks. GPU/original-file tests remain opt-in and were not rerun as part of this presentation update.

The owner approved pushing the reviewed release preparation and getting 0.2.0 released. Commit `e95dcd2` includes the local Quattro H merge, signing-only workflow and new README/screenshot. The unpublished v0.2.0 tag was moved under that approval. No previously published tag or release was modified.

The [signing-only hosted run](https://github.com/hank-bond/foveonic/actions/runs/39973024540) passed checks, native packaging, signing, verification, archive upload and credential cleanup. The downloaded ZIP's SHA-256 is `33da4ec891f463e1a8063d899dc6803b4a9bcab3faa0e00bf645068a665e3453`. The extracted app verifies against its Developer ID chain, uses hardened runtime and a secure timestamp, and has version 0.2.0, minimum macOS 26.0 and an arm64 executable. Draft notes exactly match the reviewed changelog.

The downloaded app's help command and isolated native screenshot smoke test pass. It produces the expected 2650×2194 frame, retains a valid signature after launch, and leaves all fourteen cached originals/sidecars unchanged. This was a GitHub CLI download without browser-applied quarantine. The browser-triggered **Open Anyway** flow was not automated, and no Gatekeeper bypass was applied.

The [v0.2.0 release](https://github.com/hank-bond/foveonic/releases/tag/v0.2.0) was published as Latest under the owner's approval. The published assets are the verified Apple Silicon ZIP and SHA256SUMS. The application remains unnotarized, with the first-launch override documented. The release tag is now fixed. The owner subsequently requested closing completed plans while preparing 0.3.0, so this completed signing migration is archived with its final evidence record.

## Closure

This completed implementation or experiment is archived during release 0.3.0 preparation at the owner's request. Earlier scope, review and working-tree statements describe that historical slice. Recorded limitations and separately tracked follow-ups remain explicit.
