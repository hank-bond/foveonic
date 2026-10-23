---
summary: "Sign and notarize macOS release builds before uploading stapled archives to draft GitHub Releases."
---

# Sign application releases

Requirements: [application releases](../docs/specs/releases.md).

## Steps

- [x] Add temporary-keychain import, Developer ID signing, notarization, stapling and verification to release CI.
- [x] Create final archives and checksums only after verification. Keep existing draft-only publication behavior.
- [x] Update credential setup and release guidance. Keep local packaging credential-free.
- [x] Validate workflow syntax, mock success and failure paths, and run repository checks.
- [x] Owner reviewed the changes and approved committing, pushing and building a signed draft release.
- [x] Owner selected version 0.2.0.
- [x] Owner added the 0.2.0 introduction and approved the release notes and version bump.
- [ ] Commit and tag the release preparation, then build the signed draft.
- [ ] Verify the downloaded archive and review the app before publication.

## Decisions

The owner configured the certificate export and its password as GitHub Actions secrets, along with the Apple Account and app-specific password for notarization. The Team ID is an Actions variable. Select exactly one valid Developer ID Application identity for that team from the temporary keychain. Sign the existing single-executable app with hardened runtime and a secure timestamp, without additional entitlements. Reject missing credentials, ambiguous identities and signing or notarization failures rather than uploading unsigned assets.

Keep signing in the release workflow, separate from ordinary Cargo packaging. The temporary keychain is selected explicitly for signing and does not replace the runner's default keychain or search list. Always remove it and temporary certificate/submission files. Existing published releases remain unchanged.

## Validation

Workflow YAML and all shell blocks parse successfully. Temporary local mocks replace Apple security/signing tools and GitHub CLI without accessing real credentials or making service requests. They cover new and existing drafts, published-release rejection, API failure, missing or malformed credentials, missing/wrong/ambiguous identities, certificate import, signing and verification failures, notarization rejection or incomplete status, malformed responses, stapling and Gatekeeper failures, and cleanup after failures. Synthetic archives use real `ditto` and `shasum` commands. Their extracted ticket marker and SHA-256 establish that the final ZIP follows stapling. These mocks are local validation, not checked-in tests.

Workspace tests, strict all-target Clippy, formatting, five artwork tests, documentation generation/read-only checks and whitespace checks pass. Initial documentation-tool failures came from cached binaries containing the path of a removed checkout. With owner approval, clearing only the `docs-index` build artifacts and rebuilding resolved those failures without source changes. The existing upstream `block` future-incompatibility warning remains.

The owner approved committing and pushing the signing migration, then building and testing a signed draft release. The owner selected version 0.2.0. The owner added the release introduction and approved pushing the reviewed release notes and version bump. The 0.2.0 version bump and changelog pass workspace tests, strict Clippy, formatting and documentation checks. The local release bundle builds with version 0.2.0 and an arm64 executable whose help command runs successfully. A stale `xtask` binary also retained an old checkout path. Clearing that package's cached artifacts under the owner's existing approval restored correct version checks.

Real certificate import, Apple notarization, Gatekeeper assessment and downloaded-app review remain pending. Local mocks cannot establish those results.
