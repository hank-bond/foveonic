---
summary: "Repair Compact speckles across all three sensor layers using shared donors and whole-pixel highlight protection."
status: archived
---

# Shared Compact speckle repair

The owner approved replacing independent layer repairs with one pixel decision and shared three-layer donors. Requirements: [Merrill speckles](../../docs/specs/merrill-speckles.md).

- [x] Confirm that independent thresholds and radii can partially repair a white speckle.
- [x] Repair all three layers at the smallest radius where any layer detects an outlier. Use the first triggering layer in sensor order to select the same six trimmed ring donors for every layer.
- [x] Preserve the whole pixel if any layer has protected original highlight evidence.
- [x] Verify unequal thresholds, shared donor weights, single-layer defects, clusters, lines, highlight protection and immutable inputs with GPU regressions.
- [x] Run repository checks and record their results.

## Findings

The previous Compact shader chose a radius and trimmed donor values independently for each layer. The revised filter keeps detection thresholds per layer while sharing the repair radius, donor locations and equal weights. Equal-valued extremes use the first minimum and last maximum so two distinct donors are excluded. Detection and repair read immutable input. The preview format version is bumped so old Compact results are regenerated.

## Validation

All six Compact GPU tests pass. They cover white speckles with unequal layer thresholds at sizes 3, 5 and 7, shared donors with unequal and tied extrema, different layer support radii, single-layer defects, compact clusters, lines, original-code clipping protection and unchanged input buffers. Protected highlights now include outliers in the other two layers to verify whole-pixel preservation.

The two reference GPU regressions also pass, retaining ordered repair and matched threshold behavior. Workspace tests, strict all-target Clippy, formatting, generated documentation checks and all five artwork checks pass. GPU checks require Metal access outside the sandbox. No owner photograph was supplied for this revision, so its visible result remains for photographic review.
