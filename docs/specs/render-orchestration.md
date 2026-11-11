---
summary: "Render runtime goals clarify application ownership, responsive interaction and work coordination while preserving serial engine execution."
kind: reference
status: draft
implementation: incomplete
---

# Render orchestration

The owner agreed the goals below and the request/job/result/handler vocabulary. Concrete boundaries and APIs remain provisional, so the overall specification remains draft until the relevant interfaces are reviewed. This revision replaces the earlier detailed type sketch. It does not authorize a full runtime rewrite. The [migration plan](../../plans/render-runtime.md) records one current review unit, evidence and continuation instructions. The [implemented map](../interactive-rendering-architecture.md) describes today's code.

## Agreed goals

The primary workflow is getting through 100 photographs quickly: selecting groups, culling and making useful edits. Engineering work should reduce unnecessary processing, coordination, scheduling, source access and cache latency. It should prepare likely useful data ahead of demand where foreground responsiveness and memory permit. Individual-node arithmetic optimization is not the priority for this migration.

Individual editing and multi-photo editing must use the same runtime. One editing target is the one-element case of the same requested-set contract. Each photograph keeps its own source, settings, dependencies, caches and result identity. Existing [selection and relative-edit behavior](./multi-edit.md) remains authoritative.

This migration improves application ownership and responsibilities while retaining serial engine execution. Wide rendering is a separate future architectural change. A clear application/engine boundary should let that effort experiment with execution order without rewriting editing state, scheduling or delivery. Do not build engine advancement or batching machinery as part of this migration.

The architecture must have few understandable concepts, clear responsibility boundaries and one owner for each decision. Separate operation implementation from the control flow deciding when it runs. Remove repeated policy from callers rather than adding a central facade over unchanged scattered decisions.

Make one conceptual improvement at a time. Before each unit, identify the concepts, define their meanings and owners, map existing terms and review the proposed names with the owner. Apply that vocabulary consistently across the affected contracts, callers, tests and documentation. Implement a complete usable slice, then revise the next step from what was learned. High-level interfaces may change substantially over this effort. That is not a reason to change all of them in one step.

The UI thread handles interaction and display. It must not wait for source/cache I/O, decoding, processing, GPU completion, background planning, resource scans, worker joins or queue capacity. Keep publication and application of updates bounded and nonblocking. Cache/resource availability and scheduling must progress without a UI paint.

## Provisional responsibility boundaries

| Boundary | Responsibility | Excludes |
| --- | --- | --- |
| Editing state and UI | Author requested edits and selections, publish immutable request/context updates, apply images/status and draw. | Scheduling, cache admission, payload I/O and processing. |
| Render runtime | Own wanted results and their lifecycle, choose useful work, reconcile completion, foreground progress and speculative plans off the UI. | Numerical dependency validity, photographic node rules and direct widget mutation. |
| Processing engine | Author/execute recipes and reuse valid intermediates through the existing serial execution contract. | User selection, persistence policy and current-view acceptance. |
| Resource owners | Acquire sources and own disk bundles, processing sessions/checkpoints and presentation resources with explicit lifetime/accounting. | A second registry of requested edits or a universal cache-validity solver. |

These are responsibilities, not four mandatory new objects or threads. Planning, request resolution and completion policy can be functions within one runtime owner. Reuse existing workers and state where they fit. Keep the engine graph inside the engine. The application runtime chooses wanted outputs, priority and admission without reconstructing a second node DAG.

The UI remains the writer for interaction-authored state. Requested values must have one authoritative home, with persistence and rendering consuming immutable snapshots. Results may supply metadata and computed values but must not overwrite newer authored edits. Choose the concrete existing owner in its own migration slice.

## Communication and completion

Publish the latest immutable requested set and context. Share unchanged recipes/data rather than copying every browser photograph on a slider event. Superseded replaceable requests need not accumulate in a queue. A group revision must not invalidate unchanged per-photo work merely because another member changed.

Availability and resource owners notify runtime coordination directly. The runtime combines current requests with their summaries to plan foreground and speculative work. UI drawing is not a planning signal. Background planning must not independently reread recipes or promote cache recency merely to discover availability.

Return immutable per-photo updates. One photo's update must not overwrite an undelivered sibling. Completion processing distinguishes exact request settlement, reusable data, metadata feedback and view admission. A late result cannot settle newer work or replace an incompatible image. The UI retains a cheap final context/order guard because user intent can change after runtime admission. Admission and UI acknowledgement remain distinct.

The implemented runtime runs the render coordinator on its own thread. It handles typed UI commands and worker results and publishes replaceable work plans. View context is an immutable UI snapshot. The UI rechecks incoming results against its latest edits and view, then applies permitted photo/view updates and acknowledges Active delivery. Each image queue has capacity two, with one result held by the coordinator during forwarding. The coordinator must continue servicing commands when UI delivery is full. Validation is recorded in the [migration plan](../../plans/render-runtime.md).

Foreground progress ends when its requested set settles, independent of useful background activity. Failed or removed targets have explicit outcomes. Keep frozen exports, writes and required tool/error acknowledgements distinct from replaceable preview updates so explicit commands cannot disappear through coalescing.

Use ordinary typed messages and explicitly wired handlers for the known consumers. The coordinator can update its state and call foreground and background planning handlers with the same current state. Handler calls stay short and describe work for executors rather than performing I/O or rendering. Shared state updates can inform both handlers without independent subscriber queues. Job distribution and result routing keep their own delivery contracts.

Use existing library channels across thread boundaries. Introduce independent subscriptions only when a consumer needs its own execution or subscription lifetime. If full broadcast/watch behavior becomes necessary, use an established library rather than implementing subscriber registration, concurrent fan-out, wakeup races or channel lifecycle as custom application infrastructure. The application owns message meanings and chooses capacity, lag/backpressure and shutdown policy.

Keep existing Rust threads and the shared Rayon CPU pool. Crossbeam remains a possible job/message queue, not the selected broadcast primitive. Channel selection is open and does not require adopting a new async runtime. UI-facing sends must not block on queue capacity, and publication must be checked for synchronization stalls. Locks protect publication or short bookkeeping only, never I/O, numerical work, GPU waits, blocking sends or joins. A generic actor framework or scheduler-level graph is not required.

As each unit is implemented, record which application glue was added and which ready-made primitive could provide it. Prefer the simpler fit for the actual contract. Ordinary typed dispatch does not justify a subscription framework, while reimplementing a concurrent channel primitive should trigger library review.

## Foreground and speculative execution

Speculation must not hold foreground work behind a background queue. On foreground demand, stop admitting competing speculative work and yield or cancel it at safe boundaries. Still-useful work may retry using retained valid data. A dependency already being prepared speculatively inherits its foreground consumer's priority instead of being restarted or left blocked behind that consumer.

Apply priority at the resource being contested. Independent CPU/source preparation may continue while the foreground uses the GPU if capacity and memory permit. Shared CPU capacity and simultaneous source-buffer admission are separate bounds. Lower host thread priority alone does not establish GPU priority.

Already-submitted GPU operations cannot generally be cancelled or preempted by this application. Bound speculative submitted work and measure the foreground interruption delay. The intent is no avoidable interference, not an unsupported claim that idle work has zero physical cost. Choose a measured submission/completion policy that limits residual blocking and temporary memory.

Cancellation means work is obsolete. Yielding means useful work gives a needed resource to higher priority. Neither should implicitly choose GPU completion behavior. Make required synchronization explicit at execution boundaries and preserve immutable storage until submitted readers/writers retire.

## Reuse and resource ownership

Keep numerical reuse with the engine's exact dependency identities and current source validity. The runtime may coalesce identical requested work without inventing semantic equivalence across recipes. A changed request does not imply a full rerender. Unchanged retained selection must perform no source payload reads or photographic processing, as required by [diagnostics](./performance-diagnostics.md).

Disk and memory caching remain separate. Existing cache owners report representation-specific availability and resource usage. Speculative access must remain distinct from actual-use recency. Bound retained and in-flight data through existing ownership, with measured temporary headroom. Do not add a universal cache manager to consolidate application coordination.

Preserve per-photo dependencies, cache validity, errors and settlement. Application coordination should depend on the requested outputs and engine contract, not the engine's internal graph traversal order. Keep resource lifetimes explicit without redesigning sessions for concurrent execution in this migration.

The future loupe and focus/eye analysis are motivating consumers, not features to implement here. Their requests should eventually fit these same request, priority and resource boundaries without separate orchestration systems.

## Validation and evolution

Keep rendering bit-exact on three images per architecture, with the one available Tiger sample as the explicit exception. Cover authoritative Full and relevant reduced grids with frozen inputs/settings and a repeatable unchanged baseline. New scheduling must also preserve result identity, work avoidance, source replacement, supersession, per-photo fairness, progress settlement and memory lifetime.

Use maintained real-app scenarios and Perfetto attribution alongside ordinary Off latency comparisons. Measure first useful result, complete-current-set latency, useful throughput, UI response under worker load, speculative interruption, unnecessary work, submissions/completion boundaries and peak in-flight resources. Validate mixed recipes, cache hits/misses and individual as well as multi-photo requests under serial engine execution. Detailed evidence and per-slice gates live in the [migration plan](../../plans/render-runtime.md).

The [architecture guide](../architecture/AGENTS.md) separates principles, vocabulary, implemented facts and open questions. Fill it in as slices land. A successful slice reduces dispersed decisions and improves the ability to reason about ownership. Measure any claimed coordination/latency improvement separately. The [deferred wide-rendering investigation](../../plans/wide-rendering.md) retains its hypotheses and evidence without becoming part of this migration's acceptance.
