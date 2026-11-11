---
summary: "Shared application diagnostics connect interaction, scheduling, caching, CPU/GPU work and delivery to reproducible correctness and performance checks."
kind: reference
status: accepted
implementation: incomplete
---

# Performance diagnostics

## Purpose

The application must support empirical attribution of runtime across UI interaction, scheduling, source access, caching, processing and presentation. Diagnostics must support both exploratory profiling and repeatable correctness and performance-regression checks. Renderer experiments must use validated capture and correctness checks.

## Measurement contract

Measurements must distinguish elapsed latency, CPU execution scopes, queue residence, explicit waits, GPU execution, resource retention and delivery. Host elapsed time must not be described as CPU-active time or inferred GPU execution. Parallel and nested intervals must remain distinguishable rather than being added into misleading totals. CPU work must use a common monotonic timeline. GPU timestamps must retain their clock provenance and associate with submissions without inventing a calibrated host position.

An interaction must be traceable through its desired render targets, coalescing or replacement, execution, cache decisions, completion admission and the frame that uses its delivered image. Reuse existing task, photo, purpose, revision and artifact identities. Background prefetch, scopes, navigator and exports must remain distinguishable from foreground work. Cancelled, superseded and failed work must have explicit outcomes.

The diagnostic model must support future concurrent Draft/Thumb execution across photographs and graph nodes without assuming one photograph finishes before another begins. Preserve per-photo node dependencies and distinguish CPU preparation overlap, interleaved submissions and actual GPU execution overlap. Report first-current-result latency, complete-group latency, throughput, fairness, obsolete work and peak in-flight resources. Diagnostics must observe scheduling independently of its implementation and must not introduce execution barriers.

Cache diagnostics must distinguish local decoded data, corrected/reduced processing inputs, graph checkpoints, displayed images and browser previews. They must report hits, misses, invalidations, admission and eviction with reasons and resource sizes. Actual source payload reads must identify original versus local storage independently of readiness indicators. Recording a cache observation must not change recency or load a payload merely to inspect it.

## Instrumentation and capture

Instrumentation must have one shared ownership model and reusable scoped APIs. Common execution boundaries should supply coverage automatically. Routine profiling must not require scattered formatted log statements or per-pixel/per-element events. Existing stage measurements and GPU activity counters should migrate into the shared model instead of establishing a parallel profiling system. The viewer and machine reports should preserve scoped application call hierarchy and actual thread ownership. Uninstrumented function internals may require a separate sampling profiler.

Captures must be bounded and locally inspectable, with machine-readable query results. Record capture configuration, build and input identities, processing settings, effective scale, hardware and intended cache state. Overflow and missing observations must be explicit. Ring-buffer recordings must periodically refresh incremental metadata so retained recent events remain decodable after older packets are overwritten. Disabled instrumentation must avoid formatting and recording work. Enabled capture must not block the UI or rendering on output, or add GPU waits merely to obtain timestamps. Measure capture overhead separately.

Scripted scenarios must run the real application and produce the same capture format as manual use, with scenario steps, expected outcomes and performance results attached. Existing native smoke and pointer scripting should be reused where they exercise the intended path. Distinguish direct application-method actions from physical/input-dispatch coverage. A user must also be able to record ordinary app use and save a locally readable capture and summary on demand without closing the application. The viewer should provide an interactive timeline with thread/task lanes, nested timings, counters and interval inspection comparable to a browser performance view.

Behavioral checks must cover applicable editing controls and user journeys across the supported camera families and available RAW sizes. For every applicable slider, exercise a sustained multi-photo drag with differing starting values while rendering progresses. Check the shared numeric display policy, stable panel and control geometry, continued pointer response, relative per-photo edits and eventual delivery of the final requested state. Record unavailable and untested controls explicitly. Use actual widget input for this coverage and keep direct-command checks separately identified. Capture enough structured state and failure artifacts to identify the control, input sequence, photo settings, displayed values and geometry. Preserve replayable actions and seeds when generated sequences are used.

Every explicitly started capture must save on orderly application exit, including one still starting. A configured capture directory is used when supplied. Otherwise the recording is saved under the platform temporary directory in `foveonic-captures/`. Closing during an already-requested save must finish that save rather than create a second capture. Shutdown may wait for recorder completion after interaction has ended. Ordinary unrecorded launches must not create capture output. Forced termination is outside this orderly-exit guarantee.

## Validation

Correctness checks must validate causality, accepted output identity, complete task settlement, effective quality and expected work/reuse independently of timing thresholds. Performance scenarios must separately cover initial loading, warm disk with cold processing state, retained processing, editing, switching, memory pressure, rapid supersession and multi-photo delivery.

Unchanged cached selection and same-photo reopening must perform zero original RAW payload reads, decoded-cache payload reads and photographic GPU dispatches for retained photographs. Check these work counts independently of accepted result identity so discarded work cannot hide a regression. Existing task purposes distinguish photographic rendering from auxiliary scopes, navigator and presentation work. Unrelated background prefetch reads must remain visible separately.

Shared behavioral scenarios must remain usable when independent photographs finish in different orders. Compare each delivered photo and revision with its expected numerical result and validate dependencies, cache ownership, cancellation, freshness, final settlement and resource policy. Keep implementation-specific scheduling-policy tests distinct. Capture the current serial execution baseline before experimenting with concurrent or stage-grouped execution.

Expected user-visible behavior must be derived independently of the application state being checked. Permit compatible intermediate previews and any valid completion order, then verify the latest desired state after foreground work settles. Keep unit, headless numerical and headed behavioral evidence distinct. Correctness checks must not depend on latency thresholds; matched unrecorded runs provide separate latency baselines, and traced runs provide attribution and failure evidence.

Headless and real-app checks should report common observations and assertion outcomes in one test-result shape where their evidence describes the same behavior. Reuse those assertions without making an engine-only run claim user-interface coverage. Each driver retains its own detailed evidence and identifies how it exercised the application.

Performance comparisons must use matched builds, settings, inputs, hardware and cache conditions, with repeated samples and reported variability. Timing budgets belong to controlled performance runs rather than ordinary portable unit tests. Detailed attribution runs must remain separate from baseline latency measurements. Profiling launchers must check for other application, compiler and measurement processes before starting. They should record concurrent workloads during each run and mark contaminated timings separately from behavioral correctness. Native frame submission is not proof of physical screen presentation. Sampling profilers and platform tools may complement application instrumentation for unmarked internals and system scheduling.
