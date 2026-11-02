---
status: archived
summary: "Correct the recursive noise filter's missing border-weight conversion and verify enlarged Detail support."
---

# Keep Detail filter borders finite

Requirements: [Detail and noise](../../docs/specs/detail-and-noise.md) and [reduced reconstruction](../../docs/specs/reduced-reconstruction.md).

- [x] Trace the reported black bars through the filtered base and Detail pyramid.
- [x] Map the complete local recursive footprint while preserving global filtering boundaries.
- [x] Add a strong-gradient regression and update the reference comparison's corrected halo contract.
- [x] Verify both supplied photographs, the Fine activation case and shared repository checks.
- [x] Rebuild and open the native app for owner review.
- [x] Owner approved closing the fix and committing it with the remaining Detail and banding work.

The edge lookup omitted one column at each side of the local mean footprint. Raw gradients above one entered its recurrence, producing nonfinite border samples. Larger Detail supports spread those samples into tile stitching. Fine activation could expose the shared Coarse pyramid even with a neutral Coarse amount.

The fix maps these columns through the existing bounded lookup. It preserves global filtering's one-column gradient boundary. The reference fixture intentionally changes at the corrected local border. Its check retains the unchanged restored interior, explicitly converts the affected transposed backing-seed entries and checks finiteness. A long-column regression validates bounded coefficients and constant preservation rather than matching the defective border.

Both reported photographs render without black bars across five Coarse sizes from one through two. The Fine amount sweep has no nonfinite change-map values anywhere in the image. The first photograph's size-one and size-1.49 display pixels match the retained originals exactly. The second differs by at most one 8-bit level in those controls. Seven reconstruction GPU checks, workspace tests, strict Clippy, formatting, documentation checks and five artwork checks pass. The normal native app is rebuilt and launched on the first reported photograph. Preview cache identity advances so old rendered previews are regenerated.
