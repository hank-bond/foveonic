---
summary: "Add three browser views with independent review ratings, metadata sorting and shared search filters."
status: archived
---

# Improve browser review

Requirements: [browser review](../../docs/specs/browser-review.md), [browser previews](../../docs/specs/browser-previews.md), [GUI workspace](../../docs/specs/gui-workspace.md) and [import workflow](../../docs/specs/import-workflow.md).

- [x] Separate browser query/order data from thumbnail and selection interaction.
- [x] Load capture metadata and review ratings asynchronously without delaying thumbnails.
- [x] Add persisted metadata sorting and text/minimum-rating filters.
- [x] Provide compact grid, metadata list and large image views with star editing.
- [x] Route rating keys through the shared shortcut registry.
- [x] Persist Browse ratings separately from recipes and freeze Import ratings at submission.
- [x] Verify selection, filtering, persistence, queue order and native layouts.
- [x] Complete repository checks and build the review app.

The owner approved separate review data rather than modifying editing recipes. Use one versioned `foveonic/ratings.json` map per source folder. Keep import source persistence disabled. The photo parser and render graph do not gain review responsibilities.

## Verification and review

The eye retains its existing hidden/imported visibility meaning. The separate view menu uses drawn grid, list and large-preview symbols. Drawing avoids unsupported font glyphs. Search fits inline in wide panes and wraps in narrow ones. A bounds test caught text-field margins extending the header, which now allocates the whole input box explicitly.

The shared query model preserves containing-folder order, sorts numeric metadata numerically, keeps missing values last in either direction and combines substring/rating filters. A filtered viewed photograph remains displayed. Browser processing highlights and decode neighborhoods follow the same order. Filename sorting does not restart neighborhoods when unrelated metadata arrives.

Workspace tests and strict Clippy pass, including added interaction checks for star clicks, highlighted rating keys, text input exclusions, filter/navigation/prefetch agreement, responsive header bounds and large-image aspect ratios. Asynchronous tests cover Browse restoration, stale-folder replies and Import source protection. Original-file checks cover copy-only rating transfer with overwrite/rename and source-independent cached selection metadata. Existing GPU processing is unchanged.

The optimized native review bundle builds. An isolated mixed-architecture folder run captures all three views and verifies that browser view, sort and rating changes retain the rendered image revision. Photographic UI styling remains for owner review. The changes are uncommitted.

The cleanup pass keeps import-menu actions in browser review and formats list metadata once per visible row. Thumbnail fitting is shared by all browser and comparison views. Active comparison photos use the same viewport presentation as single-photo editing, including Smart regional overlays. Larger-preview retirement collects demand from both sidebar and viewport before canceling work; a regression test covers comparison requests outside the sidebar's visible range. The compact grid retains image-only presentation, green import-pick borders and the adjusted cache-dot clearance.

Workspace tests, strict Clippy, formatting, indexes and artwork checks pass after cleanup. The optimized native mixed-architecture comparison run passes initial display and focus switching with the highlighted set retained. The review bundle is rebuilt; changes remain uncommitted.
