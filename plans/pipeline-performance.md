---
summary: "Pause algorithm expansion, stabilize a buildable baseline, measure completed cold/warm renders and GPU stages, then optimize the largest bottlenecks with output-equivalence checks."
read_when: "Resume the owner-approved profiling and optimization pivot; remaining algorithm work is paused."
---

# Profile and optimize the pipeline

## Agreed scope

The owner reported unacceptable processing latency and approved a pivot: finish only the minimal in-flight wiring needed for a buildable baseline; measure cold/warm wall time, GPU stages, synchronization and memory with reconstruction off/on; optimize the largest measured bottleneck and check output equivalence after each change. Algorithm expansion in [the completion plan](./complete-reference-algorithms.md) is paused. The owner authorized branch `perf/pipeline-latency` and commits for successfully checked changes, with final difference review afterward. Keep cleanup, profiling-only work and optimization in separate commits; review diffs and validate before committing. Work autonomously through measured, output-equivalent rounds until returns diminish or complexity outweighs the incremental gain. Profile the full pipeline with all processing enabled. UI responsiveness and caching are follow-up work, not part of this pass. Record output-changing alternatives for later owner review. Installs, reference edits and evidence deletion remain unauthorized; preserve concurrent GUI work.

## Steps

- [x] Stabilize partial shared noise/ColorDQ wiring; check shared GUI/CLI callers and record baseline semantics.
- [x] Commit the validated cleanup checkpoint, excluding unrelated GUI work.
- [ ] Capture completed, unprofiled cold/warm renders, a separate timestamped stage breakdown, and process peak memory for representative/default and reconstructed routes.
- [ ] Capture/review/commit profiling-only work before implementing optimization.
- [ ] Optimize the largest measured stage with equivalent output; preserve photographic selection/order, correction controls, and retained ownership.
- [ ] Verify component/native and photographic output equivalence; rerun the identical benchmark and quantify improvement.
- [ ] Review the next bottleneck, latency/synchronization/memory tradeoffs, and remaining performance limits.
- [ ] Run workspace/GPU/GUI/CLI/strict/documentation checks and commit successful rounds on the branch.

## Baseline constraints

Timestamp profiles currently resolve/read back per operation and therefore synchronize frequently. Use completed unprofiled wall time as the primary latency measurement; report profiled times separately. Wait for the queue before declaring render completion. Engine creation/decoding, rendering, display texture preparation and readback/export are separate costs. Warm means a reused engine and decoded source; it does not imply image-result caching.

Earlier measured expensive stages are ordered camera records (~0.79 s), masked cluster matching (~1.90 s), and scene-dependent highlight donor accumulation (up to ~2.10 s). These are hypotheses until the current build is measured. Do not replace dependent repairs with simultaneous updates or unordered donor statistics merely to improve time. Parallel candidate evaluation/reduction can retain sequential member writes and strict first-best tie ordering.

## Progress

Minimal wiring retains native automatic ColorDQ only with opt-in reconstruction, manual `Some(gain)` with 0 Off, and shared metadata-derived despeckle dose using the actual prepared calibration WB factors. GUI/CLI/catalog callers are migrated. This is stabilization of existing partial work, not authorization to continue adding resampling or other algorithm stages. Current scale tables give dose 1 for the six originals. Ordinary workspace tests, strict all-target Clippy and release build pass. All 40 ignored core GPU/fixture tests and the GUI worker test pass. Thirty native recipe cases now check despeckle dose/ColorDQ limit as well as reduction/window. The new fixture assertion caught the in-flight ColorDQ column mixup: native column 7 is the cap, not column 12 (sharpening base). Corrected before the baseline checkpoint; preliminary reconstruction hashes before this fix are not equivalence goldens. Automatic ColorDQ is bit-identical to its explicit calculated-gain route, including retained-image checks. Non-unit despeckle dose, invalid dose and geometry-independent coupling preparation are checked. Preliminary diagnostic measurements exist under `/tmp/foveonic-performance-baseline-*`; no optimization code has been changed. A submitted render is not a completed render: preliminary default blue-sign submission is ~80 ms while completion is ~3.25 s, mainly cluster (~1.9 s) and ordered record (~0.8 s) GPU work; flower defaults complete in ~5.33 s with an additional ~2.10 s GPU donor-table loop. These are initial findings, not the finished profiling checkpoint. Image data stays device-resident through processing; timestamp resolution reads small timing buffers and introduces synchronization only with profiling enabled.

Cleanup is reviewed independently from the concurrent GUI work: selectively staged controls/smoke API migrations compile and pass workspace tests and strict Clippy in `/tmp/foveonic-cleanup-review`. Forty core ignored GPU/fixture tests and the GUI worker test pass in the actual tree; staged core sources are identical. Documentation generation/check, formatting, diff checks and eight Python tests pass. Owner viewport/help/scopes/inspection work stays uncommitted. Evidence: `/tmp/foveonic-cleanup-{workspace,gpu-current,clippy-current,review-workspace,review-clippy,review-docs,python}.log`.

Latest available disk is approximately 26 GiB. Keep logs/compact measurements and avoid duplicate full-frame TIFF exports. Retained originals and review evidence remain untouched.
