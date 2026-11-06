---
summary: "Shared application diagnostics connect interaction, scheduling, caching, CPU/GPU work and delivery to reproducible correctness and performance checks."
kind: reference
status: accepted
implementation: incomplete
---

# Performance diagnostics

## Purpose

The application must support empirical attribution of runtime across UI interaction, scheduling, source access, caching, processing and presentation. Diagnostics must support both exploratory profiling and repeatable correctness and performance-regression checks. Renderer optimization is paused while this foundation is designed and reviewed.

## Measurement contract

Measurements must distinguish elapsed latency, CPU execution scopes, queue residence, explicit waits, GPU execution, resource retention and delivery. Host elapsed time must not be described as CPU-active time or inferred GPU execution. Parallel and nested intervals must remain distinguishable rather than being added into misleading totals. CPU work must use a common monotonic timeline. GPU timestamps must retain their clock provenance and associate with submissions without inventing a calibrated host position.

An interaction must be traceable through its desired render targets, coalescing or replacement, execution, cache decisions, completion admission and the frame that uses its delivered image. Reuse existing task, photo, purpose, revision and artifact identities. Background prefetch, scopes, navigator and exports must remain distinguishable from foreground work. Cancelled, superseded and failed work must have explicit outcomes.

The diagnostic model must support future concurrent Draft/Thumb execution across photographs and graph nodes without assuming one photograph finishes before another begins. Preserve per-photo node dependencies and distinguish CPU preparation overlap, interleaved submissions and actual GPU execution overlap. Report first-current-result latency, complete-group latency, throughput, fairness, obsolete work and peak in-flight resources. Diagnostics must observe scheduling independently of its implementation and must not introduce execution barriers.

Cache diagnostics must distinguish local decoded data, corrected/reduced processing inputs, graph checkpoints, displayed images and browser previews. They must report hits, misses, invalidations, admission and eviction with reasons and resource sizes. Actual source payload reads must identify original versus local storage independently of readiness indicators. Recording a cache observation must not change recency or load a payload merely to inspect it.

## Instrumentation and capture

Instrumentation must have one shared ownership model and reusable scoped APIs. Common execution boundaries should supply coverage automatically. Routine profiling must not require scattered formatted log statements or per-pixel/per-element events. Existing stage measurements and GPU activity counters should migrate into the shared model instead of establishing a parallel profiling system.

Captures must be bounded and locally inspectable, with machine-readable query results. Record capture configuration, build and input identities, processing settings, effective scale, hardware and intended cache state. Overflow and missing observations must be explicit. Disabled instrumentation must avoid formatting and recording work. Enabled capture must not block the UI or rendering on output, or add GPU waits merely to obtain timestamps. Measure capture overhead separately.

Scripted scenarios must run the real application and produce the same capture format as manual use, with scenario steps, expected outcomes and performance results attached. Existing native smoke and pointer scripting should be reused where they exercise the intended path. Distinguish direct application-method actions from physical/input-dispatch coverage. A user must also be able to record ordinary app use and save a locally readable capture and summary on demand without closing the application. The viewer should provide an interactive timeline with thread/task lanes, nested timings, counters and interval inspection comparable to a browser performance view.

## Validation

Correctness checks must validate causality, accepted output identity, complete task settlement, effective quality and expected work/reuse independently of timing thresholds. Performance scenarios must separately cover initial loading, warm disk with cold processing state, retained processing, editing, switching, memory pressure, rapid supersession and multi-photo delivery.

Shared behavioral scenarios must remain usable when independent photographs finish in different orders. Compare each delivered photo and revision with its expected numerical result and validate dependencies, cache ownership, cancellation, freshness, final settlement and resource policy. Keep implementation-specific scheduling-policy tests distinct. Capture the current serial execution baseline before experimenting with concurrent or stage-grouped execution.

Performance comparisons must use matched builds, settings, inputs, hardware and cache conditions, with repeated samples and reported variability. Timing budgets belong to controlled performance runs rather than ordinary portable unit tests. Detailed attribution runs must remain separate from baseline latency measurements. Native frame submission is not proof of physical screen presentation. Sampling profilers and platform tools may complement application instrumentation for unmarked internals and system scheduling.
