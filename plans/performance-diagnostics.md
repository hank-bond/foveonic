---
summary: "Design and review a shared trace foundation and repeatable application scenarios before continuing renderer optimization."
---

# Application performance diagnostics

Implements the accepted [diagnostic requirements](../docs/specs/performance-diagnostics.md). The owner requested a sustainable profiling foundation before further render optimization. This plan proposes the implementation approach for review. It does not authorize a renderer, scheduling or cache-policy redesign.

## Existing coverage and gaps

The processor's `Profile` already measures host stages, executed/cached graph outputs, publication, completion waits, retirement, GPU passes and engine activity. The GUI's `timing` records selection, worker lifecycle, image delivery and drawing. Resource inventories and native/headless scenario drivers already exist. Preserve these capabilities.

The recordings currently have separate clock origins, partial cross-thread identity and synchronous JSON output. Native image-receipt events omit a common timestamp. Coordinator coalescing, cache eviction reasons and source access are not connected into one causal timeline. The headless timeline exporter uses synthetic placement between independently timed groups. Detailed native draw reporting can overwhelm other events. New profiling should replace these recording paths incrementally instead of adding more independent timers.

The owner's cached-selection investigation reproduced eight local disk hits and no cache misses, yet initial Reference/Thumb processing took 3.60 seconds with profiling Off. The following eight-photo exposure edit took 45 ms in the same headless worker. Separate Wall and GPU diagnostics identified substantial sensor preparation and GPU stage work on initial selection. These figures exclude native interaction and drawing and do not establish a general latency target. They motivate separate cache-state scenarios and complete request attribution.

## Proposed architecture

Use one thin diagnostic API in the processor's existing profiling area. It owns shared clock/context conventions, stable stage/category names and typed task/cache/resource annotations. Integrate an established trace recorder and viewer. Do not build a profiler UI, distributed telemetry service or new execution runtime.

Recommend Perfetto as the recorder, viewer and query engine. Its interactive timeline serves exploratory use, while Trace Processor queries serve automated reports. Evaluate its in-process Rust SDK first. It is community maintained, so validate macOS builds, capture overhead and supported events before broad migration. Rust `tracing` remains a candidate for ordinary scoped CPU instrumentation. Its current Perfetto layer does not translate task dependencies into flows, so explicit helpers must emit task links, counters and GPU observations. A generic Chrome JSON exporter alone does not satisfy this contract. Avoid maintaining two collectors or instrumenting once per backend.

Reuse the coordinator's task/stamp and image identities. Trace asynchronous job lifetime separately from thread execution scopes. Connecting an interaction to an already cached or coalesced task must not create fictitious execution. Named categories cover UI, coordination, cache/I/O, CPU processing, graph execution, GPU submission and presentation. Optional counters describe queue depth, cache residency and resource pressure.

Instrument at existing ownership boundaries: input dispatch, coordinator transitions, mailbox send/receive, cache lookup/admission/eviction, actual source reads, CPU-pool tasks, graph planning/execution, shader creation, GPU submission/completion and image receipt/use. Graph operations acquire their scopes in the executor, so adding a node does not require its own tracing protocol. Cache decisions belong to cache owners, independently of the browser badge.

Keep CPU and GPU timelines honest. Preserve GPU timestamp intervals and submission IDs, resolve queries after normal completion, and explicitly record unavailable clock calibration. Do not move GPU work to invented CPU positions or add completion waits for diagnostic convenience. Record frame encoding/submission as observable milestones, with physical presentation separately measured when needed.

## Scripted and manual entry points

Both entry points use one recorder and analysis model. Extend the existing native smoke/scenario driver and pointer tools to launch the actual app, execute named actions and attach assertions/results to the capture. Route scripted input through the real input/command path when that is the behavior under test, and label direct method-level checks accordingly. Scenario setup controls the intended cache conditions without assuming that a green indicator proves a hit.

For manual use, offer capture start/stop/save while the app stays open. A bounded recording can include an explicit user marker for a slow interaction. Saving produces the trace, a compact machine-readable summary and its manifest. Inspection uses Perfetto's timeline, zoom, thread/task lanes and counter tracks rather than a new embedded profiler UI. A first version needs explicit capture control, not continuous recording by default or a new general UI automation framework.

## Implementation slices for review

- [ ] Prove the recording backend with one interaction-to-cache-to-render-to-frame trace and a headless query. Measure disabled and enabled overhead and capture limits.
- [ ] Migrate existing GUI/processor records to the shared context and clock. Remove replaced synchronous JSON emitters.
- [ ] Add coordinator outcomes, cache reasons and actual I/O provenance at their single owners. Extend existing resource counters.
- [ ] Connect graph/GPU observations and shader creation without introducing synchronization.
- [ ] Extend the maintained native and headless drivers with named cache-state scenarios, repeatable inputs and comparable reports.
- [ ] Add manual capture start/stop/save with the same artifacts and summaries as scripted runs.
- [ ] Add deterministic causal/work-count checks and separate controlled-machine latency comparisons.

The first complete slice must answer whether a cached eight-photo selection read original RAW payloads, which processing state was reused or rebuilt, how long it waited for execution, what each CPU/GPU stage cost, and when current results reached the native drawing path. New phases extend this same model.

## Future concurrent rendering experiments

The owner wants to investigate advancing N Draft/Thumb photographs across their M graph nodes concurrently instead of completing every photograph serially. That scheduler departure is a later experiment. Establish diagnostics, behavioral tests and serial baselines first. Do not add a second scheduler or force a stage-major barrier as part of instrumentation.

Represent each photograph's node executions and dependencies independently, associated with the CPU worker and GPU submission that performed the work. This must distinguish simultaneous CPU work, interleaved GPU submissions and measured GPU overlap. Enqueuing more work is not proof of concurrent device execution or reduced total work.

Use the same named scenarios for current photo-major execution and later bounded-concurrent or stage-grouped candidates. Vary photo count within the configured edit limit, quality, camera families, recipes and cache conditions. Compare first-current-result latency, full-group completion, throughput, per-photo fairness, UI responsiveness, superseded work, retained memory and peak in-flight resources. Include an edit arriving during a group, removal/re-addition and memory pressure. Reuse existing numerical comparisons outside timed intervals.

Shared behavioral assertions identify results by photo/task/revision rather than arrival order. They check dependency readiness, latest-result admission, cache isolation and resource release. Exact scheduling order belongs only to tests of a specific scheduling policy. Preserve the serial baseline and its manifest so a faster candidate must demonstrate correct output and resource behavior as well as shorter latency.

- [ ] Record serial Draft/Thumb baselines and scheduling-independent behavioral scenarios before any concurrent renderer experiment.
- [ ] Verify that trace queries represent interleaved independent tasks without requiring photo-major completion or introducing synchronization.

## Regression strategy

Ordinary tests use known clocks/fake scheduling where appropriate to verify span/context ownership, coalesced and cancelled outcomes, stale completion rejection and trace completeness. Scenario checks assert effective scale, cache hit/miss and original-read counts, numerical results and final settlement. These assertions catch unnecessary work without depending on machine speed.

Controlled performance runs compare repeated matched trials. Keep warm disk with cold processing distinct from a retained processed image, use randomized switching and memory-pressure cases, and report median/tail latency, CPU/GPU breakdown, queue delay, memory and operation counts. Establish thresholds from observed variability. Keep detailed GPU diagnostics and fresh-output verification outside baseline timing. Store the manifest and capture with failures so a regression can be explained rather than only reported as slow.

## External approaches

[Blender's Tracy integration](https://developer.blender.org/docs/handbook/tooling/tracy/) uses execution zones to investigate intermittent frame stalls. [Unreal Timing Insights](https://dev.epicgames.com/documentation/en-us/unreal-engine/timing-insights-in-unreal-engine) combines CPU/GPU, task and counter tracks. [Perfetto track events](https://perfetto.dev/docs/instrumentation/track-events) supply slices, flows and counters, while its [conversion/query workflow](https://perfetto.dev/docs/getting-started/converting) supports machine analysis. [Rust tracing](https://github.com/tokio-rs/tracing) supplies structured scopes/context without requiring Tokio. The [profiling facade](https://github.com/aclysma/profiling) offers small zone instrumentation with selectable established backends. These are design inputs, not dependencies selected by this plan.

The [Perfetto Rust SDK guide](https://perfetto.dev/docs/getting-started/rust-sdk) documents self-contained capture, function scopes and GPU packet extensions. The current [tracing layer source](https://github.com/google/perfetto/blob/main/contrib/rust-sdk/tracing-perfetto/src/lib.rs) covers scoped slices and instants. Explicit cross-thread flows remain part of the integration spike, rather than being assumed from parent-span annotations.
