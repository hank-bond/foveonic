---
summary: "Reduced RAW integration keeps family recipes, shared preview policy and numerical node contracts consistent."
status: archived
---

# Review reduced RAW integration

Requirements: [X3F input](../../docs/specs/x3f-input.md). The owner requested an architecture and consistency pass over the working reduced RAW changes.

- [x] Review source loading, camera/layout interpretation, presentation geometry and graph construction.
- [x] Remove duplicated layout construction and reconstruction state without changing numerical algorithms.
- [x] Keep unsupported regional preview policy at the shared rendering boundary and retain existing full-resolution cache identities.
- [x] Run focused numerical, original-file and workspace checks and reopen the reviewed app.

Keep the established family recipes and typed graph. Preserve the distinct Classic, Merrill, Quattro HI and Quattro LOW numerical contracts. Avoid introducing a generic camera pipeline or duplicating a recipe for each resolution.

## Findings and changes

The existing family recipes remain the processing boundary. X3F decoding preserves original layouts and declared photographic dimensions. Source validation checks supported storage. Recipes select calibration, ordering and source-resolution controls. Numerical nodes receive extents, coefficients and explicit sampling choices. Classic and Merrill share their existing reduced-resolution recipes. Quattro LOW shares the established Quattro recipe while selecting its distinct preprocessing and scalar reconstruction controls.

Column-gain preparation now shares coefficient and geometry validation for triplets and packed cells. Calibrated extraction selects its numerical calibration once before applying the source layout. Stage2B derives output doubling and mask expansion from reconstruction geometry instead of retaining separate preview-grid and native-size flags. This also rejects regional resizing of LOW reconstruction through the HI-only helper. Sparse repair selects its dispatch count once and uses the same count for GPU timing capacity.

Unsupported regional requests now retain whole-frame rendering at the shared rendering boundary for every family. The LOW-specific policy was removed from Quattro's geometry helper. Exact image readback checks cover Merrill MED and Quattro LOW fallback. Existing HI spatial-gain keys retain their original structure. Per-pixel gains carry a distinct layout identity. Existing inspection output identities and numerical algorithms remain unchanged.

## Validation

Workspace tests pass, including 435 ordinary processor tests and 332 GUI tests. Strict Clippy passes. Focused GPU checks cover packed and RGB column gains, reduced blue reconstruction, saved rotation coordinates, complete DP2 Quattro and Quattro H synthetic LOW rendering/export, and real SD1 HI/LOW and DP3 LOW/MED color/monochrome processing. Four LOW sparse-repair worker cases still match exactly. Documentation, formatting and artwork checks pass. Unrelated help-example and release changes remain unchanged.

Independent reduced Classic and Quattro originals remain unavailable. This cleanup preserves the validation limits of the [implementation slice](./reduced-raw.md).

## Second review

A second pass found that unsupported region requests were cleared for recipe construction but retained on image artifacts. `RenderedImage` uses that region marker to skip saved composition, so the mismatch could suppress crop, fine rotation and output sizing after whole-frame fallback. The shared render boundary now normalizes the request before both recipe construction and artifact collection. Authored photographic controls remain intact.

The expanded Merrill MED and Quattro LOW regressions cover neutral framing and saved crop, a 17-degree rotation and explicit 700×1050 output. They verify view geometry, affine mapping, normalized region state and identical image samples against ordinary whole-frame rendering. The mismatch was reproduced before the fix. Both representative Quattro branches and all four Merrill originals pass afterward. Workspace tests and strict Clippy pass again.
