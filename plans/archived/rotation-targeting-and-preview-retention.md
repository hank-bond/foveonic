---
summary: "Correct rotation targeting and retain edited previews across active-photo changes."
status: archived
---

# Rotation targeting and preview retention

Implements the corrected [rotation interaction](../../docs/specs/rotation.md) and [browser preview retention](../../docs/specs/browser-previews.md) requirements before rebuilding the release draft.

- [x] Buttons, menus, R and Shift+R select rotation tools without changing photographs. Quarter-turn tools apply their relative turn when the image is clicked.
- [x] Command+Left and Command+Right affect only the active photograph. Selected thumbnail and comparison-photo targets retain the active view and selected set.
- [x] Straighten and Freehand double-click reset both saved rotation components to camera orientation, preserving crop content. Selected rotation icons follow the pointer over image targets.
- [x] Use the existing sidecar writer for unviewed targets, preserving each recipe, import review without source writes, malformed-recipe protection and queued writes at shutdown.
- [x] Retain bounded GPU thumbnails and fitted previews as each photographic frame arrives. Accept delayed CPU previews according to per-photo freshness after activation changes. Reject outdated embedded replies and invalidated presentations.
- [x] Preserve normalized linear wide-gamut samples when cached thumbnails rotate.

Workspace tests, Clippy, formatting, documentation and artwork checks passed. Tests cover shortcut routing, selection without execution, full double-click reset, selected thumbnail targeting, preview revision ordering and pending sidecar turns during activation and shutdown. GPU tests cover turn direction, extended linear samples and existing preview readback. The optimized native rotation smoke completed all stages, including shared framing, zoom, Freehand, Fast and Smart. The native comparison smoke completed activation changes across selected photographs.

The smaller overview experiment remains separate from these interaction fixes. The release draft and tag remain unchanged pending a rebuild after owner review.
