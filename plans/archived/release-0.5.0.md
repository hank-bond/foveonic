---
summary: "Clean up the shared photographic editor and prepare the local 0.5.0 feature release for review."
status: archived
---

# Prepare release 0.5.0

Requirements: [releases](../../docs/specs/releases.md), [photographic finishing](../../docs/specs/photographic-finishing.md), [Color Editor](../../docs/specs/color-editor.md), and [GUI workspace](../../docs/specs/gui-workspace.md).

- [x] Review Color Editor, DNG basis mapping, bypass policy and visual section reordering together.
- [x] Remove the common-row/Color Editor dependency cycle and centralize active-state checks.
- [x] Distinguish main RGB white from classifier white in the numerical editor contract.
- [x] Prepare versioned notes covering all changes since 0.4.2, retaining the owner's introduction slot.
- [x] Run repository, GPU/original and native bundle checks.
- [x] Complete the local preparation record for owner review.

Scope is local release preparation. Commit, tag, push and publication remain separate owner actions. The feature release includes the earlier committed shared finishing and monochrome/DNG work plus the new Color Editor and editing-section layout. No new photographic defaults are selected.

The owner subsequently approved gathering the decode-cache files and tests into a directory module for this release. The completed [module plan](./decode-cache-module.md) records that follow-up and its checks. The public `decode_cache` path, storage format and scheduling policy remain unchanged. The pool shared by X3F/background scheduling stays separate.

## Verification and handoff

Workspace tests, strict Clippy, formatting, documentation indexes and artwork checks pass. The four numerical Color Editor fixtures pass. Original-file DNG reprojection checks pass for Classic, Merrill, Quattro and Quattro H. The combined Merrill viewport check passes with Color Editor, HDR, Local Contrast and Universal sharpening active. Section-layout tests cover meaningful insertion positions, payload isolation and reorder-only preference changes.

The optimized native bundle reports version 0.5.0 and targets macOS 15 on Apple Silicon. Its command-line help starts successfully. Interactive compatibility on macOS 15 and photographic review remain owner testing tasks. No application was installed or launched over the owner's running app.

The changelog retains the owner's introduction slot. After reviewing the native app, the owner requested the hosted release draft. That authorizes committing and pushing the reviewed changes, tagging 0.5.0, and preparing draft notes and signed assets through the release workflow. Public release publication remains pending owner review.
