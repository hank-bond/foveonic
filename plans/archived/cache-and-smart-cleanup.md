---
summary: "Organize cache and Smart preview responsibilities and explain constants while preserving processing behavior."
status: archived
---

# Cache and Smart preview cleanup

The owner requested a structural review of the current work before further changes. Preserve the accepted [decode cache](../../docs/specs/decode-cache.md), [browser previews](../../docs/specs/browser-previews.md), and [Smart preview](../../docs/specs/smart-preview.md) behavior.

- [x] Separate cache scheduling from persistence and shared preview preparation from browser UI.
- [x] Keep Smart orchestration with its controller and move inspection tests beside the implementation.
- [x] Name timing, format, geometry, and storage constants introduced by this work.
- [x] Verify architectural boundaries, ordinary tests, GPU regressions, and native Smart interaction.

The Quattro recipe continues to own regional support and camera decisions. Generic numerical nodes remain camera independent. No photographic algorithm, cache format, queue capacity, or rendering policy changes are intended.

## Findings

Cache scheduling, prefetch eligibility, and readiness scanning now live in a private worker module. Decoded-source persistence and shared LRU accounting remain in the cache module. Display-preview persistence remains its existing companion. Worker counts, queue capacities, temporary identity, binary versions, and eviction policy are preserved.

Shared CPU display-image preparation now belongs to a presentation module rather than the organizer. The browser loader uses named request kinds and replies. Smart orchestration sits beside its controller, scope controls beside scope presentation, and inspection tests beside their implementation. The existing GPU presentation mapper is named for both navigator and photographic-preview callers.

Timing intervals, preview limits, grid geometry, binary layout fields, reconstruction bounds, and bilinear region support have names or local explanations. Float32 byte accounting uses type sizes. Quattro margins and alignment remain recipe-owned. No shader arithmetic changes were required.

Preview writes capture policy and generation under one lock. Background scheduling setup runs once per thread, preventing repeated Linux niceness increments when processing successive previews. macOS validation does not exercise the Linux syscall branch.

## Validation

All 483 ordinary workspace tests pass, including the existing 164 GUI tests after moving inspection and image-preparation tests. Strict workspace Clippy, formatting, documentation-index generation/checking, and diff whitespace checks pass.

Original-file regressions cover exact decoded samples and camera metadata order across Classic, Merrill, and Quattro, folder prefetch yielding and family exclusions, large-preview orientation and cropped-camera-JPEG rejection, and small-thumbnail framing. GPU regressions cover sRGB display readback and row padding, plus exact regional crops and whole-frame continuations after Fast overviews and panning.

The rebuilt macOS app completed all twelve native Smart smoke captures. Receipts verify Smart before rendering, Fast overview, Full detail with the continuation running, whole-frame completion, panning without another render, refreshed photographic thumbnails, and cached reopening. The scope selector and inline Viewport control were also visually reviewed. Changes remain uncommitted for owner review.
