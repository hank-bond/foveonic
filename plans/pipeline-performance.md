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
- [x] Capture completed, unprofiled cold/warm renders, a separate timestamped stage breakdown, and process peak memory across six originals with all processing enabled.
- [x] Capture/review/commit profiling-only work before implementing optimization.
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

## All-enabled baseline

Cleanup checkpoint: `43fcd2e`. The reproducible example enables camera repair, highlights, despeckle, row correction, reconstruction (including column/pattern), automatic ColorDQ, native sharpening 0.75, gain 1.3, Exposure 0.5, nonneutral tone/saturation, and oriented 800×1200/1200×800 encoding/resizing/TIFF. Three completed unprofiled renders are followed by a separate timestamped render. Warm uses the same decoded source and engine, not result caching. New-process setup may reuse the driver's persistent shader cache; the first engine took 1022 ms, subsequent engines 44–54 ms.

| Original | Decode ms | First render ms | Warm median ms | Timestamped ms | Warm export ms | Process peak MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| blue-parking-sign | 268 | 4156 | 3912 | 3980 | 13 | 889 |
| glossy-leaves-and-buds | 243 | 4034 | 4114 | 4143 | 15 | 1040 |
| rocks-and-tree-shadows | 242 | 3695 | 3697 | 3825 | 21 | 922 |
| sunlit-red-flower | 240 | 6222 | 6244 | 6288 | 13 | 1054 |
| turquoise-still-life | 243 | 3923 | 4125 | 4089 | 13 | 1053 |
| yellow-flowers-and-stroller | 243 | 5666 | 5729 | 5727 | 16 | 895 |

Every unprofiled render has **zero CPU readbacks**, 62–64 compute passes, 64–66 engine submissions, ~264 MB upload (source and prepared tables), and ~275 MB device-local copies. Export adds one 7.68 MB readback; timestamp profiling adds 18–21 reads totaling just 992–1024 bytes, plus resolve submissions. Diagnostic golden readback is outside timing and activity. The latency is dominated by serial GPU work, not intermediate image round-trips. Total wrapper storage allocation is 4.89–5.67 GB per render; this is cumulative allocation, **not peak residency**. RSS also does not fully represent Metal device storage.

Largest common stages: masked clusters ~1.89–1.90 s, ordered camera lists ~0.79 s. Flower/yellow donor tables add 2.10/1.47 s. Next costs include edge weights (55–365 ms), row correlation/fits (combined across banks), directional local means, bilateral, and scene-dependent despeckle. GPU sums account for 3.61–6.09 s of timestamped runs; remaining preparation/submission/waits are materially smaller. All host/GPU stages remain in JSON logs for the full-pipeline pass.

```sh
cargo build --offline --release -p foveonic --example profile_pipeline
/usr/bin/time -l target/release/examples/profile_pipeline \
  /Users/work/git/foveonic/references/x3f/blue-parking-sign.x3f \
  .local/pipeline-performance/ROUND/exports \
  .local/pipeline-performance/baseline/goldens
```

Initial goldens are created only when absent. Further rounds fail on any changed full-buffer byte. Keep baseline files: `.local/pipeline-performance/baseline/{logs,exports,goldens}`. Small exports also permit direct encoded-TIFF comparison. Counters describe engine-owned work, not hidden driver staging or other shared-device users. Wrapper counter tests, workspace tests, strict Clippy and documentation/format checks pass before this instrumentation-only checkpoint.

## Round 1: cooperative cluster candidate search

Independent candidate searches run in one 64-lane workgroup; clusters/member updates remain sequential with storage barriers. Each lane retains native float accumulation order. Reduction chooses lowest cost then original row-major/mapping rank, preserving strict first-best ties. Host metadata supplies a uniform maximum member loop bound; unused slots do no repairs. No new submissions or readbacks.

GPU matching fell from ~1890–1900 ms to 29.5–29.8 ms. Completed warm all-enabled renders now measure 1994/2196/1850/4377/2174/3752 ms (blue/leaves/rocks/flower/turquoise/yellow), versus 3912/4114/3697/6244/4125/5729 ms. Six full-sensor float buffers and all six resized TIFFs are **byte-identical** to baseline. Native/order/capacity/dark-defect component regressions, explicit cross-lane first-tie/no-candidate/uniform-padding tests, workspace and strict Clippy checks pass. Evidence: `.local/pipeline-performance/{clusters,clusters-warm}` and `/tmp/foveonic-opt-clusters-*.log`. Work remains GPU-only with immutable earlier artifacts.

## Round 2: stable GPU donor bin grouping

A direct per-bin private recurrence preserved exact output but only lowered flower/yellow table work from 2104/1468 to ~1379/966 ms. Splitting workgroups gave little further benefit. Materializing floating donor ratios in compaction failed the strict flower image comparison and was rejected (likely changed contraction/rounding; not fully isolated). Caching bin labels alone was exact but slower. Evidence for all attempts remains under `.local/pipeline-performance/highlight*`; rejected ratio-source files are preserved in `highlight-cached/source`.

The selected path uses integer block histograms/prefixes and stable scatter, retaining each bin's original donor sequence. It reuses old rank scratch for bin labels after compaction completes. Ratio arithmetic remains inline in ordered summation. Source pixels/maps are unchanged; no photographic host arithmetic or readback is added. Two integer buffers add ~128.7 MB cumulative storage, and four passes/submissions replace the long serialized table loop. Flower/yellow table work is ~421/211 ms, plus ~14/11 ms grouping; all-enabled warm renders are 2059/2123/1859/2648/2073/2470 ms (blue/leaves/rocks/flower/turquoise/yellow). Common scenes add ~1–2 ms grouping overhead. Further per-bin serial latency is visible and can be investigated after the camera-record bottleneck.

All six full float buffers and TIFFs remain byte-identical to the original all-enabled baseline. Stable grouping across block boundaries/partial tails/empty bins, original saturation/connectivity/density/gain/selectivity, all **43 core ignored GPU/fixture tests**, GUI worker, ordinary workspace and strict Clippy checks pass. Evidence: `.local/pipeline-performance/highlight-sorted-warm` and `/tmp/foveonic-opt-highlight-*.log`.

## Output-changing alternatives for later review

No alternatives are implemented in the equivalent-output pass. Possible later choices include block-partial donor sums (different float32 association), simultaneous rather than sequential defect updates (different adjacent repairs), or a simpler modern edge/noise model instead of the native processing recipe. These require photographic review and explicit acceptance; disabling corrections or lowering noise quality is not an optimization in this pass.

Latest available disk is approximately 26 GiB. Keep logs/compact measurements and avoid duplicate full-frame TIFF exports. Retained originals and review evidence remain untouched.
