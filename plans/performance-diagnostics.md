---
summary: "Integrate the owner-approved Perfetto recorder and application scenarios, then validate causal coverage, capture completeness and overhead before renderer experiments."
---

# Application performance diagnostics

Implements the accepted [diagnostic requirements](../docs/specs/performance-diagnostics.md). The owner approved the shared Perfetto SDK backend, migration of existing diagnostic paths and repeatable real-app scenarios. Implementation is active. The foundation has passed workspace, asynchronous GPU and initial native checks. Automated acceptance and controlled overhead comparisons are complete. Physical manual-control verification remains pending desktop availability. This work does not authorize a renderer, scheduling or cache-policy redesign.

The [profiling guide](../docs/profiling.md) documents manual capture, scripted scenarios, local Trace Processor analysis and existing aggregate benchmark reports. Keep commands there and implementation progress here.

## Focused stabilization review

The owner chose a focused trust review before correcting the Thumb engine path, followed by cleanup within existing application owners. Further orchestration changes remain deferred.

- [x] Replace purpose-name filtering in work analysis with existing task/photo lineage. Task zero describes work outside the foreground coordinator. Reads of selected targets remain visible regardless of task identity.
- [x] Preserve manual recordings on exit even without a configured save parent. Ordinary unrecorded exits still create no artifact.
- [x] Configure periodic incremental-state refresh for ring-buffer traces. A forced-overflow SDK capture retained a decodable tail event after about 5.5 MB of overwritten data. Overflow remains a validity failure.
- [x] Finish short real-app capture checks. Native source-opening close without a configured parent and the manual-exit SDK lifecycle test pass. A complete loss-free eight-photo cached-switch scenario passes expected-read, delivery and settlement assertions.
- [x] Finish the Thumb boundary census and move to engine corrections before further application restructuring.

Lessons: the controller must own starting, active and saving states through shutdown. A saved artifact is distinct from a loss-free measurement. Retained tail events must carry recoverable descriptors. Work attribution must follow task/photo ownership rather than particular producer names. A reader created and cancelled before its first I/O is reported separately from an actual payload read. Failed and incomplete read attempts remain visible once I/O begins. These are corrections to the existing recorder and analyzer, without another recording path or scheduler.

## Selected architecture

Use one SDK-backed Perfetto recorder behind the processor's shared diagnostic API. Scoped CPU work, typed events, flows, counters, retrospective elapsed observations and GPU observations share its capture lifecycle. The GUI timing facade uses this backend. Manual and scripted captures save the same trace, manifest and summary. Replaced native stderr JSON recording and synthetic headless timeline placement are removed. Scientific harnesses retain explicitly identified aggregate JSON reports.

Reuse coordinator task, original request revision, photo, purpose, artifact and graph-node identities. Presentation revision is separate from the delivered task's lineage. A cached or coalesced task must not create fictitious execution. CPU scopes describe elapsed work on their real thread. Retrospective intervals carry explicit clock/start/end/duration fields instead of fabricated nesting. GPU intervals retain their own clock, submission and node provenance, with calibration availability explicit.

Instrumentation belongs at existing ownership boundaries: input dispatch, coordinator transitions, mailbox delivery, cache decisions, actual source reads, CPU-pool work, graph execution, shader creation, GPU submission/resolution and image receipt/use. Payload storage kinds distinguish original and decoded-cache RAW reads from preview and metadata I/O. No inspection should load data or change recency solely to observe it.

The backend records a bounded local session. Start/stop/save work runs through its existing background control path. GPU timestamp queries resolve after normal completion rather than adding diagnostic completion waits. Missing observations, query drops and capture transitions remain visible. Trace Processor is an explicitly supplied local executable, with version/hash recorded in analysis. Normal runs do not acquire tools from the network.

## Implemented integration

These items describe implemented code, not completed native acceptance:

- [x] Add the shared SDK recorder, typed context/events/scopes, flows, counters and capture artifacts.
- [x] Prove SDK capture and local SQL decoding with cross-thread flow and overlapping retrospective elapsed observations.
- [x] Migrate GUI timing and capture control to the shared backend, with start/mark/stop/save while the app remains open.
- [x] Connect task admission, completion outcomes, image receipt and native encoding to original delivered identities.
- [x] Add source-read storage provenance and graph/node observations at existing owners.
- [x] Add asynchronous GPU observations and query lifecycle reporting without introducing profiling-only waits.
- [x] Extend the maintained native smoke driver with named scenarios and declared direct-command versus input-dispatch coverage.
- [x] Save existing smoke suites through the same controller before automatic exit, with suite/target checkpoints and separately scoped screenshot encoding.
- [x] Add runner input/build manifests, isolated cache namespaces, separate Off baselines and portable analyzer tests.
- [x] Refuse native/scenario app launches while competing app/build workloads run and separate observed contention from behavioral validity.
- [x] Replace synthetic timeline export with actual Trace Processor queries, preserving scientific aggregate report summaries.
- [x] Verify configured review captures save on orderly exit, including a pending startup or manual save, then relaunch for owner reproduction.

An owner review exposed a capture-lifecycle gap: the recording ran for ordinary application use, but closing without the manual save action discarded the in-memory trace. Its eight-photo selection cannot be attributed after the fact. The controller now uses its existing recorder/save path on orderly exit when a capture directory was explicitly configured. An already-requested save is awaited rather than replaced. Normal unrecorded exits create no capture, and smoke panic preservation keeps its existing failure annotation. Shutdown waits occur after interaction ends. Validate this through the SDK and a native close before another review run.

The first native source-opening close saved a complete trace but timed out during teardown. A thread sample identified the UI joining the browser metadata loader while that loader waited behind thumbnail priority. The thumbnail worker was blocked on a full image reply channel that the closed UI no longer drained. Browser shutdown now stops both image loaders before its metadata field joins, releasing queued thumbnail demand. The existing image-loader destructor delegates to the same stop operation. A deterministic bounded-mailbox test checks that stopping releases priority without draining image replies. This is an observed shutdown-order correction, not a change to render scheduling policy. Keep the failed native close evidence and verify the corrected source-opening close.

Portable tests cover CSV/typed argument decoding, shared revisions across distinct photos/tasks, completion order independence, expected scale and payload reads, incomplete settlement, capture-loss handling, caller-owned process cleanup and artifact preservation. SDK query proofs establish backend representation and SQL compatibility. They do not prove native app behavior or capture overhead.

## Lessons from the first complete slice

The first real eight-photo warm-disk capture read eight decoded-cache payloads and no original RAW payloads. Processing still rebuilt native sensor dependencies after decoded loading. Cache readiness and warm processing are separate conditions, so the wider investigation follows task lineage and actual reads rather than a readiness badge.

Native drawing must retain the task and revision that produced its image, independently of later UI revisions. Scenario checks now require native encoding for every current photo identity and scale, including cached images with no new receipt. Trace queries preserve real parent/depth and thread ownership to expose the instrumented call hierarchy. This hierarchy is scoped application work, not a sampler of every Rust or driver function.

The SDK's CSV output does not reliably escape embedded JSON quotes. Query text is hex-encoded in SQL and decoded after structural parsing. This keeps metadata lossless without a second parser or writer. Disabled I/O observation delegates bulk reads to the underlying reader to preserve its normal allocation strategy.

A real Quattro H preparation node exceeded the demo's eight pending timestamp traces. The shared device limit is 32, matching Metal's documented counter-sample-buffer limit and covering the observed 23-trace burst. The explicit timestamp/query/readback payload is bounded to 3 MiB, with driver object overhead additional. No polling or completion waits were added. Query loss remains visible and invalidates device-coverage claims independently of host trace validity.

The full pass extends existing job owners for folder discovery, browser loading/metadata, sidecars, derived previews, navigator and batch/export work. Checkpoint admission and eviction stay at the checkpoint cache owner. Existing smoke suites save through the same controller before exit, with artifact encoding separated from application work. A real Smart failure demonstrated why opt-in fatal smoke unwinds also preserve a failure capture before the original panic exits. External termination and failures before App creation remain outside that mechanism. No new scheduling or cache policy is needed.

Before timing, check actual executing app/compiler/measurement programs. Record overlapping workloads throughout each run, exclude only the owned child app, and keep timing validity separate from behavioral success. Detailed attribution, baseline latency and numerical equality remain separate measurements.

## Native and manual acceptance

- [x] Build the coordinated backend, GUI controller, provenance and scenario integration and complete ordinary workspace checks.
- [x] Capture a real initial multi-photo selection and warm-disk/cold-processing selection. Confirm actual original/local RAW reads, preparation/cache reuse, queue residence, graph work and native image encoding.
- [x] Exercise retained warm editing, cached switching/re-addition, rapid supersession and memory pressure through the actual app.
- [x] Verify shared captures for existing smoke suites without changing their readiness assertions, including intentional pending previews.
- [x] Verify one Draft photograph and one through eight Thumb photographs, with actual delivered scales/settings and complete desired-work/progress settlement.
- [ ] Verify manual start, marker and stop/save while the app remains open, including repeated captures and preserved partial/failing artifacts.
- [x] Inspect Trace Processor loss statistics, explicit emission-transition drops, task outcomes and GPU query completeness in real captures.
- [x] Run repeated matched Off/Wall comparisons, separate GPU attribution, recorder overhead and bounded-buffer/loss checks. GPU attribution is not treated as an Off latency baseline.
- [x] Associate fixture/recipe manifests with independent headless cached-versus-fresh numerical checks outside timed native intervals.
- [x] Regenerate documentation/source indexes and run the documented checks after coordinated integration.

The accepted native suite covers Smart, Crop, Rotate, Browser, Status, White Balance, Import and Multi-edit. Shared captures pass their original readiness checks. The Smart reopen check now requires exact retained texture, delivered task and scale, after seeding the isolated session with authored settings. Its failure exposed an ordinary same-photo selection clearing settled worker ownership and recipe readiness unconditionally resubmitting. The fix preserves only a compatible same-photo worker owner and skips submission only for the exact settled/admitted delivered task with matching settings, framing and current source identity. A separate scripted preset proves staged recipes are read and the declared exposure reaches the first current image.

The manual Start/Mark/StopSave implementation uses the same controller and has SDK lifecycle, distinct-save and fatal-unwind tests. Physical desktop automation later failed with `cgWindowNotFound` for every app, including the previously inspected Perfetto viewer. The owned manual test process was closed with recording Off. Keep the physical-control acceptance item open until the desktop is available.

Native multi-photo overview deliberately uses Thumb. Multi-photo Draft is a headless scheduling baseline, not a hidden native product path. The scenario driver must not suppress ordinary application requests to force it. Cases accept declared caller inputs and optional recipes, using owned hard links and sidecars. The namespace is a validated component under the existing temporary cache root and does not expose a new arbitrary cache path. Default isolation protects the owner's live cache. Intended warm/cold state must still be checked against actual identity and reads.

The first complete slice must explain a cached eight-photo selection from action through final native encoding: original versus local payload reads, reused or rebuilt inputs, CPU and GPU work, queue delay, current results and final task settlement. A saved trace alone is insufficient evidence.

## Measurement findings and limits

The SDK proof records scoped CPU work, cross-thread flows, typed arguments and retrospective elapsed observations readable by the selected Trace Processor. A service-lifetime `traced_chunks_discarded` statistic can reflect earlier sessions, so it stays visible as a warning without alone invalidating the current capture. Current-capture checks include nonzero data-loss/error statistics, per-buffer overwrite/discard records and explicit emission-transition drops. Successful stop/save establishes `complete`, not loss-free validity.

GPU query status is separate from host trace validity. Preserve raw ticks, period, clock ID, submission and context. Missing calibration forbids inventing a common CPU/GPU position. Native encoding does not establish physical screen presentation. Nested/parallel elapsed scopes are not additive and do not measure CPU-active time.

Scenario setup includes a single active image and device before the measured group, and input hashing warms the OS file cache. Initial selection therefore does not claim cold-device I/O. Warm-disk setup primes local decoded entries and resets processing ownership. Expected-read assertions and retained frame identities establish what was reused. Fresh numerical equality remains an independent verification, rather than an inference from current settings or a screenshot.

## Measured cached-selection cause

Controlled runs on Apple M1 Pro with Metal used an isolated decoded-cache namespace and a one-GiB retained-memory preference. For the owner's eight-file Merrill set, three Diagnostics Off trials measured a median 5.789 seconds for warm-disk/cold-processing selection. Fifteen retained exposure edits measured a median 220 ms per eight-photo group. These native group intervals include application delivery and the driver's normal observation cadence.

The detailed capture independently verified eight decoded-cache hits, about 1.560 GB of local payload reads, no original RAW payload reads and no source rereads during retained edits. All 226 GPU query observations resolved, including 222 for current images and four for scopes. Initial processing still executes native repair, highlight restoration and banding before sensor pooling. The green cache indicator describes decoded samples, rather than these processed checkpoints.

Highlight restoration costs 83–289 ms of measured GPU execution per image. Its content-dependent blue-correction-table pass ranges from 15 to 206 ms. Native banding costs 99–109 ms and repair about 72–75 ms. Host submission scopes span 85–132 ms per photo. Numeric repair-record preparation is smaller at 15–17 ms. Existing graph-stage completion waits span 411–620 ms per photo. These host intervals overlap their parents and GPU work, so they must not be added together or interpreted as CPU utilization. No shader creation occurred within the measured selection capture.

The evidence identifies native highlight-table execution as the first kernel investigation target, followed by banding and repair. Reusing valid corrected/reduced inputs can also avoid this prefix, but extending their preparation or retention policy remains a separate decision. Faster RAW decompression, a different badge or shader-cache changes would not remove the measured native GPU work.

A separate mixed-camera matrix measured Diagnostics Off initial selection at 5.214 seconds median and retained edits at 186.7 ms median. Wall capture measured 5.275 seconds and 212.1 ms respectively. Each used three trials with fifteen retained edits total. Detailed GPU capture is a separate attribution run. The recorder-only microbenchmark measured 7.05 ns per disabled scope-plus-event pair and 814 ns enabled over 100,000 pairs. These figures describe this build and machine, not portable latency budgets. Native polling, work distribution and instrumentation affect small groups, so automated latency comparisons accept only matched Off reports.

The deliberate 64-KiB overflow proof recorded overwritten bytes/chunks and an incremental-sequence loss. Analysis rejects that capture while accepting the loss-free proof. Auxiliary GPU work still active at stop remains explicitly incomplete by purpose. A mixed-camera recording can have all current-image queries resolved while a final scopes query is pending.

## Decoded-cache read attribution

The decoded-cache lookup previously exposed payload reads, checksums and integer-to-float conversion as one combined interval. Add scoped attribution around those operations and the surrounding file, allocation, metadata and validation work without changing the cache format or scheduling.

- [x] Instrument the existing cache read using bounded, opt-in Perfetto scopes, retaining chunk boundaries and cancellation behavior.
- [x] Verify the capture on real warm-disk app scenarios and compare its cost with separate Diagnostics Off runs.
- [x] Run the ordinary cache and workspace checks and exact original-sample round trips across camera families.

Three isolated, OS-warm two-photo captures show a median decoded-cache lookup of 19.87 ms for the Merrill sample and 15.80 ms for the Quattro sample. Median sample reads take 7.21 and 4.75 ms, checksum updates 4.23 and 2.85 ms, and integer-to-float conversion 3.66 and 2.39 ms. Buffer allocation takes 2.00 and 1.94 ms, and camera metadata parsing takes 2.02 and 2.77 ms. These are separately recorded host elapsed scopes; allocation can defer page costs into later work. They are instrumented observations, not physical disk or CPU-active timings.

Sixteen foreground reads across five captures contain the expected cache scopes and matching per-chunk read, checksum and conversion counts. An eight-photo Thumb capture records all eight cache hits without trace loss; it contains about 8,700 scopes in a 1.6 MB trace. Three matched two-photo Off trials before and after instrumentation measured 565 and 578 ms median group completion, versus 588 ms with Wall capture. These small samples include the scenario's 20 ms observation cadence and do not establish a tracing regression or precise capture overhead. The original-sample cache round trips remain exact across Classic, Merrill, Quattro and Quattro H. Forty Full/Fast/Draft/Thumb photographic comparisons remain bit-exact against the existing goldens.

## Original decode attribution

Expose the existing source-reader and camera-decoder stages on the shared timeline. Plane-compressed formats must identify each physical sensor layer and its dimensions and compressed input size. Classic interleaved data keeps its existing row-group work instead of inventing independent plane operations.

- [x] Add opt-in file, sensor preparation, per-layer decoding, assembly and camera-metadata scopes.
- [x] Preserve scope attributes in the generic hierarchy report.
- [x] Verify actual application captures across camera families, with uncontended Off comparisons and exact sample checks.
- [x] Complete workspace and documentation checks.

Four loss-free real-app captures cover twelve original decodes across Merrill, Quattro and Classic. Per-layer scopes retain stored index, physical layer, geometry, sample count, compressed bytes and format. On three OS-warm traced runs, the Merrill sample's top/middle/bottom planes take 106.66/105.71/104.72 ms median for 16.08 million samples each. The Quattro sample takes 187.84/49.19/47.33 ms; its top plane contains 23.03 million stored samples, versus 5.41 million in each other plane. The top plane includes the format's additional sensor columns. These parallel intervals are elapsed observations and must not be summed. Classic format 6 exposes its independent interleaved row groups instead of synthetic per-layer intervals.

A real-original check compares every decoded sample, camera metadata and storage layout with recording Off and Wall for Classic, Merrill, Quattro and Quattro H. Forty photographic Full/Fast/Draft/Thumb outputs also remain bit-exact against the existing goldens. Five matched Off trials measured three-photo initial selection at 1120.84 ms before and 1131.08 ms after instrumentation, and 25 retained edits at 53.52 and 54.65 ms median. The scenario's 20 ms observation cadence and limited sample count prevent treating these as precise overhead measurements. All declared behavior and workload checks pass.

## Future concurrent rendering experiments

The owner wants to investigate advancing N Draft/Thumb photographs across M graph nodes concurrently. Establish diagnostics, behavioral tests and serial baselines first. This plan does not add a second scheduler or a stage-major barrier.

Represent each photograph's node executions independently, associated with the actual CPU worker and GPU submission. Distinguish simultaneous CPU preparation, interleaved submissions and measured GPU execution overlap. Enqueuing more work is not proof of device overlap or reduced total work.

Reuse the same fixture/recipe manifests and behavioral assertions for current photo-major execution and later bounded-concurrent candidates. Identify each current result by photo/task/revision, independently of completion order. Compare first-current latency, group completion, throughput, per-photo fairness, UI responsiveness, superseded work, retained memory and peak in-flight resources. Keep scheduling-policy tests separate from these shared correctness checks.

- [x] Record serial native Thumb and headless multi-photo Draft baseline artifacts before any concurrent renderer experiment.
- [x] Verify existing two-worker headless sessions preserve independent task contexts, flow queries and exact fresh outputs. This checks attribution, not a new reduced-photo scheduler.
- [x] Add matched-workload Off comparisons with caller-supplied median/p95 regression budgets. Baseline and candidate input/settings/hardware keys must match, every trial must be complete and uncontended, and no default timing threshold is invented.

## Release-workflow baseline follow-up

The finishing unit added a complete portable fresh-processing baseline for 144 maintained recipe/grid/policy cases. The remaining release requirement is to connect comparable retained-edit, native-detail, multi-photo and UI-delivery baselines using the existing scenario/checkpoint reports. Preserve source loading, foreground delivery, background completion and UI-update gaps as separate measurements. This linkage was deferred from the completed finishing optimization unit; fresh-engine results alone do not close it.
