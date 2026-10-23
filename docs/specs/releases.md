---
summary: "Cargo packages macOS apps and release tags build signed, notarized draft GitHub Releases without publishing them."
kind: reference
status: accepted
implementation: incomplete
---

# Application releases

## Requirements

- Local macOS packaging and launching **must** use a Rust workspace task exposed through Cargo. Normal builds **must not** require Python or regenerate checked-in artwork.
- Packaging **must** preserve the native icon appearances, compiler-provided icon metadata, executable permissions, stable application identity, and macOS 26 minimum. It **must** use Cargo's reported executable path, including custom target directories.
- Application crates and bundle metadata **must** share one version. Release tags **must** use `v` followed by that version. A mismatched tag **must** fail before release assets are uploaded.
- Pushing a release tag **must** trigger GitHub Actions checks and an Apple Silicon macOS app build with Xcode 26 or later. GPU/original-file fixtures **must** remain opt-in.
- Release CI **must** sign the app with a valid Developer ID Application identity for the configured team, enable hardened runtime and obtain a secure timestamp. Local packaging **must not** require signing credentials.
- Release CI **must** obtain Apple notarization acceptance, staple the ticket, validate the staple and verify the app's signature and Gatekeeper assessment before creating the final archive and checksum. The archive **must** preserve the app bundle and executable permissions.
- Missing credentials, ambiguous signing identities, or signing, notarization and verification failures **must** block release uploads. There **must not** be an unsigned fallback.
- Signing credentials **must** come from GitHub Actions secrets. The Team ID **may** be a repository variable. Certificate exports and imported private keys **must** remain in temporary runner storage and be cleaned up on success, failure or cancellation.
- Successful release builds **must** attach the app archive to a draft GitHub Release. Automation **must not** publish releases or change an existing release's prose. Uploads to an already published release **must** fail.
- The checked-in changelog **must** supply the initial draft notes for the tagged version, with space for the owner to add introductory prose. The owner reviews prose and publishes the release.

## Release scope

Release assets remain Apple Silicon ZIP archives for macOS 26 or later. Intel builds and installers are outside the current scope. Creating or pushing a tag and publishing its release remain separate owner-approved actions. Previously published unsigned releases remain unchanged.

## Validation

Local packaging, archive round-trip checks, task/workspace tests, and mocked draft-release handling pass as recorded in the [preparation plan](../../plans/archived/cargo-releases.md). The first [hosted release run](https://github.com/hank-bond/foveonic/actions/runs/38832699996) passed checks, built and archived the app, and uploaded assets to a draft. The [v0.1.0 release](https://github.com/hank-bond/foveonic/releases/tag/v0.1.0) is now published with the Apple Silicon archive and checksums. That initial release was unsigned and unnotarized.

The [signed-release plan](../../plans/signed-releases.md) tracks the signing migration. Hosted signing, notarization and downloaded-app validation remain pending, so conformance is not yet established for the signed workflow.
