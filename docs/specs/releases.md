---
summary: "Cargo packages unsigned macOS apps and release tags build draft GitHub Releases without publishing them."
kind: reference
status: accepted
implementation: conforming
---

# Application releases

## Requirements

- Local macOS packaging and launching **must** use a Rust workspace task exposed through Cargo. Normal builds **must not** require Python or regenerate checked-in artwork.
- Packaging **must** preserve the native icon appearances, compiler-provided icon metadata, executable permissions, stable application identity, and macOS 26 minimum. It **must** use Cargo's reported executable path, including custom target directories.
- Application crates and bundle metadata **must** share one version. Release tags **must** use `v` followed by that version. A mismatched tag **must** fail before release assets are uploaded.
- Pushing a release tag **must** trigger GitHub Actions checks and an Apple Silicon macOS app build with Xcode 26 or later. GPU/original-file fixtures **must** remain opt-in.
- Initial releases **must** be unsigned by a Developer ID and unnotarized. The archive **must** preserve the app bundle and executable permissions.
- Successful release builds **must** attach the app archive to a draft GitHub Release. Automation **must not** publish releases or change an existing release's prose. Uploads to an already published release **must** fail.
- The checked-in changelog **must** supply the initial draft notes for the tagged version, with space for the owner to add introductory prose. The owner reviews prose and publishes the release.

## Initial release

Prepare `v0.1.0` with a concise feature changelog and the owner's introductory prose. Creating or pushing its tag and publishing the release remain separate owner-approved actions. Signing, notarization, Intel builds, and installers are outside this release's scope.

## Validation

Local packaging, archive round-trip checks, task/workspace tests, and mocked draft-release handling pass as recorded in the [preparation plan](../../plans/archived/cargo-releases.md). The first [hosted release run](https://github.com/hank-bond/foveonic/actions/runs/38832699996) passed checks, built and archived the app, and uploaded assets to a draft. The [v0.1.0 release](https://github.com/hank-bond/foveonic/releases/tag/v0.1.0) is now published with the Apple Silicon archive and checksums.
