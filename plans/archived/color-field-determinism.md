---
summary: "Make color-field GPU solver row ordering deterministic without changing equations, tolerances, Classic processing or per-image controls."
status: archived
---

# Deterministic color-field solver ordering

The owner approved deterministic GPU row ordering after the [production investigation](../color-field-production.md) captured a small recovery-rounding difference crossing the downstream speckle detector's strict boundary. Preserve the [accepted recovery requirements](../../docs/specs/color-field.md), all numerical bounds, solver iteration caps, compensated color arithmetic and exactly 64 Local brightness sweeps. Do not change Classic or its detector.

## Design

Use ascending sensor-pixel order for native unknowns and ascending grid-cell order for color unknowns. Eligibility and equation arithmetic remain unchanged. Replace unordered atomic row allocation with integer GPU compaction: workgroup-local ranks, a deterministic prefix of block totals, then scatter into the existing position/index maps. Reuse the same small block-total buffer for the two domains. Keep the compaction arithmetic in WGSL and use guarded resource reflection for the new kernels.

## Steps

- [x] Add and register rank, prefix and scatter kernels.
- [x] Integrate both domains without changing equations or solver acceptance.
- [x] Test empty, sparse, full and partial workgroups across prefix partitions, including repeated compaction and reused buffers.
- [x] Validate strict fresh/cached highlight and Output comparisons and saved recipes.
- [x] Validate the full numerical corpus, severe clipping, healthy samples and Local sweep count.
- [x] Measure the feature cost against the frozen main-integrated candidate.
- [x] Run the full suite, recording separate existing exact-cache failures without retries or relaxed assertions.
- [x] Owner reviewed the validation summary and approved commit and merge. Keep the separate Classic exact-cache failure open without weakening its assertion.

## Findings and checks

Both domains now have ascending physical row order. GPU eligibility markers feed a 256-thread local integer scan, a single-workgroup partitioned prefix over block totals, and a parallel scatter. The same approximately 256 KiB block-total buffer is reused for native pixels and grid cells. No equation, stopping tolerance, iteration cap, recovery constant, control or Classic shader changed. An initial shader-validation check rejected a reserved local variable name. Renaming it corrected that setup error before GPU validation.

The compaction test covers empty, full and sparse eligibility, partial groups, counts across the 256-block prefix boundary, repeated runs, reused block buffers, both counter slots and preservation of unrelated counters. All 13 initial color-field GPU tests passed. The previously failing Local/selectivity-0.25 comparison now has zero difference in both highlight layers and Output. All Global and Local settings in that regression, as well as the six saved-setting/DNG steps, are exact.

An additional corpus regression completed 128 fresh/cached pairs: sixteen originals, two brightness methods, and selectivity 0, 0.25, 1 and restored 0. All 256 comparisons of highlight and final images were byte-identical. Input sidecars remained unchanged. The sixteen full-raster identical-evidence CPU comparisons still pass the original `2e-4` bound. Maximum normalized error is approximately `8.85e-5`. Healthy-sample, mixed-evidence, compensated-solver and exactly-64-sweep checks passed. The previously failed exact saved-recipe replay also passed without changing its fixture or assertion.

The performance check interleaved two old/new execution orders over five scenes and three methods, with six warm samples per version and case. Geometric-mean wall-time changes were approximately +2.20% Global, +1.62% Local and +0.23% Classic. The largest observed change was 0286 Global, approximately 650 to 701 ms (+7.9%). This is a small benchmark, not a claim of a new speed improvement or guaranteed cost.

The complete serial core GPU run passed 88 of 89 tests. Color Field and saved-recipe replay passed. The sole failure was the existing Classic recovery-threshold cached/fresh comparison: 31,019,819 differing components, maximum `0.9444937`. The two exact-cache tests that failed in the preceding run passed this time, which does not establish fixes for them. One pristine-main control run of the threshold test passed. That control does not clear the candidate failure or identify its cause. The candidate failure was frozen and not repeatedly retried. Classic processing has not been changed by this work.

Workspace, tool, strict all-target Clippy, formatting and all eight GUI GPU tests passed. Remaining integration acceptance is the separate Classic exact-cache issue recorded in the [mismatch plan](../gpu-output-mismatch.md). The owner subsequently approved committing and merging this candidate while retaining the Classic failure as an open issue. App installation, launch and publication remain outside scope.

## Closure

This completed implementation or experiment is archived during release 0.3.0 preparation at the owner's request. Earlier scope, review and working-tree statements describe that historical slice. Recorded limitations and separately tracked follow-ups remain explicit.
