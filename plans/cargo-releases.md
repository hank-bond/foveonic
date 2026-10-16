---
summary: "Replace Python app packaging with Cargo tasks and prepare an unsigned v0.1.0 draft-release workflow."
---

# Prepare Cargo-based releases

Requirements: [application releases](../docs/specs/releases.md) and [native GUI packaging](../docs/specs/gui-workspace.md).

## Steps

- [x] Add tested Rust tasks for packaging, launching, and extracting versioned release notes.
- [x] Share application version 0.1.0 and remove the Python packaging path.
- [x] Add release-tag CI checks, unsigned Apple Silicon archives, and draft-release uploads that preserve owner prose.
- [x] Prepare the initial changelog and migrate build/release guidance.
- [x] Run workspace checks, packaging regressions, and a native release build. Inspect bundle metadata, executable architecture, and archive extraction.
- [x] Owner added release prose and approved the changes for commit and push.
- [ ] Owner authorizes the release tag and reviews the hosted draft before publication.

## Decisions

Use the checked-in Icon Composer foregrounds directly during packaging. Artwork generation remains an explicit Python editing tool. The supported GitHub `macos-26` runner is Apple Silicon and includes Xcode 26. Build drafts only. An existing draft keeps its notes, and an already published release rejects uploads.

CI itself cannot be exercised without pushing. Local checks must distinguish verified packaging behavior from the unexecuted hosted workflow.

## Validation

Workspace tests, strict all-target Clippy, formatting, documentation generation/read-only checks, and five artwork tests pass. Ten task tests cover CLI argument forwarding, release/debug conflicts, Cargo-reported custom target paths, executable permissions, compiler icon metadata, stale-resource replacement, preserving an older bundle on assembly failure, exact tag matching, and selecting one changelog section while preserving owner prose. Both release-note and bundle commands reject a mismatched tag before building.

The release bundle builds locally with Xcode 26.5. Its executable is arm64, its Mach-O minimum is macOS 26.0, and its plist version is 0.1.0. The asset catalog retains Aqua and DarkAqua appearances. Only the linker's ad-hoc signature is present, with no Developer ID or team identity. Building leaves checked-in icon artwork unchanged. Archiving with the workflow's `ditto` command and extracting it preserves every file's content and permission bits. The extracted executable runs its help command, and the archive checksum verifies.

Workflow YAML and shell syntax pass local parsing. A mocked GitHub CLI exercises missing releases, existing drafts, published releases, and API failures. New releases use `--draft` and tagged changelog notes. Existing drafts only receive replacement assets without editing their prose. Published releases and failed creation attempts never upload assets. Validation performed no real release API writes or tag operations. The owner subsequently approved committing and pushing the preparation work.

The first hosted release run and draft review remain pending. The existing upstream `block` future-incompatibility warning is unchanged.
