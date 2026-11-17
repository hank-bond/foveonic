---
summary: "GPU probes and real-worker experiments separate rendering width, completion waits, latency, memory and task attribution."
kind: explanation
---

# Wide-rendering experiments

## Scope

The owner approved the width-one extraction and isolated overnight experiments starting from `fa4a09c`. The functional branch is `design/wide-rendering`. The opt-in engine probe is on `experiment/wide-rendering-current`, behind the `wide-probe` feature. Default application execution remains serial. The later real-worker experiment below is explicitly enabled with `wide-experiment`. No commits or publication are part of this investigation.

The [integration history](../plans/archived/wide-rendering-integration.md) records the application experiments. The [architecture map](./interactive-rendering-architecture.md) describes current execution boundaries and the [orchestration specification](./specs/render-orchestration.md) records admission requirements and limits. The [earlier probe](../plans/archived/wide-rendering.md) owns its historical results. Current requirements remain in [processing graphs](./specs/processing-graphs.md), [render orchestration](./specs/render-orchestration.md) and [performance diagnostics](./specs/performance-diagnostics.md).

The method and findings through native application context describe the isolated investigation. The real-worker section has its own later method, measurements and limits.

## Questions

- Does traversing several photographs help when existing completion waits remain?
- How much benefit comes from grouping completion waits across photographs?
- Can batching several operations within one photograph achieve similar gains with simpler ownership?
- How does the polling interval affect waiting and CPU use?
- What happens to the first useful image, replacement latency and memory as work becomes wider or larger?

Width means admitted photo executions. Depth means required operations advanced for one photo before completing a window. The GPU decides physical overlap. These controls do not establish simultaneous shader execution.

## Method

Measurements use an Apple M1 Pro with 16 GiB RAM. The fixed Merrill set and its hashes are in [wide_inputs.json](../scripts/profiling/wide_inputs.json). The heterogeneous set is in [wide_mixed_inputs.json](../scripts/profiling/wide_mixed_inputs.json). Owner photographs resolve through the example release. External originals retain their publisher and license metadata in the [external manifest](../scripts/profiling/external_raw_samples.json).

Each process opens and seals independently owned reduced sessions before timing. Warm cases also complete an initial downstream graph and retain its display textures. Exposure adds 0.25 EV with automatic exposure disabled. The reconstruction case changes chroma from zero to one. Camera defaults supply the remaining settings. Numeric scale, processing policy and complete transient snapshots accompany every result.

The timed interval includes changed-recipe preparation, graph execution, per-photo cache settlement and completed display textures. It excludes source opening, dependency sealing, verification readback and file writing. Cold-downstream cases skip the initial downstream render. They still exclude opening and sealing, so they do not measure cold selection from disk. No UI input, coordinator delivery or physical screen presentation is measured.

[wide.py](../scripts/profiling/wide.py) rotates mode order, excludes one warmup per mode, and gathers seven measured trials. Each output must match independent serial-control samples, geometry and complete settings. Concurrent compiler/application/probe work is sampled. Contested observations remain separate from clean distributions. Small sample counts and a single machine limit generalization.

[wide_replay.py](../scripts/profiling/wide_replay.py) additionally checks probe pixels against their recorded hashes and replays snapshots through a separately frozen ordinary renderer. It uses the maintained complete-buffer and geometry comparator. This guards against a mistake shared by all probe modes after recipe/artifact preparation was split.

Logical GPU boundary diagnostics run separately from latency trials. Their counts deduplicate sessions, graph values, scratch, completed images and textures. They exclude within-operation temporaries, decoded CPU data, staging, driver and allocator overhead. Every timing process also has physical-footprint sampling and a guard at 100 ms intervals across setup, rendering and verification. Sampled peaks can miss shorter transients. The 4 GiB child guard is a sampled test-process guard, not application memory admission.

## Implementation and correctness

The functional extraction keeps one numerical dispatcher, existing waits, cancellation checks, progress, scratch retirement and caller-owned cache commit/finalization. All 144 maintained cases match the frozen pre-extraction serial build exactly, including complete samples and storage/view geometry. These cover 13 distinct originals, four numeric resolutions, both processing policies and additional dense-guide/edited recipes. Four focused cursor GPU tests pass.

The experimental window owns background completion through the existing queue-prefix callback. Progress and completed-scratch trimming wait for that window. Independent photos commit and settle separately. Five experimental cursor GPU tests pass, including grouped progress. The prototype retains one CPU orchestrator and individually owned sessions. It has no application working-set admission policy.

The first experiment version had a redundant wait when an interleaved photo finished. It is preserved as preliminary evidence. Version two removes that wait and adds cold-downstream, depth, scale and polling controls. Version three adds an explicit choice between flushing a finished photograph promptly and deferring it through the bounded round. Matched comparisons use one frozen binary per experiment.

## Findings

Completion waiting is a substantial part of these workloads. Advancing photographs round-robin while retaining the existing waits gives little throughput benefit. Grouping completion waits helps. Grouping several operations inside one photograph also helps, with simpler ownership and earlier first feedback in the measured reconstruction edits. These results support investigating completion policy before introducing a worker pool or enabling wider application scheduling.

There are 1,044 measured engine observations across the primary, matched supplement and combined polling/depth runs. Workload monitoring classifies 1,026 as uncontended. Tables below use uncontended medians from seven measured trials per cell. One-photo exposure cells with only two or three clean trials and preliminary smoke cells are excluded from strong comparisons. These sample sizes describe this machine and workload. They do not establish a population tail-latency bound.

### Width and depth

The following matched experiment uses eight warm Merrill photographs, Quality processing, scale divisor eight, a 1,000 µs completion polling interval and prompt completion of finished photographs. Times are milliseconds. The reconstruction edit changes chroma from zero to one.

| Schedule | Group complete | First complete |
| --- | ---: | ---: |
| Ordinary serial | 102.12 | 12.09 |
| Grouped width one, depth one | 103.58 | 12.05 |
| Grouped width one, depth two | 73.70 | 9.00 |
| Grouped width one, depth four | 60.34 | 7.51 |
| Grouped width two, depth one | 69.00 | 13.09 |
| Grouped width four, depth one | 53.57 | 16.87 |

Width four reduces group time about 48%, while increasing first-result time about 40%. Depth four at width one reduces group time about 41% and improves first-result time about 38%. The width-one/depth-one control stays close to ordinary serial execution, so the grouped bookkeeping alone does not explain the gains.

A separate matched round-robin control keeps operation waits: eight-photo reconstruction takes 100.69 ms serially and 100.88 ms interleaved at width two. First completion moves from 12.08 to 20.82 ms. Traversal order alone is insufficient here.

Warm exposure takes about 32 ms across serial and prompt-finish width-two/four schedules. This is a completion-policy limitation: each photo has one required graph operation, and prompt photo completion closes the window. Those nominally wide runs actually reach only one operation and one submission per window. This result cannot establish GPU saturation or disprove useful exposure batching.

Deferring finished photographs through a bounded round changes that behavior. In a separately matched experiment:

| Edit and schedule | Group complete | First complete |
| --- | ---: | ---: |
| Exposure serial | 31.69 | 3.99 |
| Exposure width two | 25.82 | 4.85 |
| Exposure width four | 22.41 | 6.40 |
| Reconstruction serial | 99.05 | 12.01 |
| Reconstruction width two | 62.53 | 13.22 |
| Reconstruction width four | 44.54 | 17.31 |

Delaying a finished image can improve group completion while worsening first feedback. A production policy needs an explicit Active-photo latency gate and useful-image publication policy.

### Completion polling

The existing completion loop polls at 1,000 µs. A matched experiment changes that interval while preserving serial application ownership and engine work:

| Warm eight-photo edit | Serial at 1,000 µs | Serial at 100 µs | Serial at zero delay |
| --- | ---: | ---: | ---: |
| Exposure group complete | 31.61 | 15.24 | 15.24 |
| Reconstruction group complete | 101.47 | 41.44 | 45.99 |
| Reconstruction first complete | 12.05 | 5.35 | 5.57 |

Zero delay busy-polls and offers no consistent improvement over 100 µs. Whole-process CPU time rises from roughly 4.94–4.95 seconds to 5.49–5.54 seconds. That includes setup, warmup and numerical readback. It cannot quantify CPU cost inside the short measured edit interval. The experiment supports evaluating a more efficient completion wakeup or shorter bounded polling. It does not yet select an application default.

At 100 µs, a fresh matched reconstruction comparison still favors batching:

| Schedule | Group complete | First complete |
| --- | ---: | ---: |
| Serial | 41.66 | 5.31 |
| Width one, depth four | 33.13 | 4.33 |
| Width two, depth one | 33.86 | 7.47 |
| Width four, depth one | 28.91 | 11.09 |

After reducing polling overhead, width four gains about 31% group time but more than doubles first-result latency. Depth four gains about 20% group time while improving first feedback. Polling and batching overlap in the waiting overhead they remove. Their individual percentage gains should not be added.

### Larger, cold and heterogeneous work

Benefits vary with recipe, scale and cache state. At scale divisor four, eight-photo reconstruction changes from 121.96 ms serially to 98.23 ms at width two and 81.06 ms at width four. First completion changes from 13.65 to 19.44 and 29.76 ms. At scale divisor two with two photos, reconstruction changes from 65.28 to 54.52 ms at effective width two, while first completion changes from 31.67 to 46.66 ms. Exposure at that larger grid changes from 14.24 to 15.27 ms and has no demonstrated benefit under prompt completion.

Cold-downstream exposure at scale eight is substantially heavier than the warm exposure case: 348.49 ms serially, 238.58 ms at width one/depth four and 228.04 ms at width four/depth one. First completion is respectively 40.75, 27.26 and 103.30 ms. This reinforces the cost of giving peers service before the Active image. Opening, decoding and dependency sealing remain outside these timers.

Eight heterogeneous cameras show no warm-exposure group gain with prompt completion: 55.52 ms serially, 56.24 ms at width two and 55.93 ms at width four. First completion grows from 3.95 to 4.80 and 22.71 ms. Recipe preparation is inside the interval, but these aggregate measurements cannot attribute the delay to a particular CPU operation.

### Existing CPU workers

The existing `profile_editing` executable provides a separate contextual control using native sessions, one executor per photo and a shared ordered GPU queue. Two processes per configuration each perform seven edits with one, two or four CPU workers. All 96 final-photo checks match fresh ordinary rendering exactly. All runs are uncontended.

For eight-photo exposure, group medians are 33.06–33.79 ms with one worker, 18.28–18.36 ms with two and 16.30–16.53 ms with four. First completion stays around 4.1–4.4 ms. Independent host execution can overlap waits in this workload. This is not a matched comparison with the reduced-session window probe and does not prove CPU preparation is the bottleneck. The reconstruction control alternates two settings and reuses their cached intermediates after its first two rounds, so its all-round median is unsuitable for comparison with the probe's fresh zero-to-one edit. A worker pool remains a candidate requiring a controlled comparison with equivalent retention and recipes.

### Memory and interruption

Eight separate memory cells complete under the sampled 4 GiB process guard. All eight sessions are retained throughout. Observed logical boundary peaks are 193.15 MiB for exposure and 214.25 MiB for reconstruction across schedules. Sampled whole-process peaks span approximately 1,010–1,197 MiB. Equal logical peaks do not establish free overlap: within-operation temporaries and physical residency are not bounded by that inventory.

The 64 MiB submission-window threshold measures allocation traffic after an operation. One larger reconstruction operation reaches about 102.8 MiB. A cold width-four window includes as many as 92 submissions across four operations, including helpers and required readbacks. These are window activity counts, not necessarily simultaneously outstanding submissions. Neither width nor the post-operation traffic threshold implements memory-budget admission or a hard interruption bound.

The original warm interruption test correctly failed its coverage requirement because peers had already finished their graphs. That failure remains recorded. Followups separate three cases: 36 warm recovery injections, 36 cold-downstream injections with one to three advanced unfinished peers, and 12 deferred-finish injections with graph-complete peers waiting for a fence. All 84 followup injections pass, covering cancellation, injected failure and replacement. They verify 588 unchanged-peer outputs and 28 replacements exactly. Cancelled or failed obsolete targets publish no image.

Cold grouped hook-to-drain observations are roughly 1.5–3.2 ms. Replacement completion is roughly 32–43 ms. The hook runs after an advancement returns. It omits request arrival in the middle of a long operation, and interleaved mode has already waited before reaching it. These values do not establish actual GUI interruption latency or a fair input-time comparison between modes. Replacement is serviced after draining the window, but already completed peers may settle first.

### Native application context

Native scenarios run the ordinary serial application in background windows with experimental polling intervals. They issue declared application commands, with no physical pointer injection. Scenario endpoints are sampled at 20 ms and do not measure physical screen presentation.

Three paired two-photo warm-edit runs remain near 41.6 ms at both intervals. Three paired eight-photo runs with the smoke readiness guard have median-of-run medians of 124.93 ms at 1,000 µs and 108.15 ms at 100 µs, about 13% lower. A separate final cohort with the experimental opening-state fix measures 124.51 and 108.33 ms, also about 13% lower. Both use three processes per interval and five edits per process. These cohorts are kept separate. The improvement is much smaller than the headless exposure improvement. Delivery, presentation, scheduling and endpoint granularity remain relevant.

The original application selection journeys fail at both polling intervals. The smoke assertion fires after desired images are accepted, textures are encoded and worker plans are idle. Replacing that assertion with a smoke-only wait preserves the timeout and reveals progress that can remain unsettled. Code inspection identifies a stale `Opening` path when selecting an already-requested refinement: duplicate publication is suppressed, but opening activity is retired only for new publication. Retained-pixel sidecar reuse needs the same retirement boundary. Earlier captures did not directly expose the opening flag, so that attribution combines code inspection and the observed stalled progress.

An independently reviewed experimental fix retires opening activity when an active ready refinement is handed off or reused, and when an admitted retained image already satisfies selection. It keeps request revisions and new-request timing unchanged. Fourteen targeted runtime/progress CPU tests, strict feature GUI Clippy and formatting pass. The final native cohort passes all 12 scenarios: eight warm-edit processes, two selection journeys and two background-edit checks. Both polling intervals pass the journeys and background checks, including eight captured-image comparisons with exact complete samples and geometry against fresh rendering. Background overlap and export-decode assertions also pass. This fix remains in the experimental worktree, separate from the functional cursor slice.

Additional initial-setup failures occurred with little free disk space. New diagnostics show `PrimingDisk` waiting for optional cache entries while opening is absent, progress has finished and rendering is idle. This is distinct from the selection progress failure. The diagnostic namespaces created by these runs had accumulated about 19 GiB of disposable decoded cache. Only namespaces bound to the investigation's recorded launch manifests were removed, after owned applications exited. The final cohort uses a disposable `TMPDIR` per scenario, preserving ordinary cache policy and clearing it after the child exits. It passes with adequate space. Failed runs remain recorded and are excluded from the successful native cohort. Polling-default selection still needs broader input and CPU-cost evidence.

## Real-worker experiment

The owner clarified that this unit is disposable experimentation: establish the speed benefit first, then choose memory efficiency and simpler ownership. The functional worktree now has an opt-in worker path using the same numerical dispatcher and existing coordinator/delivery machinery. The initial worker experiment used `FOVEONIC_WIDE_RENDER_WIDTH=1|2|4|8` for live warm reduced jobs and `serial` for the ordinary control. Its unsupported work used the serial path after live experimental work retired. The subsequent native-region slice extends that same lifecycle to retained Merrill zoom patches, starting with width-two validation. Missing/cold inputs and unsupported tool work retain the serial fallback. This is not production memory admission.

The maintained ignored `real_worker_warm_edit_experiment` test uses the eight pinned Merrill inputs. Each trial clears processing history, fully warms the baseline outside timing, then atomically publishes the same chroma change from zero to one. It measures coordinator-classified image receipt, including completed view textures. It does not measure physical window presentation. One warmup precedes seven measured trials. Complete samples, settings and storage/view geometry are checked against fresh ordinary rendering after timing. Actual submitted width, source acquisitions, dispatches and readbacks are recorded.

The following measurements use one frozen optimized V3 test executable. Another Foveonic application remained open throughout, so these are exploratory distributions rather than uncontended performance gates. Repeated cohorts confirm the larger throughput gains, but small differences at width one should not be interpreted as an established optimization.

| Eight photos, scale 8, Quality | First Active median | Complete-set median |
| --- | ---: | ---: |
| Ordinary serial | 15.64 ms | 133.80 ms |
| Experimental width 1 | 15.58 ms | 130.28 ms |
| Width 2 | 18.42 ms | 90.87 ms |
| Width 4 | 19.95 ms | 63.11 ms |
| Width 8 | 29.56 ms | 56.89 ms |

Width four saves about 71 ms across the set at a roughly 4 ms first-image cost in this cohort. Width eight saves another 6 ms across the set while delaying first feedback by another 10 ms. More RAM is not the only consideration when choosing width: throughput and first feedback differ.

The owner also requested two images at half width and half height, numeric scale 2. A reversed-order repeat gives:

| Two photos, scale 2, Quality | First Active median | Complete-set median |
| --- | ---: | ---: |
| Ordinary serial | 33.95 ms | 68.66 ms |
| Experimental width 1 | 34.21 ms | 69.89 ms |
| Width 2 | 48.24 ms | 53.06 ms |

That is a smaller, approximately 23% group-time improvement with a roughly 14 ms first-image delay. Earlier trials had substantial outliers while the other application was open; retain them separately rather than presenting the cleaner repeat as the entire distribution. Every cohort remains exact, with zero timed source acquisitions and no timed photographic readbacks or profiling queries.

### Memory observations

Separate runs mark baseline and edit phases and sample OS-accounted process footprint at approximately 5–6 ms intervals. These runs are not latency evidence. The process includes the verifier and its retained expected samples, so absolute footprints are not minimum product RAM requirements. Sampling can miss shorter transients, and allocator/device caches can change the baseline between processes.

The original whole-scenario peaks for the eight-photo case were about 1.60–1.64 GiB across serial and widths 1–4. They include source preparation and verification and mask the edit-only cost. Phase sampling shows an approximately 72–73 MiB rise during the eight-photo edit at serial, width four and width eight. For two scale-2 images the rise is approximately 265–282 MiB in both serial and width two. Absolute pre-edit baselines differ more than the apparent width penalty; these observations do not demonstrate that wider execution uses less RAM or establish a reliable extra-memory bound.

Allocation activity is identical within each matched workload: about 156.46 MiB of requested GPU storage for eight scale-8 images, and 572.36 MiB for two scale-2 images. These are allocation traffic, not peak residency. Width changes overlap, while cached inputs and delivered results remain retained under both schedules. Meaningful attribution is the goal; exact attribution of insignificant deltas is not a prerequisite for this experiment.

### Profiling validation and ownership review

A separate V4 GPU capture warms all eight inputs before starting, runs one width-eight edit, drains timestamp callbacks, and saves before image readback. It records 56 started and 56 resolved GPU queries, zero unresolved/failed/dropped queries, zero trace loss and eight complete graph explanations. All eight expected task/revision/photo/purpose contexts match, each with six graph dispatches and one view dispatch. Numerical verification remains exact. Captured timings are not substituted for Off measurements.

Resumable preparation and finish scopes are short per-call slices rather than overlapping thread-stack graph spans. Wide delivery/packaging is labelled separately from execution. Trace analysis recognizes resumable graph headers and reports elapsed job time separately from initialization duration. This avoids attributing packaging-only work or a preparation slice as the complete render.

The owner requires attribution for profiling and validation across serial and wide execution. Preserve task, revision, photo, purpose, graph and operation identities, and distinguish CPU activity, elapsed job duration, GPU work and delivery. Memory observations should identify their phase and distinguish allocation traffic, retained ownership and sampled process footprint. Finer allocation attribution is useful when it explains a material difference, rather than a prerequisite for every small delta.

The closing Astra review supports keeping one shared execution lifecycle at widths one through N, with working-set sessions outliving jobs and the coordinator remaining authoritative. The prototype's nested one-job `Driver` wrapper and repeated preparation in `Wide::candidate` are disposable scaffolding. A follow-up should resolve task preparation once, use the same execution object for width one and wider runs, and later feed native, inspection and export recipes through that lifecycle, initially at width one where necessary. Memory availability should constrain this engine's admission rather than create another renderer. The owner has endorsed this common-engine direction; production policy and implementation remain a subsequent review unit.

The final feature workspace run passes 1,181 tests. Default and feature strict Clippy, formatting, documentation checks and 196 Python profiling tests (one skipped) pass. Two focused grouped-execution GPU tests cover exact results, fence-complete progress, pressure protection, cancellation and retry. The existing multi-edit delivery and continuous-drag worker GPU regressions also pass with width eight enabled. These correctness checks do not establish physical UI latency or production memory admission.

Reproduce the warm worker comparison with the pinned fixtures and an explicit width. Use `serial`, `1`, `2`, `4` and `8` in separate runs:

```sh
FOVEONIC_FIXTURE_DIR=path/to/fixtures FOVEONIC_WIDE_RENDER_WIDTH=8 RUST_MIN_STACK=16777216 cargo test --offline --release -p foveonic-gui --features wide-experiment rendering::worker::wide_tests::real_worker_warm_edit_experiment -- --ignored --exact --nocapture --test-threads=1
```

Set `FOVEONIC_WIDE_TEST_COUNT=2 FOVEONIC_WIDE_TEST_SCALE=2` for the half-resolution pair. Freeze the executable and build provenance before comparing timings when worktrees share a build cache. Frozen builds, input identities, raw trials, workload samples, phase memory, failed harness attempts and trace evidence use the `.local/wide-investigation/wide-worker-v2`, `wide-worker-v3` and `wide-worker-v4` output convention. These ignored artifacts are local evidence; the maintained test and tracked input manifest are the reproduction procedure.

### Owner zoom feedback and continuation priority

A subsequent owner-driven six-photo Merrill session in Speed used a 4 GiB cache target on a 16 GiB M1 Pro. Its loss-free GPU capture shows small native regions still materializing approximately 184 MiB of sensor layers, repeated empty native caches, and 44 session evictions for the shared memory budget. Later full-size Current jobs retained approximately 1 GiB of processing cache per photograph. Their execution took roughly one second each, with some request-to-UI observations reaching six to seven seconds. These are GPU-profiled exploratory observations, not matched Off latency measurements or system-memory peaks. The selected photographs' browser-thumbnail load jobs themselves took about 5–8 ms, which excludes waiting for an updated photographic preview to become available.

Every recorded execution attempt for the six late full-size Current tasks was explicitly marked speculative. The resulting slice separates optional completion from foreground progress and busy state, adds shared navigator/scope priority, and supplies the persistent “Complete full images while idle” setting. See the [priority contract](./specs/render-orchestration.md#foreground-and-speculative-execution). Optional completion uses retained native sources and does not reopen an evicted photograph. Reducing whole-image preparation for native regions remains a separate processing experiment. The owner's approximately 1 GB-per-image cache sizing target still needs representative workload validation.

The priority slice passes 1,189 workspace tests and six selected worker GPU regressions, including thumbnail preemption, disabling submitted continuation, regional admission/progress preservation, source-free retry and scope priority. At that checkpoint, fresh photographic navigator/browser content still needed an independent reduced whole-photo request.

The owner then approved the shared-preview migration. A distinct reduced `Preview` target now feeds navigator, browser and whole-photo scopes independently of optional native completion. Viewport frames and native detail remain separately owned. Shared source acquisition retains the strongest consumer priority and retries interrupted lower-priority work without publishing a photograph error. All 1,205 workspace tests and eight selected worker GPU regressions pass. The new fixture-pinned check disables native completion, verifies retained-source reuse and regional-overlay preservation, then compares complete reduced samples/settings/geometry with fresh rendering in both Quality and Speed. Explicit authored output resizing does not enlarge preview execution. A later two-line scope-source integration receives a focused GUI and strict lint repeat; it does not change the verified numerical path. Whole-photo scopes use reduced-preview samples, not a promise of native histogram equality. These checks establish correctness, not native UI latency or cheaper native preparation.

Reproduce the new numerical/ownership check with declared inputs:

```sh
FOVEONIC_FIXTURE_DIR=path/to/fixtures RUST_MIN_STACK=16777216 cargo test --offline --release -p foveonic-gui --features wide-experiment rendering::worker::tests::shared_preview_with_idle_completion_off_matches_fresh_render_and_reuses_native_source -- --ignored --exact --nocapture --test-threads=1
```

### Warm native zoom regions

The next slice generalizes the existing execution lifecycle to native and reduced sessions. `RenderExecution` borrows the appropriate source/cache representation under the existing claim. The worker admits compatible retained Merrill Current Detail regions, with one execution per photograph. Cold acquisition, unsupported requests and optional full-native continuations retain their existing serial paths. Preparation remains serial. This is still an explicitly selected speed experiment with coarse process guards, not production memory admission.

The headless real-worker test uses the first two distinct Merrill originals in the pinned [manifest](../scripts/profiling/wide_inputs.json), a 4 GiB render-cache target, Speed, and 512×512 requested regions. Effective output geometry includes the existing regional support border. Disk caching and background completion are disabled for this isolated cohort. Each trial clears processing history and fully warms the original region before measuring the edit. Exposure changes 0→0.25; chroma changes 0→1; the pan moves the region 576 native pixels horizontally. Fresh complete-image samples, settings and geometry are verified after timing. The comparison uses one frozen binary, ordinary serial versus experimental widths one and two, three measured trials per process plus one excluded warmup, and reversed process order. The final six samples per mode below are uncontended under the workload monitor. Earlier exposure observations overlapping build activity remain excluded from this table.

| Two-photo Speed workload | Serial group median | Width-one group median | Width-two group median | Serial first median | Width-two first median |
| --- | ---: | ---: | ---: | ---: | ---: |
| Exposure in retained region | 10.04 ms | 10.47 ms | 8.88 ms | 5.13 ms | 6.66 ms |
| Chroma in retained region | 43.24 ms | 43.09 ms | 31.61 ms | 20.97 ms | 29.72 ms |
| Pan into a new region | 181.12 ms | 182.42 ms | 156.08 ms | 89.04 ms | 154.22 ms |

The owner clarified that complete-set update time is the deciding metric for a shared edit across zoomed photos; first-image timing is diagnostic for that flow. Exposure and chroma are the practical shared-edit comparisons here. The two-photo pan case synthesizes linked viewport movement, which the current UI does not offer. Its group result is useful for reconstruction attribution and stress coverage, while ordinary single-photo pan latency remains a separate target.

The pan group ranges are 179.01–187.03 ms serial and 154.15–159.45 ms at width two. Chroma ranges are 41.12–45.59 ms serial and 31.03–33.91 ms at width two. Width one broadly matches ordinary rendering. Width two improves group completion, but its breadth delays the first native patch. These observations measure coordinator-classified image receipt and existing view-texture preparation, excluding physical UI drawing and presentation. They do not establish six/eight-photo latency, cold-native performance or continuous-input responsiveness.

Separate process-memory runs use roughly 5 ms physical-footprint sampling and phase markers. Pan process peaks are approximately 2.14 GiB serial, 2.15 GiB width one and 2.23 GiB width two. Measured edit-phase peaks rise by about 0.10 GiB at width two; serial/wide baselines also vary. Chroma process peaks are approximately 2.13/2.13/2.15 GiB. These totals include test verification/preparation and retained baseline frames. They are sampled observations, not guaranteed peaks or a supported 1 GiB-per-photo bound. All selected checks remain below the sampled 4 GiB process guard.

Separate loss-free GPU captures reconcile pan 30/30 and exposure 4/4 queries across the expected two photo contexts, without unresolved queries or emission-transition drops. In the warm pan capture, metadata/checkpoint planning is about 0.81/0.64 ms per photo, while reconstruction dominates. The existing 184 MiB highlight and despeckle checkpoints are reused. Each new region dispatches 45 graph passes: GPU pass durations include Banding about 27–30 ms, NoiseBase 17–19 ms, Pattern 11–13 ms and Column about 6 ms per photo. A roughly 520×520 delivered patch still requires broader support, including a 2528×944 Banding output. Logical native geometry on tiled outputs does not imply a full-frame physical buffer. Same-clock GPU pass intervals overlap across photos, so their summed durations are not additive wall time.

Both pan graphs finish around 154.5 ms in that capture after ten grouped compute windows. Ordinary view preparation and delivery then run independently. The first-result delay therefore comes from advancing both expensive chains together, not from cold source loading or long recipe preparation in this warm case. Exposure dispatches only Tone plus display preparation per photo, making queue/wait/delivery overhead more important than its tiny GPU kernels. An active-photo lead or weighted-round experiment could explore the first/group tradeoff later. Reducing unnecessary reconstruction support is a separate numerical/domain investigation.

The workspace passes 1,215 tests, with 617 opt-in checks ignored, and strict default/feature Clippy. Explicit GPU checks cover native Quality/Speed equality, same-region edits and uncovered pans, submitted cancellation, retry, early failure, source claims, unchanged peers and related overlay/preview/background behavior. A controlled worker hold verifies replacement after an actual two-photo submission window has retired; it does not measure arrival during GPU execution. Quality and Speed worker results, reduced-path regression and all 48 retained experiment runs pass. Independent Astra source review found no remaining issue after fixing telemetry for native sessions evicted after execution and correcting capture configuration attribution.

Reproduce the native worker comparison using the same frozen executable for each selected width:

```sh
FOVEONIC_FIXTURE_DIR=path/to/fixtures FOVEONIC_WIDE_RENDER_WIDTH=2 FOVEONIC_WIDE_TEST_COUNT=2 FOVEONIC_WIDE_TEST_SCALE=1 FOVEONIC_WIDE_TEST_QUALITY=speed FOVEONIC_WIDE_TEST_CASE=pan FOVEONIC_WIDE_TEST_TRIALS=3 RUST_MIN_STACK=16777216 cargo test --offline --release -p foveonic-gui --features wide-experiment rendering::worker::wide_tests::real_worker_warm_edit_experiment -- --ignored --exact --nocapture --test-threads=1
```

Select `serial`, `1` and `2` in reversed orders. Use `exposure` or `chroma` for the other edits, and `quality` for Quality checks. `FOVEONIC_WIDE_MEMORY_MARKERS` enables phase files for separate memory runs. `wide_native_profiled_edit_attribution` records the two-photo Speed GPU capture with `FOVEONIC_WIDE_CAPTURE_DIR` naming a fresh output directory. Keep capture/readback runs separate from Off timing. Source hashes and executable provenance are required when worktrees share a build cache.

### Four-photo native edits and shared execution

A four-photo Speed pilot and 19 subsequent comparison/memory runs use the first four pinned Merrill originals, the same 512×512 native region, and a 4 GiB cache target. Each comparison uses one excluded warmup and three measured trials per process in reversed mode orders. Complete-set update is the acceptance metric. All cells verify complete pixels/settings/geometry, zero timed source acquisition and actual requested submission width.

| Four-photo shared edit | Ordinary serial | Width one | Width two | Width four |
| --- | ---: | ---: | ---: | ---: |
| Chroma group median | 71.81 ms | 74.68 ms | 55.35 ms | 44.61 ms |
| Exposure group median | 21.02 ms | 21.46 ms | 18.31 ms | 16.75 ms |

Each entry has six measured samples. Chroma ranges are 70.02–74.69 ms serial and 43.28–76.90 ms at width four. No tracked compiler/render process overlaps these observations, and no new swapouts occur. System compression/decompression, swap-ins and some pageouts occur across process intervals, which include setup and verification. These are not memory-quiet timings, and the width-four outlier cannot be attributed to a specific OS event from this evidence. The median group reductions are approximately 38% for chroma and 20% for exposure.

Separate roughly 5 ms memory samples observe whole-process peaks of about 3.52 GiB serial, 3.50 GiB width two and 3.59 GiB width four. Measured edit-phase peaks rise with width even when cold setup masks that difference in whole-process maxima. Retained worker inputs after warming are approximately 2.75 GiB. All cells stay within the sampled 4 GiB process guard. These include harness preparation, verification and retained frames, and are not guaranteed bounds or a general per-photo recommendation.

Eight-photo native editing remains unmeasured because current host headroom is insufficient for the selected guarded experiment plus margin. This does not imply that width eight is unsupported. Use `FOVEONIC_WIDE_TEST_COUNT=4 FOVEONIC_WIDE_TEST_CACHE_GIB=4` with the native worker command above to reproduce the four-photo cohort. Keep the cache target constant across compared widths. The new cache parameter applies only to the native benchmark and reports its byte value; reduced configuration is unchanged.

Following these measurements, the shared-lifecycle cleanup removes the redundant one-job Driver container. `RenderExecution` now owns one boxed job directly. The worker's ordinary Merrill native final-image branch and reduced path drive that same lifecycle synchronously at width one, while the opt-in worker advances it through grouped windows. Existing source/session owners, native attempted-settings tracking, reduced successful-settings tracking, native cache-stage profiling and safe retirement remain unchanged. Camera/sampling resolution is shared rather than repeated during every candidate scan. Cold acquisition and unsupported tool work retain their serial paths. This is an ownership cleanup, not a width default or memory-admission change.

The shared-path cleanup passes 1,215 workspace tests (619 opt-in checks ignored), strict default/feature Clippy and 20 selected core/worker GPU regressions. Two native helper regressions compare complete pixels, geometry, progress and cache-stage profiling with ordinary rendering, including missing input, invalid preparation, early/submitted cancellation and retry. Related source reuse, independent peers, overlay, preview, scopes, idle completion and continuous-drag checks pass. Eight post-cleanup worker cohort runs also pass. Four-photo chroma group medians are 71.70 ms ordinary, 71.57 ms width one and 45.15 ms width four; the width-four range is 44.34–49.36 ms. These follow-up measurements establish no material throughput regression from the ownership cleanup; they retain the same headless endpoint and OS-activity qualifications. Persistent width selection and conservative memory admission remain separate implementation work.

## Earlier probe recommendation and remaining work

This section records the boundary before the later real-worker experiment; it does not supersede that experiment's implementation or results above.

Keep the reviewed width-one cursor extraction as the first owner-testable production slice. It preserves numerical output and current completion policy. Its fresh matched Fast and Thumb processing checks show no material regression. The next experiment should compare completion wakeup/polling and bounded within-photo batching through the real application path, alongside an explicitly selected width-two policy. The measured gains do not require several CPU workers or a same-stage barrier.

Before wider admission, extend shared working-set protection, reserve temporary and submitted-work resources, and account for completed images awaiting delivery. Retire obsolete work safely and prefer the Active image. The coordinator should retain one authority for claiming jobs. Several independent executions can be driven by one CPU orchestrator.

Still unproven are configured 1/2 GiB admission behavior, transient memory bounds, sustained real input, interruption during long operations, bounded delivery under a paused UI, source replacement during execution, clear/shutdown, frozen exports, native detail, and background contention through an integrated wide worker. The functional extraction and opt-in engine probe do not supply that integration. Review the separate opening-state fix before relying on unmodified application selection journeys as a latency gate.

Workspace tests pass serially with `RUST_MIN_STACK=16777216`. Strict workspace Clippy, formatting, documentation indexes, Python profiling tests and artwork tests are also checked. Parallel debug tests encountered existing timeouts under contention, and default-stack GUI tests overflowed. The larger-stack serial run is the reported complete test result.

## Reproduction

Use [wide_suite.py](../scripts/profiling/wide_suite.py) to run the investigation serially. Build and freeze the probe and ordinary renderer separately. The probe build record must bind the executable SHA-256 to its source-tree hash, revision, command and Rust toolchain. The runner checks that hash and records its own source separately. Keep the probe executable named `profile_wide` so workload monitoring recognizes competing instances.

```sh
python3 scripts/profiling/wide_suite.py --binary path/to/frozen/profile_wide --build-provenance path/to/profile_wide.build.json --ordinary-binary path/to/frozen/profile_pipeline --source-dir path/to/example-photographs --directory path/to/new-investigation
python3 scripts/profiling/wide_suite.py --binary path/to/frozen/profile_wide --build-provenance path/to/profile_wide.build.json --ordinary-binary path/to/frozen/profile_pipeline --source-dir path/to/example-photographs --directory path/to/new-supplement --supplement
python3 scripts/profiling/wide.py --binary path/to/frozen/profile_wide --build-provenance path/to/profile_wide.build.json --source-dir path/to/example-photographs --directory path/to/new-poll-depth --counts 8 --compare-depths --poll-us 100 --trials 7
python3 scripts/profiling/wide_stress.py --binary path/to/frozen/profile_wide --build-provenance path/to/profile_wide.build.json --source-dir path/to/example-photographs --directory path/to/new-live-interruption --skip-memory --cold-graph --require-unfinished-peer --repeats 3
```

For the separate warm and deferred recovery cases, replace `--cold-graph --require-unfinished-peer` with no extra flags or with `--defer-finished`. The live-peer requirement belongs to the cold case. Use distinct output directories. `wide_native.py --help` describes paired native polling contexts and their frozen GUI provenance. Its fail-fast behavior deliberately stops after an unsettled scenario. The worker context uses the existing `profile_editing REPORT.jsonl --resolution thumb --workers N --repeats 7 --mode off --edit exposure --verify ORIGINAL...` with `N` equal to one, two and four, twice in reversed order. Keep native, worker and reduced-session results separate.

The optional `--allow-contended-observations` flag retains exploratory data while excluding it from clean distributions. It does not waive numerical checks. Source and runner copies, per-trial reports, workload samples, physical-memory samples and serial expected pixels remain in the owned output directory. Duplicate candidate pixels may be removed after exact validation while retaining hashes and expected samples.

Shared Cargo target storage requires particular care: a standalone test invocation reused another worktree's older test executable during this session. Changed crates were explicitly recompiled from the intended worktree, test names were inspected, and executables were frozen before validation. No shared-cache cleanup or other checkout modification was needed.

Retained serial pixel controls may be archived losslessly to recover investigation space. Verify the decoded byte count and SHA-256 before removing an uncompressed copy, and retain a storage mapping. Exact duplicate candidate buffers may be removed only after rechecking their hashes against retained controls. Preserve source snapshots, reports, comparison results and original hashes.
