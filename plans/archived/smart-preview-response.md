---
summary: "Show Full viewport detail first for zoomed Smart edits while retaining the previous image."
status: archived
---

# Smart preview editing response

Requirements: [Smart preview](../../docs/specs/smart-preview.md), [interactive rendering](../../docs/specs/interactive-rendering.md), and [cropping](../../docs/specs/cropping.md).

- [x] Compare fresh Fast-first and regional-first sessions with completed work and exact regional and whole-frame checks.
- [x] Implement regional-first zoomed edits when an existing frame has compatible geometry and the requested recipe supports native regions.
- [x] Exercise rapid supersession, regional presentation, and continuation in an isolated native Smart smoke.
- [x] Check direct whole-frame Full feedback for a zoomed Adaptive edit.
- [x] Exercise crop transitions and zoom changes during an active regional request.
- [x] Complete warm editing measurements, independent review, and repository checks.

The prior frame remains visible while the newest regional render runs. A matching regional result overlays it and starts whole-frame Full. A compatible zoomed edit whose recipe requires whole-photo processing requests Full directly. Adaptive retains its existing regional restriction. Unknown geometry, source changes, incompatible framing, explicit output resizing, and fitted or 100% views keep the existing route. One active and one replaceable pending request remain unchanged.

On one portrait Merrill original and an M1 Pro, a monitored warm exposure comparison used one warmup and three alternating pairs per viewport. Regional-first versus Fast-first reached Full detail in median 209/372 ms for a region spanning 25% of each axis and 324/488 ms for 50%. Whole Full completion took 652/834 ms and 770/932 ms respectively. Checked regional and whole-frame samples matched independent Full output exactly. No monitored workload was flagged, but owner apps remained open and monitor coverage was limited. These are completed engine-ready timings, not screen presentation or a general camera and hardware benchmark. The [campaign record](./responsiveness-campaign.md) describes the measurements and their limitations.

The native Smart smoke submits two immediate exposure edits at 200% zoom. The newest regional Full image arrived over the previous Full frame and the matching whole Full image followed. The settled scope and browser-preview revisions matched. This covers one source and one edit sequence, not physical pointer latency or transient memory.

An additional native stage enables Adaptive at 200%. Its first matching image is whole-frame Full, with no regional request and matching scope and browser-preview revisions. The smoke rejects a matching Fast frame during that transition. Combined workspace, lint and documentation checks passed as recorded in the campaign. An additional owned native smoke queued regional edits before changing zoom and entering or leaving Crop. At Fit it hid regional detail and settled with current settings and matching scopes. After leaving Crop, the browser preview also matched, and a subsequent whole-frame Full render retained the same pairing. The smoke checked one Merrill original and does not measure physical pointer latency or transient memory.
