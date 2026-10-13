---
summary: "Continue native editing measurements with guarded GPU ownership while preserving the deferred mismatch evidence."
---

# Editing optimization handoff

## Assignment

The owner requested parallel work: this agent continues editing-performance optimization; the diagnosing agent retains the intermittent cached/fresh output mismatch. New GUI diagnostics are deferred to a later pipeline-visualization view. Prioritize full-quality slider-to-main-image feedback and the baseline editing workspace.

Start from **`d686266`** on `perf/pipeline-latency`. The reviewed follow-up commits include read/decode profiling, independent capture metadata, organizer keyboard navigation, simplified photographic controls/help, and broader drag/navigator measurements. All completed concurrent-agent changes are committed. Local `main` is an older checkpoint; no remote is configured.

Use a separate worktree/branch from that commit and a separate Cargo target directory. Keep the shared worktree untouched. Uncommitted imports require owner approval; the owner subsequently approved refreshing this branch with the cleaned guarded processor (see below), excluding the tone-cache prototype and concurrent inspection GUI changes. Cargo reused an earlier snapshot's binaries during the first review harness because copied source mtimes were older than shared artifacts. Final review explicitly rebuilt sources in isolated artifacts and checked the expected test inventories.

## Read first

- Repository and applicable crate/source `AGENTS.md` instructions.
- [Interactive rendering requirements](../docs/specs/interactive-rendering.md).
- [Responsiveness plan](./ui-responsiveness.md), including evidence and unresolved acceptance.
- [Render resources](../docs/specs/render-resources.md) and [GUI workspace](../docs/specs/gui-workspace.md).
- [Contribution conventions](../docs/contributing.md) before documentation changes.

The reference repository `/Users/work/git/foveonic` is read-only, including owner edits. Preserve attribution when importing code and retain existing evidence.

## Ownership and coordination

The diagnosing agent owns the shared processor and mismatch notes. Its guarded ownership migration is now retained, temporary investigation probes are removed, and the mismatch is deferred without an established cause or fix: see [GPU output mismatch](./gpu-output-mismatch.md) and [guarded GPU storage](./guarded-gpu-storage.md). The owner approved importing that cleaned guarded processor into this isolated branch. Keep `tone.rs` at `d686266` and omit the tone-cache fields/hooks from imported `interactive.rs`/`render.rs`; that prototype remains outside this work. Do not edit the shared originals, reopen diagnosis unasked, or treat passing runs as proof of a fix.

Begin with **native editing and GUI scheduling/presentation measurements**. Likely investigation sites are `crates/foveonic-gui/src/worker.rs`, `viewport.rs`, `navigator.rs`, and adjacent tests. An inspection-differences agent has also begun changing the shared worker (paired roots, comparison textures and request identity), so these are not exclusive ownership assignments: coordinate with that agent before implementation. Use separate measurement/test files first. Coordinate before changing processing shaders or reconstruction code: the mismatch's origin is not known, and such changes may interfere with diagnosis. Keep optimization findings in this plan rather than concurrently editing the diagnosing agent's plan.

A separate worktree prevents source/build interference, not physical GPU contention. Coordinate GPU-heavy runs with the diagnosing agent. Never use timings collected during another agent's render, benchmark, memory sampling, or native smoke run as acceptance evidence. Do not stop the owner's running application without permission.

Review and discuss material architecture, memory or photographic tradeoffs before implementation. Do not introduce a compatibility path to preserve a replaced design. Do not commit, merge or publish unfinished optimizations as validated improvements.

## What is already done

- Processing-owned exact effective keys and upstream generations select one active edit's complete unchanged continuation boundary. Cheap version records do not imply retaining all image buffers.
- One active render and one replaceable latest pending request, no debounce/FIFO backlog. Safe preparation/operation boundaries permit supersession; submitted GPU work cannot be forcibly cancelled.
- Images publish before optional revision-matched scopes. Completed same-photo/root older frames can advance feedback without adopting old requested settings.
- Navigator preparation is GPU-only, deferred after main publication, and suppressed while image processing is busy. The maximum-512-edge tile uses ordinary egui painting and source/revision/mapping-safe publication. Pan/zoom/scroll do not regenerate it.
- Continuous worker dragging now covers Detail, Exposure, chroma/luma presets and all seven advanced overrides. Cache/fresh full-float comparisons cover the corresponding noise/override recipes.
- Profiling separates complete raw-file reading into memory, in-memory X3F decoding and total source opening. Filesystem cache state is uncontrolled. Warm editing reuses decoded data; historical load timings must not be charged to each edit. Local SSD reads do not characterize slow SD-card openings.
- The output-equivalent full-pipeline performance pass is closed; do not reopen it as an unrelated optimization project.

## Remaining performance work

1. Establish native pointer-driven slider feedback and frame-gap baselines from the frozen checkpoint, distinguishing UI handling, completed image delivery and shared-GPU drawing stalls.
2. Profile remaining main-image preparation/presentation and worker scheduling costs. Explore the smallest measured improvement first; allocation/binding overhead and presentation waits are candidates, not established bottlenecks.
3. Review any change to display storage, submission granularity or operation boundaries before expanding the architecture. Preserve full-quality arithmetic, immutable artifacts, source/revision pairing, clipping, orientation and channel/signal mappings.
4. Measure completed unprofiled wall time before/after under matched warm conditions. Timestamp-profiled runs synchronize and are diagnostic, not wall-time acceptance. Measure memory and native interaction separately.

No reduced-quality drag recipe, weakened corrections, photographic CPU processing/readback, fixed checkpoint set, cache-all preference, or chained editing from the previous slider result. Every edit continues from unchanged original dependencies. Export stays non-cancellable with editing disabled. Scopes and navigator remain secondary to main-image feedback.

## Measurements and limits

Blue-sign full-quality native Detail feedback is approximately **125–143 ms** at the accepted complete-boundary checkpoint. Completed cached processing plus image preparation was **119–131 ms**, versus **747–774 ms** fresh. Six dispatches remain, including display texture preparation. Warm native busy frame gaps: median **16.6 ms**, p95 **45.8 ms**, maximum **46.7 ms**. Background execution does not prevent contention on the shared eframe/wgpu device and queue.

Detail inputs retain **783.6 MiB** without sharpening or **845 MiB** with sharpening. Settled total physical footprint was approximately **2.1 GiB**, with a **3.5 GiB** transient peak. Approximately 2 GB total normal editing memory is a soft target, not a cache/RSS limit. GPU storage allocation totals are not residency; RSS excludes substantial GPU accounting.

Worker drag tests settle Detail/Exposure around **163/96 ms**. Earlier noise/override edits still need **286–955 ms**; brightness/layer mixing roughly **150–171 ms**. These checks establish progress/settings/scopes correctness, not native interaction acceptance.

Navigator preparation: synthetic average **1.62 ms**, native warm commonly **1.7–3.3 ms**, with cold/contention outliers. Synthetic completed batched drawing measured **0.169 → 0.122 ms** for repeated float presentation versus ordinary prepared-tile painting. This is a modest standalone-pass result, not proof that native frame stalls are removed.

## Resumed measurement preparation

- Isolated worktree: `/Users/work/git/foveonic-editing`, branch `perf/editing-feedback`, starting at `d686266`; Cargo artifacts: `/tmp/foveonic-editing-target`. Shared source and concurrent prototypes remain untouched.
- Baseline release binary and native app bundle rebuilt in those artifacts. Ordinary release workspace tests pass, including the expected 52 core and 61 GUI tests. Strict release all-target Clippy, release example builds, formatting, documentation-index generation/checks/tests, bundle/artwork Python tests and five local harness tests also pass. Logs and native captures live in the new worktree's `.local/editing-feedback/`. No new core GPU/fixture equality suite has been run; the production source is unchanged.
- A local, non-default Python harness (`.local/editing-feedback/native_pointer.py`) records existing stderr timing with monotonic timestamps and injects real macOS pointer events at explicit screen coordinates. Its five CPU-only tests pass; event-posting permission is available. The unchanged bundled app launches with an isolated `HOME`, without smoke settings: eframe 0.31.1 resolves macOS persistence through `HOME/Library/Application Support/foveonic`. Native launch confirms that isolated preference path; the owner's existing app and preferences remain untouched.
- Existing Detail smoke captures advance settings programmatically; they are not pointer-driven interaction evidence. Existing `UI-IMAGE` logs measure worker-queue-to-UI receipt, not native-event-to-completed-screen presentation. External pipe timestamps select interaction windows but cannot establish precise per-event latency or revision pairing. Retain those distinctions in all reports.
- `FOVEONIC_UI_TIMING` additionally requests a repaint after 16 ms while busy. Keep that diagnostic condition matched for baseline comparisons; its frame-gap results are not an uninstrumented acceptance measurement. Intentional idle gaps must not be reported as busy stalls. Frame gaps alone cannot identify GPU drawing as the cause.
- Notify the owner immediately before each GPU/native run so they can pause other agents. Do not add production scheduling or presentation changes until measurement supports a small proposal and shared-file work is coordinated.

### Native pointer baseline

The first run (`native-baseline-1`) overlapped another agent's ignored GPU suite in `/tmp/foveonic-cleanup-review-1`. Its `INVALID-FOR-TIMING.json` records that exclusion; all captures remain intact. It is not timing acceptance evidence. The repeat (`native-baseline-2`) began after that suite finished, with a half-second process sampler recording no overlapping test/build/benchmark workloads during the measured drags. The owner paused video playback and interaction for the repeat. The owner's separate app remained running; only recorder-owned apps were closed.

Conditions: unchanged `d686266` release bundle, shared native eframe/wgpu device, blue-sign original, recorded Sunlight/Standard route, full-quality default noise/detail, no sharpening, Fit at 38% physical-pixel zoom, 1440×1000-point content window on a high-density display. Navigator stayed open; Scopes was collapsed but still computed after settlement. Native mouse drags used 61 positions over two seconds, repeated three times per control. Source opening, reconstruction enabling and two warm-up drags per control were outside measurement windows. Detail ranged approximately 0.15–1.85 and Exposure approximately −0.85–+0.85. Screenshots and memory sampling ran after the timed interaction, not during it.

| Measurement (ms) | Detail median / p95 / max | Exposure median / p95 / max |
| --- | --- | --- |
| Completed worker image (processing + texture preparation) | 150.0 / 154.5 / 194.8 | 82.4 / 100.1 / 101.6 |
| Worker queue to UI image receipt | 169.2 / 245.4 / 278.2 | 114.5 / 155.2 / 200.7 |
| Busy UI frame gap | 18.1 / 54.7 / 56.1 | 17.0 / 43.1 / 45.9 |
| UI update handling | 0.256 / 0.307 / 5.256 | 0.258 / 0.310 / 5.402 |

Detail published 12–13 images during each drag; Exposure published 23. All six drags delivered a final `current=true` image after release. External timestamp estimates for release-to-final-UI-receipt were 178–287 ms for Detail and 151–202 ms for Exposure (`drag-settlement.json`); these include pipe timing uncertainty and are not completed screen-presentation measurements. Three scope replies per control followed settlement, with no scope publication during the drags. Detail retained exactly 821657600 bytes (783.6 MiB); Exposure retained 193019904 bytes. A separate settled sample after restoring the Detail boundary reported 1.9 GiB physical footprint and a 3.6 GiB process-lifetime peak (`memory-detail-settled.log`). These are unchanged-checkpoint observations, not claimed memory improvements or a matched before/after comparison.

`provenance.json` retains the frozen checkpoint, binary/source hashes and OS version; production source has no diff. The repeat establishes a real pointer-driven queue-to-UI baseline, not full input-to-present acceptance. Historical programmatic smoke values of 125–143 ms are not directly comparable to continuously coalesced native drags. UI update handling is small; the worker still accounts for substantial elapsed time, and final settlement can wait behind the active render. Existing logs cannot separate queue waiting from reply-delivery delay, split processing from full-resolution display preparation, or attribute frame gaps to drawing/submission/surface waits. Buffer/uniform/binding allocation is a candidate, not an established bottleneck.

### Next measurement slice

- [x] Rebuild the frozen checkpoint in isolated artifacts and check ordinary tests.
- [x] Capture native pointer-driven Detail/Exposure progress, settlement and busy frame gaps; exclude contended evidence.
- [x] Sample settled Detail memory separately and preserve source/settings conditions.
- [x] Add owner-approved timing hooks on the isolated branch: worker queue/start/publication, completed processing versus display preparation, and main callback preparation/encoding. Most probe code is in the separate `crates/foveonic-gui/src/timing.rs` module; hooks in `main.rs`, `worker.rs` and `viewport.rs` add no shader, arithmetic, storage or scheduling changes. Shared inspection files remain untouched.
- [ ] Distinguish UI receipt, drawing submission and GPU-completed drawing explicitly; queue completion still does not establish screen scanout. Keep synchronized diagnostic runs separate from completed unprofiled wall measurements.
- [ ] Use that evidence to propose the smallest optimization for owner review, then run matched wall, native, memory and exact-output checks if approved.

## Guarded baseline refresh and probes

The owner requested checking the shared worktree's ownership guards and approved refreshing the baseline before continuing. Imported the cleaned guarded processor, its migrated callers/tests/benchmark readback, and ownership guidance into `/Users/work/git/foveonic-editing`. Production shaders are unchanged. `tone.rs` remains byte-identical to `d686266`; interactive rendering still regenerates tone preparation. Concurrent inspection GUI/comparison work is not imported. Core dispatches retain explicit read/write bindings, synchronous submission, exclusive writes and ownership-checked scratch recycling. Existing interactive operation-boundary waits remain; no new completion fences are introduced.

Guarded, unprobed release workspace: 52 ordinary core tests, 52 ignored core tests, 61 ordinary GUI tests and five ignored GUI tests. Ordinary tests pass; rebuilt the guarded GUI and preserved its unprobed binary separately. This is a new baseline, not an optimization benefit.

Opt-in probes use the existing `FOVEONIC_UI_TIMING` flag and one process-wide monotonic clock. Worker observations include request queue wait, processing, host texture preparation, the existing texture-completion wait and reply publication. Accepted UI replies and main-image callbacks carry revision IDs; draw IDs pair callback preparation/encoding. The preparation-to-encoding gap brackets eframe's surface acquisition plus other egui work: it is not a pure surface timer. eframe's previous-frame CPU estimate excludes its measured surface-acquire/submission/present waits. Callback encoding does not establish draw submission, GPU completion or screen scanout. No CPU image readback, timestamp-query synchronization, added polling or new GUI diagnostics are used.

Probed ordinary release workspace and strict all-target Clippy pass: 52 core tests and 65 GUI tests, including four new CPU-only probe tests. Release examples, formatting, documentation generation/checks/tests, bundle/artwork Python tests and ten local harness tests also pass.

### Guarded validation and native findings

`guarded-refresh/gpu.log` passes all 52 ignored core and five ignored GUI GPU/fixture tests, including guard rejection/access checks, retained-image behavior, exact six-original floats against fresh results and retained goldens, and cold/warm resized TIFF equality. New evidence is in `.local/editing-feedback/guarded-equality-1`. A different agent's suite was finishing when the correctness run began; no timing from these correctness suites is acceptance evidence. Passing does not establish the mismatch's cause or a guard-based fix.

Separate captures retain the same native conditions described above: `native-guarded-baseline-1` uses the preserved guarded/unprobed bundle; `native-guarded-probes-1` uses the guarded/probed bundle. Both have no observed overlapping test/build/benchmark executables during native measurement. The original sampler flagged 27 probe-exposure samples because its own shell wrapper *mentioned* a later `vmmap` command; that command ran only after the sampler stopped and timed interaction ended. `workload-review.json` retains the classification and original raw samples/source. The corrected sampler now examines executing programs rather than shell text, with three CPU regressions. No other process was stopped; the owner's app remains running.

| Guarded native measurement (ms) | Unprobed Detail median / p95 | Unprobed Exposure median / p95 |
| --- | --- | --- |
| Completed worker image | 150.0 / 153.0 | 95.4 / 101.5 |
| Worker queue to UI receipt | 170.6 / 275.9 | 125.4 / 170.9 |
| Busy frame gap | 18.1 / 55.8 | 16.9 / 44.3 |

Unprobed Detail published 13 images during every drag; Exposure published 21. All six drags settled to `current=true` after release. External release-to-final-receipt estimates were 281–288 ms for Detail and 139–211 ms for Exposure. Probed Detail/Exposure worker medians were 150.2/83.7 ms and queue-to-UI medians 169.7/126.9 ms. Detail agrees closely between these captures, while Exposure varies; sequential single captures do not establish instrumentation overhead or an optimization benefit.

| Diagnostic median (ms) | Detail | Exposure |
| --- | --- | --- |
| Processing, including existing interactive completion waits | 130.6 | 56.6 |
| Host display-texture preparation/submission | 0.230 | 0.265 |
| Existing display completion wait on shared device | 19.0 | 25.3 |
| Latest-pending worker queue wait | 19.3 | 27.6 |
| Ready reply to accepted UI receipt | 0.246 | 0.374 |
| Main callback resource preparation | 0.029 | 0.028 |
| Main callback draw encoding | 0.001 | 0.001 |
| Accepted UI receipt to first draw-encoding end | 0.661 | 0.636 |

Main callback preparation-to-encoding gaps had p95 about 16.2/16.1 ms and maxima 21.5/22.3 ms for Detail/Exposure. eframe's previous-frame CPU estimates were about 0.79 ms median and below 0.98 ms p95, excluding its measured acquire/submission/present waits. A separate sample after restoring the probed Detail boundary reported 2.0 GiB settled physical footprint and 3.8 GiB process-lifetime peak. This is not a matched memory-improvement claim. All source/binary hashes and conditions are retained in provenance files.

These findings deprioritize per-frame uniform/binding reuse: its measured host cost is only about 0.03 ms. Worker publication and UI receipt are also small. Processing dominates Detail and the full-resolution view-completion wait is material for both controls, but that wait includes earlier shared-queue work, view allocation/initialization/compute and potentially later submissions selected by `Maintain::Wait`. It cannot yet be attributed to the view kernel alone. Draw-encoding end still does not establish submission, GPU completion or scanout.

Next recommendation was a separate native GPU timestamp pass before selecting an optimization. The owner approved continuing that measurement slice.

### Native GPU timestamp mode

Implemented `FOVEONIC_UI_GPU_PROFILE` in GUI-owned probe/setup code. It requests only the adapter's supported `TIMESTAMP_QUERY` feature and invokes the processor's existing synchronized profiler. Ordinary launches and host-only timing request no extra GPU features. Explicitly requested GPU capture fails at startup if timestamps are unavailable, rather than silently returning host-only data. No processor, shader, storage guard or photographic arithmetic changes were made for this mode.

GPU stage records identify revision, processing/view phase, report-entry index and complete kernel label. Nested profiles need not append entries in chronological dispatch order; indices only disambiguate repeated labels. Historical source-load records in the engine's host profile are deliberately not printed or charged to warm edits. Timestamp resolutions read back only small timing data and add synchronization: profile-mode host spans/latency are diagnostic, not unprofiled acceptance evidence. Main callback encoding still does not establish screen presentation.

CPU validation passes: 52 ordinary core tests and 68 GUI tests (three new tests cover optional feature selection, unsupported capture and GPU log formatting), strict all-target Clippy and 13 local harness/parser tests. The recorder requires explicit `--gpu-profile`, clears inherited profiling flags for baseline captures, and records the mode in its manifest. The previous host-only probed binary is preserved in `.local/editing-feedback/gpu-timestamps/`. Native GPU capture completed in `.local/editing-feedback/native-gpu-timestamps-1`; notify the owner before further GPU or mouse use.

### GPU diagnostic findings and proposed experiment

The profiling-mode continuous worker-drag regression passed with newest settings and matching scopes (`gpu-timestamps/worker-gpu-profile.log`). Native timestamp support was confirmed at startup, with no other sampled test/build/benchmark workload during capture. All six native drags settled to a current image; source/binary/GUI hashes, raw records, parser summaries and screenshots are retained. Only the test app was closed.

| Diagnostic GPU stage median (ms) | Detail | Exposure |
| --- | --- | --- |
| Residual detail mixing | 6.81 | skipped |
| Detail reconstruction | 4.74 | skipped |
| Processing-tile projection | 21.90 | skipped |
| Tone | 4.31 | 10.73 |
| View texture | 6.13 | 8.27 |
| Sum of processing GPU samples per delivered revision | 37.94 | 10.73 |

These no-sharpening recipes ran four processing passes plus view for Detail, and tone plus view for Exposure. Projection is the largest measured Detail shader, but shader execution alone does not explain the processing wall span. In this same synchronized run, processing host spans were 123.8/54.2 ms median for Detail/Exposure. Texture preparation now included timestamp resolution (18.0/19.4 ms median), followed by the existing global completion wait (about 2.55 ms). These profile-mode figures cannot replace the unprofiled 130.6/56.6 ms processing and 19.0/25.3 ms view-wait observations. Implicit initialization, allocations/uploads, CPU recipe preparation, queue delays and profiler synchronization remain outside the shader timestamps; subtraction is not a pure queue-wait measurement. Frame cadence also changed under profiling, so no speedup is claimed.

Next proposal for owner review: replace only the GUI's full-resolution view-completion `Maintain::Wait` with `WaitForSubmissionIndex` for that view's actual submission. Retain its opaque completion token with `ImageTexture`; processing guards remain live through the same synchronous submission and the worker still waits for required image preparation before publishing. Existing graph-operation completion/cancellation boundaries stay unchanged. No shader/arithmetic changes, additional image buffers, altered scope/navigator priority or device-idle assumption are needed. This is a narrowly scoped test of whether unrelated later GUI submissions extend publication, not a claim that queue waiting explains the entire gap. Run matched unprofiled native A/B and exact-output checks; discard it if the gain is not repeatable. The owner approved testing this prototype. It is now implemented only in the isolated worktree: GPU submission helpers return their actual token, `ImageTexture::ready_submission()` carries it, and the GUI worker uses that token at its existing view-completion boundary. No graph-operation waits or guards changed. Before/after native bundles are preserved in `.local/editing-feedback/targeted-view-wait/`. Ordinary workspace tests (52 core, 68 GUI) and strict all-target Clippy pass. A new ignored GPU regression checks the view-completion callback with later work queued, unchanged signed/headroom texture samples, retained source samples and absence of extra submissions. The trial completed and was discarded, as described below. No performance optimization from this trial remains in the source.

### Image-specific completion-wait trial: rejected

Serial correctness validation passed all 53 core GPU tests (52 existing plus the new token/callback test) and five GUI GPU tests. A tool timeout stopped the first core invocation during an unfinished test; 31 completed passes remain in `core-gpu.log`, and the 22 remaining tests passed in `core-gpu-rest.log`. The recorded resume command and unique test-name count establish coverage without treating the interrupted invocation as a complete pass. No assertions were weakened. Correctness timings are not performance acceptance data.

Four native captures used identical full-quality source/settings/window geometry and the order before/after/after/before. Timestamp profiling was disabled and absent from all logs; host probes were identical in both binaries. All 24 drags settled current. No sampled test/build/benchmark workload overlapped timed interactions or their settlement windows. The only flagged processes were recorder-owned `vmmap` calls after the timed drags; raw flags and `workload-review.json` are preserved.

| Median (ms) | Before 1 | After 2 | After 3 | Before 4 |
| --- | --- | --- | --- | --- |
| Detail worker | 115.66 | 115.28 | 117.90 | 117.59 |
| Detail view wait | 16.42 | 16.42 | 16.42 | 16.46 |
| Exposure worker | 71.18 | 72.47 | 72.41 | 71.79 |
| Exposure view wait | 17.68 | 18.87 | 18.88 | 17.68 |

Detail changed by −0.38 ms in the first pair and +0.31 ms in the repeat: no repeatable gain. Exposure was 0.62–1.30 ms slower. Callback-gap p95 was essentially unchanged (Detail about 35–37 ms; Exposure about 29–30 ms). Queue-to-UI tail movement was not a repeated meaningful improvement. The display-only indexed wait therefore did not demonstrate worthwhile benefit in this workload; it does not rule out costs at graph-operation waits or implicit resource initialization.

Separate settled memory observations were the same coarse `vmmap` readings for the final before and after apps: 1.4 GiB footprint and 3.4 GiB process-lifetime peak. This is not an improvement claim against earlier captures. Earlier sessions' approximately 150/95 ms worker medians also are not a matched comparison to this trial's approximately 116/72 ms medians; no cross-session speedup is claimed.

The prototype and its completion API/test were removed. Hash checks confirm every core and GUI Rust source is byte-identical to the pre-prototype guarded/probed baseline. Before/after bundles, a standalone prototype patch and source snapshot, hashes, raw pointer/probe logs, screenshots, settlement records, memory samples and `comparison.json` remain in `.local/editing-feedback/targeted-view-wait/`. The shared worktree was untouched. Continue diagnosis of costs outside the measured shader passes before proposing another implementation; do not replace graph-operation waits or add persistent image caches without owner review.

## Maintained diagnostic design

The experimental flags, text formats and local measurement scripts described above are historical evidence. The owner approved consolidating the useful capabilities: see [profiling consolidation](./profiling-consolidation.md), [requirements](../docs/specs/profiling.md) and [current usage](../docs/profiling.md). Live modes are off/wall/GPU, with shared JSON reports and maintained scripts under `scripts/profiling/`. The former forced busy repaint is removed; source loading is reported separately, and current CPU preparation timings accompany GPU diagnostics. No speedup from the rejected image-specific wait experiment is retained.

## Mismatch boundary

Three separate runs failed on the red-flower fixture: after a Shadows edit, after a Detail edit, and after cancellation/resume. CPU tone-table equality passed in the second run. The shadow failure differed in 34382750 RGB components (71.25% of full-sensor components), maximum absolute float difference **1.177125**. The resume failure differed in 11213165 components (23.24%), maximum **0.0087557435**, initially near sensor coordinate (1229,2444). These are processing floats, not display/TIFF code differences.

Later focused repeats and whole suites passed. There is no reliable on-demand reproduction or established cause, and neither concurrent work nor tone reuse has been conclusively ruled in/out. The diagnosing agent will test frozen versions with and without the uncommitted prototype. Do not mask the failure with weaker tolerances or a timing delay.

## Evidence and validation

Fixture originals: `/Users/work/git/foveonic/references/x3f`. Retained float goldens: `.local/pipeline-performance/baseline/goldens`. Use a new evidence directory; never overwrite goldens or delete failed-run evidence.

Reviewed stable checkpoint: **52 ordinary core tests**, **49 ignored core GPU/fixture tests**, **61 ordinary GUI tests**, **5 ignored GUI GPU/worker tests**, plus CLI/docs-index and Python tests. All passed in explicitly rebuilt isolated snapshots. Logs: `/tmp/foveonic-followup-*-isolated.log`, `/tmp/foveonic-followup-stable-isolated-gpu.log`; exact-output evidence: `.local/ui-responsiveness/stable-checkpoint-equality-2`.

The shared checkout retains `/tmp/foveonic-ui-{followup-gpu,followup-gpu-2,flower-isolation-5}.log`, plus later diagnostic logs and all native/equality captures. Earlier artifact-reused review checks are not authoritative for later snapshots.

Run the repository's release workspace, strict all-target Clippy, example, formatting, documentation-index and Python checks. Run fixture/GPU checks serially when GPU use is coordinated. Report scope, completed wall measurements, output equivalence, memory changes, native interaction limits and any unresolved failures before requesting acceptance.
