---
summary: "Keep the latest native section visible over an overview while loupe and zoom request replacement coverage."
---

# Layered detail presentation

Implement the approved [viewport presentation requirements](../docs/specs/viewport-workflows.md#detail-rendering-and-ownership) before continuing upstream sectional optimization. Loupe and double-click zoom share image-coordinate layer drawing. Keep the existing overview and one latest native section, using their existing texture ownership and memory accounting. Preserve render-request completeness independently of display overlap.

- [x] Share native-coordinate layer placement and GPU drawing between viewport overlays and the circular loupe, with distinct callback binding slots for each layer.
- [x] Retain compatible detail through partial overlap and redundant reduced-overview delivery. Preserve current progressive edit feedback without showing older edits over a newer overview.
- [x] Check mapping, clipping and edit/source compatibility. Extend native held-delivery gestures across a section edge for loupe and persistent zoom.
- [x] Run related ordinary/native tests and obtain independent largest-model review.
- [x] Package the owner review build before resuming upstream work.
- [ ] Obtain owner visual review before resuming upstream work.

No numerical pipeline, scheduler or disk-cache change belongs to this slice. More than one retained native section remains deferred.

## Results

The loupe now draws the overview and latest section through the same native-coordinate placement and draw helper used by zoom overlays. Each layer has an independent GPU binding slot. Circular clipping is derived from the complete lens for each clipped draw. Section overlap stays useful for display while complete visible coverage remains necessary to settle the native request.

One shared frame predicate handles overlay compatibility and replacement. Same-edit reduced overviews preserve retained native detail despite later delivery. Newer detail may advance existing progressive feedback over an older overview. Newer edits, incompatible framing/source/calibration, or a complete native overview supersede old detail. The existing photo owner still retains one section and uses its original texture and memory accounting.

Validation passes 522 ordinary GUI tests and a real-worker GPU ownership check using the maintained blue parking sign. The ownership check uses Draft followed by Thumb to obtain a real later reduced delivery. Identical settled requests correctly coalesce without another delivery. It checks source, calibration, rotation and exposure mismatches, one-section replacement, overview refresh and complete-native replacement.

The maintained two-photo native transition journey now holds replacement delivery while the loupe moves partly outside its native section, then checks retained overview/detail identities, incomplete overlapping coverage and a captured frame. Its single-photo Smart segment also zooms and pans while replacement is held. Both screenshots were produced and visually inspected. Python independently requires these observations and screenshots, checks distinct image identities and verifies partial overlap. Whole-only recipes explicitly report their exemption from the regional zoom check. The final Wall capture is complete, loss-free and uncontended. Controlled pauses verify behavior and are excluded from latency claims.

Independent largest-model review found no remaining blockers. Strict release workspace/all-target Clippy, formatting, generated indexes and 156 Python profiling checks pass, with one platform-dependent Python check skipped. The packaged build is ready for owner testing. Upstream sectional optimization remains paused for that review.
