---
summary: "Measure data movement, preparation, scheduling, synchronization and delivery overhead before selecting current-architecture optimizations."
---

# Coordination overhead profiling

Requirements: [profiling](../docs/specs/profiling.md) and [shared diagnostics](../docs/specs/performance-diagnostics.md). This investigation supplies runtime evidence to the [coordination consistency inventory](./render-coordination-consistency.md). The owner requested measurement of overall shuffling and waiting, including data movement and asynchronous handoffs, before deciding whether improvements fit the current architecture or should wait for the larger redesign.

Keep production scheduling, cache policy and node arithmetic unchanged during attribution. Use the existing maintained headless and native scenario tools, isolated inputs and local cache namespaces. Check for competing apps/compilers before each run and retain workload observations. Compare ordinary Off latency separately from shared Wall/GPU trace attribution. GPU time and inclusive host waits overlap and must not be added as independent work.

## Questions

- How much latency comes from original/decoded-source access, metadata recreation and native preparation, versus retained editing?
- Which CPU preparation, GPU uploads/copies/readbacks, submissions and completion waits lie on the path to a usable image?
- How much time falls between useful processing, reply publication, UI admission and draw submission?
- Does cost scale with pixels, executed operations, photographs or handoffs?
- Are there measured opportunities within current ownership and execution boundaries, with explicit correctness and responsiveness constraints?

## Measurements

- [x] Record build/input identities and a quiet-machine baseline.
- [x] Measure source-free retained multi-photo edits with separate sessions and fresh-output verification. Use single-photo controls for completion policy.
- [x] Capture actual-app warm selection and edits, including decoded disk versus retained processing state.
- [x] Attribute preparation, transfer, synchronization, scheduling and delivery with shared traces.
- [x] Use matched focused experiments where attribution warrants them. Keep changes isolated from defaults and report confounders.
- [x] Record practical opportunities, estimated scope and deferred work.
- [ ] Review findings with the owner and select implementation slices alongside the wider consistency inventory.

## Interpretation

Headless source opening is outside retained-edit measurements. Native warm-disk selection can still read local decoded payloads and rebuild processing state. Physical screen presentation is outside the scenario harness. Separate fixed setup and first-use shader costs from steady-state edits. Treat instrumented elapsed spans as wall time, not CPU-active samples, and distinguish a wait that overlaps useful GPU work from an idle scheduling gap.

## Measured setup

Measured the release build at `0fa7bbe` on an Apple M1 Pro with eight logical CPUs, 16 GiB RAM and the Metal backend. Executables and input files were hashed and frozen before measurement. Workload monitoring found no competing Foveonic, compiler or measured workload. No production code, defaults or owner sidecars changed. Shared captures completed without reported loss. Native scenario behavior checks and headless cached-versus-fresh verification passed.

The eight-photo set contains blue-parking-sign, dp0-quattro, merrill-hazy-coast, rocks-and-tree-shadows, sd-quattro-h, sd-quattro, sd14 and sd9. Thumb uses divisor eight for Merrill/Quattro and four for Classic. Native runs use isolated sidecars, decoded disk cache and preferences through the maintained [scenario runner](../scripts/profiling/scenarios.py). Default photographic controls provide a common workload. These results do not establish costs for every optional inspection, export or photographic control.

Headless measurements use `profile_editing` with `--workers 1 --reduced --gui-analysis --quality thumb --edit exposure --verify`, with five measured groups. Native `warm-edit` uses three Off trials with five edits each. GPU and Wall captures are separate diagnostic runs. Preparation is measured from primed local decoded disk payloads, not an SD card. Filesystem caching is uncontrolled and input hashing can warm it.

## End-to-end results

| Workload | Ordinary Off elapsed | Samples and meaning |
| --- | --- | --- |
| Eight-photo retained exposure edit, headless | 59.45 ms median | Five groups, completed image and display preparation. Source opening excluded. |
| Eight-photo retained exposure edit, native app | 117.04 ms median, 126.85 ms maximum | Fifteen groups across three launches at 1 GiB memory budget. All targets delivered and editable. |
| Eight-photo selection, decoded disk ready but processing cold | 2055.05 ms median | Two native launches at 2 GiB memory budget, matching the prepared-selection budget. Rebuilds working processing state. |
| Eight-photo selection, reduced inputs already prepared | 219.57 ms median | Two native launches at 2 GiB memory budget. Preparation completes before measured selection. |
| Six-photo reconstruction edit, first changed settings | 185.61–187.18 ms | Headless, Merrill/Quattro. The selected reconstruction controls are unsupported on Classic. |
| Same six-photo reconstruction sequence, retained revisits | 57.75–59.44 ms | Three subsequent groups reuse retained work. |

The headless and native exposure sequences have different endpoints and presentation lifetimes. Native relative edits alternate original exposure and +0.25 EV. Headless alternates +0.25 and −0.25 EV. Their difference is context, not a matched estimate of avoidable UI overhead. Native scenarios finish after application observation of delivery and draw encoding. They do not measure physical display presentation.

Prewarming demonstrably changes the work on selection. Its GPU capture reads zero source payloads and executes only eight photographic and eight display dispatches. The cold-processing selection reads eight decoded payloads totaling 576,416,680 bytes and executes 681 photographic/display dispatches. It is still a decoded-cache hit. The two readiness states have very different costs. Prewarming moves preparation out of the interaction interval and consumes memory and idle execution time.

## Attribution

### Warm edits

The native Wall capture takes 130.07 ms median over five groups, compared with 117.04 ms in the separate Off dataset. Selected worker intervals below describe the Wall run. They are inclusive wall intervals and several nest inside each other. Do not add the rows into a total or interpret the waits as wholly idle CPU/GPU time.

| Interval per eight-photo group | Median | Interpretation |
| --- | --- | --- |
| Eight render tasks | 111.30 ms | Includes numerical preparation, submissions, waits and publication. |
| Interactive graph completion waits | 20.47 ms | One per newly executed photographic operation in this workload. |
| Display completion waits | 23.09 ms | Eight additional worker waits before publishing display textures. |
| GPU submit calls | 23.43 ms | Can include backend work and contention with other producers on the shared queue. |
| GPU uploads | 1.33 ms | Response tables, not full sensor payloads. |
| Memory trimming/accounting | 10.11 ms | 16–18 calls per group, includes inventory and eviction policy. The scan component is not separately attributed. |
| Interactive checkpoint planning | 3.27 ms | Graph reuse/planning remains small but measurable. |

Warm edits perform no source payload reads and no production image readbacks or device copies. The eight responses upload about 6 MiB total. The separate GPU capture measures 6.54–8.90 ms of image-task GPU pass time per group. GPU-query capture itself adds submissions and small timestamp readbacks and takes 136–150 ms per group. Its clocks are uncalibrated against the host. These pass sums cannot prove that the remaining host time is avoidable overhead.

Actual reply-channel sends are tiny in these captures. The larger `reply_publication` span includes memory trimming before the send. UI receipt to native draw encoding is generally another 6–8 ms per image, overlapping later worker work. Serial queue wait for a later photograph includes time processing earlier photographs. Paused background priority waits can overlap the entire foreground interval and must not be summed into its latency.

The aggregate Wall headless run also exposes CPU recipe preparation that is not fully scoped in the SDK-only native capture. Merrill preparation takes about 0.36–0.42 ms per photo. Quattro takes about 5.58–9.09 ms, mostly response-curve construction from exposure and tone controls. This is real numerical preparation in [Quattro appearance](../crates/foveonic/src/render/recipes/quattro/appearance.rs), not merely message passing. Reusing invariant preparation may be useful, but changing the curve algorithm is outside this coordination pass.

### Selection from decoded disk

The native Wall cold-processing selection takes 2096.47 ms. Source opening totals 187.19 ms, including local payload reads and conversion. Processing totals 1779.28 ms. Inside processing, upload spans total 191.57 ms, submit spans 270.77 ms and interactive completion waits 967.41 ms. These overlap useful GPU execution and contain nested work. A separate GPU capture measures about 517 ms of image-task GPU passes. Native repair preparation also contributes measurable CPU work.

Thus neither decoded disk I/O nor arithmetic alone explains the whole selection interval. The current path repeatedly prepares, submits and completes operations while advancing through each photo serially. Retaining/preparing reduced inputs avoids most of that path entirely. This investigation does not demonstrate that removing every wait would be safe or recover its full measured duration.

## Focused completion experiments

These isolated public-API probes run without optional recording. They change no application defaults.

1. **Fresh render API comparison.** Alternate stateless `Engine::render` with a fresh `RenderSession` and `render_session` using a cancellation predicate that always returns false. Use the same resident decoded source, settings and explicit scale. Finish both with GPU completion, exclude the first pair and measure six pairs per file. The first pair's output is byte-exact. Both routes have identical dispatch, submission, upload, device-copy and allocation counts.

   | Input | Stateless | Fresh interactive session |
   | --- | --- | --- |
   | Blue parking sign, Thumb | 103.25 ms | 132.49 ms |
   | DP0 Quattro, Thumb | 143.11 ms | 185.70 ms |
   | Quattro H, Thumb | 121.97 ms | 159.73 ms |
   | Blue parking sign, Full | 555.17 ms | 663.17 ms |

   Completion policy is worth examining. This comparison also changes checkpoint retention and temporary resource lifetimes, so the difference cannot be attributed only to polling. The stateless route is not a replacement for interactive cancellation and cache reuse.

2. **One completion versus two.** Prepare a fixed Thumb image, then issue the same two `view_texture` operations either waiting after each or waiting once after both. Retain both textures until completion. Alternate order for 101 pairs and exclude the first pair. Over 100 measured pairs, separate waits take 3.188 ms median and one final wait takes 1.644 ms. Both perform two submissions and two dispatches, with identical allocation/uniform counts and no image uploads, copies or readbacks. Fewer completion boundaries can materially reduce latency without reducing numerical work. Applying that principle to the application still requires cancellation, resource lifetime and drawing checks.

3. **Bounded polling, rejected.** For one display operation, compare normal completion waiting with up to 500 µs of nonblocking polling followed by the normal wait. Both routes register the same completion callback. Across 100 alternating measured pairs, normal waiting takes 1.332 ms median and bounded polling takes 1.831 ms. Polling falls back to waiting in 97/100 cases. Spinning is not justified by this experiment.

## Opportunities within the current architecture

These are measured opportunities, not an implementation order. The owner's priorities and the [proposed cleanup sequence](./render-coordination-consistency.md#proposed-cleanup-sequence) put coherence first, overhead reduction second and new cache behavior third. Prewarming remains deferred while the current boundaries are cleaned up.

| Candidate | Evidence and bounded change | Required checks |
| --- | --- | --- |
| Make completion policy explicit | [Executor](../crates/foveonic/src/render/execute.rs) currently couples the presence of cancellation to per-operation waits. [Worker](../crates/foveonic-gui/src/worker.rs) adds display completion. Evaluate combining compatible work before a necessary completion, keeping existing worker and cache owners. | Rapid supersession, bounded queued GPU work and temporary memory, foreground interruption, output equality and multi-photo delivery. Keep cancellation checks distinct from GPU completion. |
| Reduce repeated memory inventory work | About 10 ms across repeated trim intervals per group. Examine whether unchanged resource snapshots are recounted and whether before/after passes can share an inventory. | Shared allocation deduplication, queued replies, active protections, eviction and memory pressure. Measure actual accounting separately before promising savings. |
| Coordinate auxiliary GPU producers consistently | Browser snapshots are prepared on UI receipt while navigator/persisted previews use separate workers and deferral. They share the device queue with foreground rendering. | Matched scenarios that suppress/defer only optional work, verify main-image latency, thumbnail freshness and starvation. Existing traces do not isolate the complete interference cost. |
| Preserve effective reduced-input prewarming | Prepared selection avoids source payload reads and native preparation. Use the existing background path and readiness distinction. The matched 2 GiB selection runs confirm a large interaction-latency difference. | Idle interruption, constrained memory budgets and no accidental promotion of speculative work into foreground use. A green decoded-cache status alone does not imply prepared processing. |
| Reuse invariant recipe preparation | Quattro response preparation and first-use repair preparation are measurable. Identify immutable inputs or redundant construction before optimizing arithmetic. | Numerical identity, correct exposure/tone invalidation and clear preparation ownership. |

Defer stage-grouped execution of several photos, a central CPU/GPU admission processor and cross-executor resource ownership to the orchestration unit. They may improve overlap but change execution ownership substantially. A generic async runtime, channel replacement or universal worker abstraction is not supported by the measured channel costs.

## Lessons for diagnostics

Keep an Off baseline beside every diagnostic capture. Shared GPU queries measurably change these short workloads. GPU pass time, inclusive host spans, queue backlog and end-to-end latency answer different questions.

Preparation scopes should use the shared recorder's enabled state independently of aggregate profiling. Some timers begin only when aggregate profiling is enabled, leaving CPU recipe authoring inside an unscoped native `processing` interval. Record this with inventory H1 before implementing instrumentation cleanup. Keep transfer observations separated into source payload I/O, sensor upload, small response tables, device copies and profiling-only readbacks. Preserve the existing real-app scenario and workload keys rather than creating a second benchmark protocol.
