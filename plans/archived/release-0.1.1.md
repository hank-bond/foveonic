---
status: archived
summary: "Release v0.1.1 passed local and hosted checks and was published as GitHub's Latest application release."
---

# Release v0.1.1

The owner requested a v0.1.1 release and asked how Latest is managed. Preparation followed [application release requirements](../../docs/specs/releases.md) and the [release workflow](../../docs/releases.md). The owner separately approved publication.

## Steps

- [x] Inspect published releases and version tags. v0.1.0 was GitHub's Latest application release at preparation time.
- [x] Bump the shared application version and prepare the matching changelog section, preserving the owner's release introduction.
- [x] Run workspace and artwork checks, validate release notes, and build/inspect the versioned native bundle.
- [x] Owner completed the release introduction and approved proceeding with the requested release preparation.
- [x] Commit and push the reviewed preparation and versioned tag for the draft build.
- [x] Check hosted build and draft assets.
- [x] Owner approved publishing the release and marking it Latest.

## Release contents

The release includes UI grouping, spacing, readout alignment, content-sized dropdowns, TIFF destination spaces/profiles, Auto sharpening, photographic TIFF resizing, and the updated README screenshot. It is not exclusively a UI release.

GitHub's Latest designation is separate from Git refs. Version tags remain fixed. The current CI creates drafts only, so preparing v0.1.1 does not change the public Latest release. A published stable application release can be explicitly marked Latest. No moving `latest` Git tag exists or is needed.

## Local validation

Ordinary workspace tests, strict all-target Clippy, five artwork tests, formatting, documentation/index checks, and release-note extraction pass. The local `Foveonic.app` bundle builds for arm64, reports 0.1.1 in both bundle version fields, and retains macOS 26.0 as its minimum. The app was not installed or launched during release preparation. The owner's completed changelog introduction is preserved in extracted release notes. Review of the remote found no existing v0.1.1 tag or release. Publication required separate owner approval and did not happen as part of the draft build.

## Hosted draft

Annotated tag `v0.1.1` points to release-preparation commit `6409115`. The [hosted build](https://github.com/hank-bond/foveonic/actions/runs/38851310552) passed formatting, workspace/artwork tests, Clippy, documentation checks, native packaging, archive creation, and draft upload.

The draft contains `Foveonic-v0.1.1-macos-arm64.zip` and `SHA256SUMS`. Downloaded assets verified with `shasum -a 256 -c SHA256SUMS`. The archive SHA-256 is `0f456e210bb828c15c07bcef01cd28ea5ade32be9b6e649540021a074a4cdcdd`. Extraction preserved the executable bit. The executable is arm64 and both bundle version fields report 0.1.1. The draft body matches the owner's changelog prose and reviewed change list. The downloaded app was not installed or launched.

The hosted build created a stable draft and left v0.1.0 as Latest pending the owner's publication approval. The version tag remained fixed while this tracking plan was updated.

## Publication

The owner explicitly approved publication and Latest selection. [Foveonic v0.1.1](https://github.com/hank-bond/foveonic/releases/tag/v0.1.1) was published on 2026-10-16 with both verified assets. GitHub's latest-release API confirms tag v0.1.1, draft false, and prerelease false. No moving Git tag was created.

The previously documented intermittent GPU cache-equality failure remains unresolved and was not retried or described as fixed by release checks. The targeted export GPU checks passed at the reviewed feature commit, as recorded in the [feature plan](../tiff-color-and-auto-sharpen.md).
