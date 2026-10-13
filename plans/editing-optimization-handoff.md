---
summary: "Continue native editing measurements with guarded GPU ownership while preserving the deferred mismatch evidence."
---

# Editing optimization handoff

## Current checkpoint

The owner requested continued full-quality editing-performance work and deferred the intermittent output-mismatch investigation. The isolated editing branch started at `d686266`; its maintained profiling work was committed as `088a8c2` on `main`. The owner subsequently approved integrating guarded storage, inspection differences, and sidecars into `main`, then removing the clean editing worktree. The unaccepted CPU tone-cache prototype is preserved separately and excluded from accepted source. No remote is configured.

Combined integration passed 61 ordinary core and 78 GUI tests, all 54 ignored core and seven GUI tests, strict checks, release workspace/example builds, index/format checks, and 21 Python tests. Six-original exact float/golden/TIFF comparisons, sidecar replay, inspection pairing, and continuous dragging passed. No new native performance benefit or mismatch fix is claimed.

This plan summarizes the measurements and rejected trial without depending on temporary filesystem paths. The more detailed historical measurement narrative is preserved in commit `088a8c2`; it is not a claim that those experiments remain implemented.

## Read first

- Repository and applicable crate/source `AGENTS.md` instructions.
- [Interactive rendering requirements](../docs/specs/interactive-rendering.md), [responsiveness plan](./ui-responsiveness.md), and [render resources](../docs/specs/render-resources.md).
- [GPU output mismatch](./gpu-output-mismatch.md) and [guarded GPU storage](./guarded-gpu-storage.md).
- [Profiling consolidation](./profiling-consolidation.md), [profiling requirements](../docs/specs/profiling.md), and [current usage](../docs/profiling.md).
- [Contribution conventions](../docs/contributing.md) before documentation changes.

## Boundaries

Prioritize native editing and GUI scheduling/presentation measurements. Relevant code is [worker](../crates/foveonic-gui/src/worker.rs), [viewport](../crates/foveonic-gui/src/viewport.rs), [navigator](../crates/foveonic-gui/src/navigator.rs), and [timing](../crates/foveonic-gui/src/timing.rs). Coordinate shared-file changes with other active work. Reference implementation access remains read-only; preserve attribution and evidence.

A separate worktree prevents source/build interference, not physical GPU contention. Never use timings collected during another render, benchmark, memory sample, or native smoke run as acceptance evidence. Do not stop the owner's application without permission.

Discuss material architecture, memory, and photographic tradeoffs before implementation. Preserve exact photographic equivalence and guarded ownership. No reduced-quality drag recipe, weakened corrections, photographic CPU fallback/readback, fixed checkpoint set, cache-all preference, or edits chained from the previous slider output. Export remains non-cancellable with editing disabled; scopes and navigator stay secondary to main-image feedback. Do not reopen the closed full-pipeline optimization pass as an unrelated project.

## Implemented editing baseline

- Processing-owned effective keys and upstream generations retain one active edit's complete unchanged continuation boundary, not every image buffer.
- One active render and one replaceable latest request, without debounce or FIFO backlog. Cancellation occurs only at safe preparation/operation boundaries.
- Images publish before optional revision-matched scopes. Older same-photo/root frames can advance feedback without replacing requested settings.
- Navigator preparation is GPU-only, deferred, bounded to a 512-pixel edge, and provenance/mapping safe. Pan and zoom do not regenerate tiles.
- Worker dragging covers Detail, Exposure, chroma/luma presets, and all seven advanced overrides. Exact cached/fresh comparisons cover the coupled recipes.
- Source timings distinguish file reading, in-memory decoding, and source opening. Warm edits reuse decoded input; uncontrolled local SSD cache state does not establish SD-card performance.
- Read/write guards cover storage, graph values, caches, retained images, and scratch recycling. No new production completion fences were introduced.

## Earlier complete-boundary measurements

Blue-sign native Detail feedback was approximately 125–143 ms; completed cached processing plus image preparation 119–131 ms versus 747–774 ms fresh. Six dispatches remained, including display preparation. Busy native frame gaps were 16.6 ms median, 45.8 ms p95, and 46.7 ms maximum.

Detail retained 783.6 MiB without sharpening or 845 MiB with sharpening. Settled physical footprint was approximately 2.1 GiB with a 3.5 GiB peak. Approximately 2 GB settled memory remains a soft target; retained bytes, RSS, allocation accounting, and physical footprint are different measurements.

Worker tests settled Detail/Exposure around 163/96 ms. Earlier noise/override edits needed 286–955 ms; brightness/layer mixing roughly 150–171 ms. These establish progress/settings/scopes correctness, not native pointer acceptance. Navigator preparation averaged 1.62 ms synthetically and commonly 1.7–3.3 ms warm natively. Synthetic prepared-tile drawing improved 0.169→0.122 ms, not proof that native stalls disappeared.

## Native pointer and guarded-baseline measurements

The first pointer baseline overlapped another GPU suite and was excluded from timing acceptance. A repeat used the unchanged `d686266` bundle without observed overlapping test/build/benchmark workloads. Conditions were blue-sign Sunlight/Standard, full-quality default noise/detail, no sharpening, Fit at 38% physical-pixel zoom, and a 1440×1000-point high-density window. Navigator was open; collapsed scopes still computed after settlement. Each control used three two-second drags with 61 pointer positions; loading, warm-up, screenshots, and memory samples were outside timed windows.

| Native median / p95 / max, ms | Detail | Exposure |
| --- | --- | --- |
| Completed worker image | 150.0 / 154.5 / 194.8 | 82.4 / 100.1 / 101.6 |
| Queue to UI receipt | 169.2 / 245.4 / 278.2 | 114.5 / 155.2 / 200.7 |
| Busy frame gap | 18.1 / 54.7 / 56.1 | 17.0 / 43.1 / 45.9 |
| UI update | 0.256 / 0.307 / 5.256 | 0.258 / 0.310 / 5.402 |

Detail published 12–13 images per drag and Exposure 23; all settled current. External release-to-receipt estimates were 178–287/151–202 ms, with pipe timing uncertainty, not screen-presentation measurement. No scopes published during dragging. Settled Detail physical footprint was 1.9 GiB with a 3.6 GiB process-lifetime peak.

The owner approved importing the cleaned guarded processor, excluding tone reuse and concurrent inspection changes. Its 52 core and five GUI ignored tests passed, including six-original float/golden/TIFF equality. No timing from that correctness run was acceptance evidence. Ordinary core/GUI inventories progressed from 52/61 to 52/65 with host probes, then 52/68 with GPU diagnostic setup tests; strict checks and local harness regressions passed.

Matched guarded, unprobed Detail/Exposure worker medians were 150.0/95.4 ms; queue-to-UI medians 170.6/125.4 ms; busy frame-gap p95 55.8/44.3 ms. All six drags settled current. Host-probed worker medians were 150.2/83.7 ms; sequential single captures do not establish overhead or improvement. A sampler initially misclassified its shell wrapper mentioning a later memory command; review excluded that false positive and three regression tests corrected executing-program classification. The owner's separate app remained running.

| Host diagnostic median, ms | Detail | Exposure |
| --- | --- | --- |
| Processing including existing waits | 130.6 | 56.6 |
| View preparation/submission | 0.230 | 0.265 |
| Existing view-completion wait | 19.0 | 25.3 |
| Latest-pending queue wait | 19.3 | 27.6 |
| Ready reply to UI receipt | 0.246 | 0.374 |
| Main callback preparation / encoding | 0.029 / 0.001 | 0.028 / 0.001 |
| UI receipt to first encoding end | 0.661 | 0.636 |

Callback preparation-to-encoding gaps had p95 about 16.2/16.1 ms and maxima 21.5/22.3 ms. Previous-frame eframe CPU estimates were about 0.79 ms median, excluding surface-acquire/submission/present waits. Settled footprint was 2.0 GiB with a 3.8 GiB lifetime peak, not a matched memory improvement. Host binding preparation is too small to prioritize; processing and view-completion waits remain material, but their causes are not isolated.

## GPU diagnostic findings

Explicit GPU mode requested supported timestamp queries; ordinary/host-only launches requested no extra features. Unsupported requested GPU capture failed at startup. Timestamp readback added synchronization, so diagnostic host spans and frame cadence are not unprofiled acceptance timings. Entry indices disambiguate repeated labels, not necessarily chronological dispatch order. Callback encoding does not prove submission, completion, or scanout.

All six timestamp-profiled pointer drags settled current without another sampled test/build/benchmark workload. The continuous-drag GPU-mode regression also passed.

| GPU stage median, ms | Detail | Exposure |
| --- | --- | --- |
| Residual detail mixing | 6.81 | skipped |
| Detail reconstruction | 4.74 | skipped |
| Tile projection | 21.90 | skipped |
| Tone | 4.31 | 10.73 |
| View | 6.13 | 8.27 |
| Processing sum | 37.94 | 10.73 |

These no-sharpening recipes used four processing passes plus view for Detail and tone plus view for Exposure. Diagnostic processing host medians were 123.8/54.2 ms; view preparation including timestamp resolution 18.0/19.4 ms, followed by approximately 2.55 ms completion wait. Allocation/initialization, uploads, CPU recipe preparation, queue delays, and profiler synchronization remain outside shader timestamps; subtraction is not a pure queue-wait measurement.

## Image-specific completion-wait trial: rejected

The owner approved testing only the GUI view-completion boundary with the view's actual submission token, preserving graph-operation waits, guards, shaders, priorities, and buffers. Serial correctness covered 53 core and five GUI GPU tests. A timeout interrupted the first core invocation after 31 passes; the remaining 22 passed separately, with coverage checked by unique test names rather than treating the interrupted run as complete.

Four native captures used before/after/after/before ordering, identical full-quality settings, and matched host probes without GPU profiling. All 24 drags settled current. No sampled test/build/benchmark workload overlapped interaction/settlement; flagged recorder memory commands ran afterward.

| Median, ms | Before 1 | After 2 | After 3 | Before 4 |
| --- | --- | --- | --- | --- |
| Detail worker | 115.66 | 115.28 | 117.90 | 117.59 |
| Detail view wait | 16.42 | 16.42 | 16.42 | 16.46 |
| Exposure worker | 71.18 | 72.47 | 72.41 | 71.79 |
| Exposure view wait | 17.68 | 18.87 | 18.88 | 17.68 |

Detail had no repeatable gain; Exposure was 0.62–1.30 ms slower. Callback-gap p95 stayed about 35–37/29–30 ms. Final before/after footprint was the same coarse 1.4 GiB with a 3.4 GiB lifetime peak. Earlier sessions' worker or memory figures are not matched comparisons to this trial.

The completion API/prototype/test were removed; source hashes matched the pre-prototype guarded/probed baseline. No performance optimization from this trial remains. It does not rule out graph-operation wait or implicit initialization costs. Do not replace those waits or add persistent image caches without owner review.

## Maintained diagnostics and remaining work

Historical flags and text formats were replaced by off/wall/GPU modes, shared JSON reports, and maintained [profiling scripts](../scripts/profiling/AGENTS.md). Forced busy repaint is removed; source loading is separate and CPU preparation accompanies GPU diagnostics. The rejected indexed-wait optimization is not retained.

- [x] Capture native pointer progress, settlement, frame gaps, and separate settled memory.
- [x] Separate host and GPU diagnostic observations; test and reject the indexed view-wait trial.
- [x] Consolidate useful profiling capabilities without changing photographic arithmetic.
- [ ] Diagnose costs outside measured shader passes before proposing the smallest next optimization.
- [ ] For any approved change, run matched completed unprofiled wall, native interaction, memory, and exact-output checks.

## Deferred mismatch

Historical red-flower failures followed Shadows, Detail, and cancellation/resume edits. Shadows differed in 34,382,750 RGB components (71.25%), maximum 1.177125; resume differed in 11,213,165 (23.24%), maximum 0.0087557435, initially near sensor coordinate (1229,2444). These were processing floats, not TIFF codes. CPU tone equality passed in one failing run.

Frozen failures also occurred without tone reuse; a repeated fresh render later matched cached output in full. Matched guarded/original-storage suites passed, but neither the cause nor first divergent stage is known. The owner deferred investigation; passing tests do not establish a fix. Keep strict equality and bounded failure reporting. Reopen on a regression failure or reported symptom, not by broad diagnostic reruns or weaker tolerances.
