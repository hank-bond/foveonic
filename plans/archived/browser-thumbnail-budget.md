---
summary: "Replace the browser thumbnail count limit with a persistent memory budget and group performance preferences."
status: archived
---

# Browser thumbnail budget

Implements [browser previews](../../docs/specs/browser-previews.md) and [decoded caching](../../docs/specs/decode-cache.md).

- [x] Replace the 256-entry thumbnail cap with a 256 MiB default budget, measured from actual texture dimensions at eight bytes per pixel.
- [x] Retain recently used thumbnails and pin browser/viewport consumers until both finish drawing. Keep the initial visible-first greedy folder pass and separate eight-entry larger-preview cache.
- [x] Persist the independent memory preference and group it with disk space, workers, family policies and look behind/ahead under Performance.
- [x] Verify variable-size eviction, replacement, reduced budgets, active-view pinning, preference migration and retention across a 673-file folder pass.
- [x] Complete workspace checks and build the app for owner review.

The old fixed count cap could finish the whole-folder pass and evict its earlier thumbnails before the owner scrolled to them. Byte accounting permits more small images without conflating thumbnail memory with disk cache readiness. A frame may exceed the budget while all remaining textures are visible. This avoids reload loops when a configured budget is smaller than the current view.

Workspace tests, strict all-target Clippy, formatting, generated-index checks and the five artwork tests pass. The optimized native bundle is ready for owner review. A native display smoke run completed and its Settings capture confirms the Performance controls fit with the expected 256 MiB, three workers and three/seven neighbor defaults.
