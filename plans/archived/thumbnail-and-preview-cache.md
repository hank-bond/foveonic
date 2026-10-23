---
summary: "Refresh rendered thumbnails, cache sharper asynchronous previews, and collect recipes in a visible source-folder directory."
status: archived
---

# Refresh browser previews

Requirements: [editing sidecars](../../docs/specs/editing-sidecars.md), [decode cache](../../docs/specs/decode-cache.md), and [browser previews](../../docs/specs/browser-previews.md).

- [x] Move recipes into the visible `foveonic` child directory and migrate adjacent recipes on save.
- [x] Add bounded larger embedded and rendered previews to the existing temporary cache budget.
- [x] Refresh browser thumbnails after completed photographic renders and use sharper previews while loading.
- [x] Verify recipe protection, preview identity/invalidation, asynchronous handoff, and native presentation.

The owner requested a visible directory so recipes remain easy to find and delete. The full-size DP0 embedded JPEG has a different crop from its Raw. Its correctly framed small thumbnail remains until a photographic render supplies the missing borders.

## Findings and checks

- Sidecars retain their complete original filename inside `foveonic/`. Adjacent recipes remain readable and migrate only on a successful save. Malformed recipes remain protected. Native inputs were never modified.
- Completed photographic frames update thumbnails independently of navigator inspection mapping. Regional detail frames cannot replace the whole-photo preview. Versioned browser replies reject late embedded work.
- Larger previews are JPEG companions under the existing boot-scoped temporary namespace. They share the decoded-source budget and LRU. Recipe fingerprints survive sidecar relocation and prevent old edits from being published under a newer recipe.
- 480 ordinary workspace tests pass. Strict all-target Clippy, formatting, documentation-index checks, and whitespace checks pass.
- GPU display readback verifies sRGB conversion and removal of row padding. Original Quattro and Merrill checks verify larger JPEG preparation, orientation, source geometry, and the DP0 cropped-JPEG exception. Folder prefetch still yields to foreground work and skips disabled families.
- Native Smart smoke completed all 12 captures and closed successfully. Whole-frame thumbnail revisions match each displayed render, including edits and disk-cache reopening. An extra background repaint exposed a smoke-only close race, now guarded. A further native Merrill portrait run completed all 10 captures and preserved the photographic thumbnail while inspecting sensor and calibrated layers.
- The local application bundle was rebuilt. Changes remain uncommitted for owner review.

## Follow-up interface adjustments

The owner requested extension-free icon/list captions and a compact Scopes row. Captions now use the source filename stem. The scope dropdown shares a row with **Viewport**, which defaults on while preserving explicit saved preferences. Existing preference checks cover the changed default and restoring an explicit Off choice.

The organizer minimum width is derived from the grid tile height, image insets, native 3:2 landscape aspect, and panel margins. The same bound applies to icon and list modes. This prevents narrowing a single-column panel past the point where landscape thumbnails shrink vertically.
