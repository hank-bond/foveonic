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

The initial presentation change leaves numerical processing, scheduling and disk caching unchanged. The owner-reported registration correction below also fixes final reduced photographic framing. More than one retained native section remains deferred.

## Results

The loupe now draws the overview and latest section through the same native-coordinate placement and draw helper used by zoom overlays. Each layer has an independent GPU binding slot. Circular clipping is derived from the complete lens for each clipped draw. Section overlap stays useful for display while complete visible coverage remains necessary to settle the native request.

One shared frame predicate handles overlay compatibility and replacement. Same-edit reduced overviews preserve retained native detail despite later delivery. Newer detail may advance existing progressive feedback over an older overview. Newer edits, incompatible framing/source/calibration, or a complete native overview supersede old detail. The existing photo owner still retains one section and uses its original texture and memory accounting.

Validation passes 522 ordinary GUI tests and a real-worker GPU ownership check using the maintained blue parking sign. The ownership check uses Draft followed by Thumb to obtain a real later reduced delivery. Identical settled requests correctly coalesce without another delivery. It checks source, calibration, rotation and exposure mismatches, one-section replacement, overview refresh and complete-native replacement.

The maintained two-photo native transition journey now holds replacement delivery while the loupe moves partly outside its native section, then checks retained overview/detail identities, incomplete overlapping coverage and a captured frame. Its single-photo Smart segment also zooms and pans while replacement is held. Both screenshots were produced and visually inspected. Python independently requires these observations and screenshots, checks distinct image identities and verifies partial overlap. Whole-only recipes explicitly report their exemption from the regional zoom check. The final Wall capture is complete, loss-free and uncontended. Controlled pauses verify behavior and are excluded from latency claims.

Independent largest-model review found no remaining blockers. Strict release workspace/all-target Clippy, formatting, generated indexes and 156 Python profiling checks pass, with one platform-dependent Python check skipped. The packaged build is ready for owner testing. Upstream sectional optimization remains paused for that review.

## Registration follow-up

Owner testing found a visible shift when detail replaced the overview or previous section. Two failing regressions isolated independent coordinate errors. Merrill and Classic final reduced framing treated pooled centers as crop edges, displacing the photograph by half a working pixel. Separate GPU callback rectangles also rounded independently to physical pixels, creating smaller shifts between sections.

- [x] Share native-edge to working-sample framing between Merrill and Classic. Keep Quattro's existing sample-index mapping.
- [x] Draw layers through a common callback canvas so section coverage cannot change their raster alignment.
- [x] Reproduce and correct pooled-center and raster-rounding regressions, including fractional crop edges, section bounds and pixel densities.
- [x] Verify Full samples, retained-layer behavior and the packaged review build.
- [x] Obtain independent largest-model design and Rust review.

The GPU ramp check exercises actual pooling and photographic resampling at divisors two, four and eight. Its tolerance allows the existing sixteen-phase interpolator's sub-cell approximation while rejecting a half-cell displacement. Full analysis branches request internal linear outputs before final reduced framing, so this correction needs no legacy analysis path.

Validation passes 557 processor tests and 523 GUI tests. All ten Full photographs remain bit-exact against the frozen camera-family matrix, as do the twelve additional Quattro and Tiger reduced outputs. Full Natural Adaptive Tone with Dehaze also remains bit-exact for the hazy coast and SD14. The real-app transition journey passes with fresh numerical comparisons, a complete uncontended trace and both held-overlap screenshots. Independent review found no blockers. Strict affected-package/all-target Clippy, formatting and documentation checks pass.

The existing display-preview signature is advanced so persisted overviews cannot preserve the old framing. This regenerates cached embedded and rendered display previews across families while preserving decoded sensor entries. Owner review of the corrected replacement behavior remains pending.
