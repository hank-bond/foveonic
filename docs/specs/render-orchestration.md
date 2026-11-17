---
summary: "Render runtime goals clarify application ownership, responsive interaction and work coordination while preserving serial engine execution."
kind: reference
status: draft
implementation: incomplete
---

# Render orchestration

The owner agreed the goals below and the request/job/result/handler vocabulary. Concrete boundaries and APIs remain provisional, so the overall specification remains draft until the relevant interfaces are reviewed. This revision replaces the earlier detailed type sketch. It does not authorize a full runtime rewrite. The [migration plan](../../plans/render-runtime.md) records one current review unit, evidence and continuation instructions. The [implemented map](../interactive-rendering-architecture.md) describes today's code.

## Agreed goals

The owner identified three reasons for this rearchitecture. Use them together when reviewing each slice:

1. **Maintainability:** replace the reactive accumulation of feature-specific coordination with clear ownership and boundaries that make behavior stable and new features easier to develop.
2. **Latency:** reduce unnecessary work, round trips, synchronization and waiting between stages. State performance benefits as hypotheses until matched measurements demonstrate them.
3. **Future execution flexibility:** let a later experiment advance several photographs through each pipeline stage before moving to the next stage. Wide rendering may improve throughput or latency, but that benefit must be tested. This migration should make the experiment easier without implementing it now.

The primary workflow is getting through 100 photographs quickly: selecting groups, culling and making useful edits. Engineering work should reduce unnecessary processing, coordination, scheduling, source access and cache latency. It should prepare likely useful data ahead of demand where foreground responsiveness and memory permit. Individual-node arithmetic optimization is not the priority for this migration.

Individual editing and multi-photo editing must use the same runtime. One editing target is the one-element case of the same requested-set contract. Each photograph keeps its own source, settings, dependencies, caches and result identity. Existing [selection and relative-edit behavior](./multi-edit.md) remains authoritative. Ordinary Current-photo submissions must publish the complete ready desired set, including unchanged pending siblings. Publication is coherent across the set, while source readiness, execution and delivery remain independent per photograph. The active photo must wait for its selected recipe to be ready without blocking ready siblings. Active Before, inspection, exports and progressive Smart detail requests keep their own meanings.

This migration improves application ownership and responsibilities while retaining serial engine execution. Wide rendering is a separate future architectural change. A clear application/engine boundary should let that effort experiment with execution order without rewriting editing state, scheduling or delivery. Do not build engine advancement or batching machinery as part of this migration.

The separately authorized [wide-rendering experiment](../../plans/archived/wide-rendering-integration.md) now evaluates that change. Its direction is one shared execution engine at width one through N for all workloads, with concurrency chosen automatically from the memory budget. Its disposable prototype does not broaden this earlier migration's scope or establish production admission policy.

The approved next experiment extends that same execution lifecycle to retained Merrill native regions, beginning at widths one and two. It must preserve native overlay geometry, per-photo source/cache identity, independent completion, viewport priority and same-photo execution exclusivity. Compare exact rendered output separately from first-result/group latency, preparation cost and process memory. Cold acquisition and production memory admission remain separate from this warm speed experiment.

For one shared edit applied to several zoomed photographs, the primary latency metric is complete-set update: when every requested native patch reflects the new settings. First-image completion remains diagnostic rather than the deciding acceptance metric for that workload. Single-photo pan and zoom responsiveness have their own latency target. Simultaneous panning across photographs is a synthetic cohort until linked viewing is explicitly implemented, so it must not substitute for shared-edit latency evidence.

Width one must use the same photographic execution lifecycle as wider groups. Migrate callers away from superseded production drivers and remove their unused implementations. Source acquisition, cache sealing, named outputs and file writing may retain their distinct semantics without becoming parallel photographic renderers. Convenient synchronous entry points may remain as adapters to the common lifecycle. Completion policy must remain independent of cancellation so interactive responsiveness and fresh/export throughput can be preserved without separate render loops.

The architecture must have few understandable concepts, clear responsibility boundaries and one owner for each decision. Separate operation implementation from the control flow deciding when it runs. Remove repeated policy from callers rather than adding a central facade over unchanged scattered decisions.

Make one conceptual improvement at a time. Before each unit, identify the concepts, define their meanings and owners, map existing terms and review the proposed names with the owner. Apply that vocabulary consistently across the affected contracts, callers, tests and documentation. Implement a complete usable slice, then revise the next step from what was learned. High-level interfaces may change substantially over this effort. That is not a reason to change all of them in one step.

The UI thread handles interaction and display. It must not wait for source/cache I/O, decoding, processing, GPU completion, background planning, resource scans, worker joins or queue capacity. Keep publication and application of updates bounded and nonblocking. Cache/resource availability and scheduling must progress without a UI paint.

## Provisional responsibility boundaries

| Boundary | Responsibility | Excludes |
| --- | --- | --- |
| Editing state and UI | Author requested edits and selections, publish immutable request/context updates, apply images/status and draw. | Scheduling, cache admission, payload I/O and processing. |
| Render runtime | Own wanted results and their lifecycle, choose useful work, reconcile completion, foreground progress and speculative plans off the UI. | Numerical dependency validity, photographic node rules and direct widget mutation. |
| Processing engine | Author/execute recipes and reuse valid intermediates through the shared graph execution contract. | User selection, persistence policy and current-view acceptance. |
| Resource owners | Acquire sources and own disk bundles, processing sessions/checkpoints and presentation resources with explicit lifetime/accounting. | A second registry of requested edits or a universal cache-validity solver. |

These are responsibilities, not four mandatory new objects or threads. Planning, request resolution and completion policy can be functions within one runtime owner. Reuse existing workers and state where they fit. Keep the engine graph inside the engine. The application runtime chooses wanted outputs, priority and admission without reconstructing a second node DAG.

The UI remains the writer for interaction-authored state. Requested values must have one authoritative home, with persistence and rendering consuming immutable snapshots. Results may supply metadata and computed values but must not overwrite newer authored edits. Choose the concrete existing owner in its own migration slice.

## Communication and completion

Publish the latest immutable requested set and context. Share unchanged recipes/data rather than copying every browser photograph on a slider event. Superseded replaceable requests need not accumulate in a queue. A group revision must not invalidate unchanged per-photo work merely because another member changed.

Availability and resource owners notify runtime coordination directly. The runtime combines current requests with their summaries to plan foreground and speculative work. UI drawing is not a planning signal. Background planning must not independently reread recipes or promote cache recency merely to discover availability.

Requested source loading must not occupy the serial renderer while another photo has reusable processing inputs. Check presentation and session reuse before loading. Coalesce acquisition independently of changing edits, bound decoded sources waiting for consumption, and report loading failure per photo through normal result handling. Deferring a render for input does not settle or cancel its request. An obsolete physical read may finish, but its unwanted result must not displace current work.

Return immutable per-photo updates. One photo's update must not overwrite an undelivered sibling. Completion processing distinguishes exact request settlement, reusable data, metadata feedback and view admission. A late result cannot settle newer work or replace an incompatible image. The UI retains a cheap final context/order guard because user intent can change after runtime admission. Admission and UI acknowledgement remain distinct.

The implemented runtime runs the render coordinator on its own thread. It handles typed UI commands and worker results and publishes replaceable work plans. View context is an immutable UI snapshot. The UI rechecks incoming results against its latest edits and view, then applies permitted photo/view updates and acknowledges Active delivery. Each image queue has capacity two, with one result held by the coordinator during forwarding. The coordinator must continue servicing commands when UI delivery is full. Validation is recorded in the [migration plan](../../plans/render-runtime.md).

Foreground progress ends when its requested set settles, independent of useful background activity. Failed or removed targets have explicit outcomes. Keep frozen exports, writes and required tool/error acknowledgements distinct from replaceable preview updates so explicit commands cannot disappear through coalescing.

Use ordinary typed messages and explicitly wired handlers for the known consumers. The coordinator can update its state and call foreground and background planning handlers with the same current state. Handler calls stay short and describe work for executors rather than performing I/O or rendering. Shared state updates can inform both handlers without independent subscriber queues. Job distribution and result routing keep their own delivery contracts.

Use existing library channels across thread boundaries. Introduce independent subscriptions only when a consumer needs its own execution or subscription lifetime. If full broadcast/watch behavior becomes necessary, use an established library rather than implementing subscriber registration, concurrent fan-out, wakeup races or channel lifecycle as custom application infrastructure. The application owns message meanings and chooses capacity, lag/backpressure and shutdown policy.

Keep existing Rust threads and the shared Rayon CPU pool. Crossbeam remains a possible job/message queue, not the selected broadcast primitive. Channel selection is open and does not require adopting a new async runtime. UI-facing sends must not block on queue capacity, and publication must be checked for synchronization stalls. Locks protect publication or short bookkeeping only, never I/O, numerical work, GPU waits, blocking sends or joins. A generic actor framework or scheduler-level graph is not required.

As each unit is implemented, record which application glue was added and which ready-made primitive could provide it. Prefer the simpler fit for the actual contract. Ordinary typed dispatch does not justify a subscription framework, while reimplementing a concurrent channel primitive should trigger library review.

## Foreground and speculative execution

UI-facing render work must have this priority order: viewport images, navigator and scopes, browser thumbnails, then speculative full-image continuations. Viewport work includes all requested editing targets and native detail. Navigator and scopes share one tier. Browser thumbnails must not wait behind a speculative full-image continuation.

Full-image continuation after useful regional delivery is an idle-compute optimization. It may start or advance only when higher-priority UI-facing render work is neither pending nor active. Regional delivery must not depend on continuation, and foreground settlement and progress must exclude it. On new UI-facing demand, continuation must stop admitting work and yield at a safe preparation or graph boundary. Already-submitted GPU work must retire safely, with a bounded submission backlog rather than a complete image queued ahead of interaction.

A persistent user setting must allow background full-image completion to be disabled to avoid its battery cost. Disabling it must prevent new speculative continuations and retire or yield active continuation safely. The setting does not prohibit whole-image processing required to satisfy an explicit viewport request or a recipe without regional support. Compatible completed whole-image results remain reusable.

Fresh reduced whole-photo results needed by navigator or browser thumbnails are UI-facing work, independent of optional full-native completion. Disabling background completion must not suppress those preview refreshes. Compatible previews may share one reduced photographic result while native detail remains independently useful.

Whole-photo scopes may consume that same current preview when a matching finer whole image is unavailable. Viewport scopes should prefer compatible native detail covering their requested region. Scope results must match their selected input artifact and request key. Whole-photo scope sampling at the reduced preview grid does not imply native-resolution histogram equality.

For a regional Current edit, the coordinator must request one reduced whole-photo preview from the same immutable edits, processing quality and calibration as the region. Give it an independent preview target so it can settle without native whole-image completion. Use the camera thumbnail working scale initially, independently of authored output resizing. Retain at most one preview per photo through the existing bounded photo owner. Sharing that result between navigator and browser must not replace a sharper viewport frame, retire compatible native detail, update authored controls or restart viewport progress. Source, edit, quality and calibration changes must reject obsolete preview delivery.

A preview needed by the active navigator has navigator/scope priority. A browser-only preview has browser-thumbnail priority. In the explicitly enabled wide-rendering build, warm reduced previews may execute as a group only within one priority tier. Browser previews must not share submission windows with viewport work or start preparation while higher-tier demand holds the execution gate. New viewport demand, including demand awaiting source input, must stop further preview submissions and cause useful previews to yield and retry after the completed window. Browser groups must likewise yield to navigator and scope demand. Priority interruption must not settle a still-wanted preview or report a photo failure. Cold preview acquisition keeps the existing serial fallback. Automatic memory admission must preserve retained inputs instead of widening preview work. Carry this ordering through source acquisition, preparation and graph execution. Lower-priority source work must yield and retry behind new viewport demand without reporting cancellation as a photograph error. Shared source acquisition remains one job per path, promoted to the strongest current consumer.

Speculation must not hold foreground work behind a background queue. On foreground demand, stop admitting competing speculative work and yield or cancel it at safe boundaries. Still-useful work may retry using retained valid data. A dependency already being prepared speculatively inherits its foreground consumer's priority instead of being restarted or left blocked behind that consumer.

Apply priority at the resource being contested. Independent CPU/source preparation may continue while the foreground uses the GPU if capacity and memory permit. Shared CPU capacity and simultaneous source-buffer admission are separate bounds. Lower host thread priority alone does not establish GPU priority.

Already-submitted GPU operations cannot generally be cancelled or preempted by this application. Bound speculative submitted work and measure the foreground interruption delay. The intent is no avoidable interference, not an unsupported claim that idle work has zero physical cost. Choose a measured submission/completion policy that limits residual blocking and temporary memory.

Cancellation means work is obsolete. Yielding means useful work gives a needed resource to higher priority. Neither should implicitly choose GPU completion behavior. Make required synchronization explicit at execution boundaries and preserve immutable storage until submitted readers/writers retire.

## Reuse and resource ownership

Keep numerical reuse with the engine's exact dependency identities and current source validity. The runtime may coalesce identical requested work without inventing semantic equivalence across recipes. A changed request does not imply a full rerender. Unchanged retained selection must perform no source payload reads or photographic processing, as required by [diagnostics](./performance-diagnostics.md).

Disk and memory caching remain separate. Existing cache owners report representation-specific availability and resource usage. Speculative access must remain distinct from actual-use recency. Bound retained and in-flight data through existing ownership, with measured temporary headroom. Do not add a universal cache manager to consolidate application coordination.

The initial user-facing sizing heuristic for Merrill multi-edit in Speed is approximately 1 GB of render-cache capacity per simultaneously edited photograph. Eight photographs should therefore target an approximately 8 GB memory-cache setting. This is a sizing target to validate across representative edits and zoom regions, rather than an established peak bound or a requirement to retain 1 GB for every image. Account shared cache consumers and temporary work when validating the recommendation. Total system RAM must also accommodate the application outside this cache, the operating system and other applications.

In the explicitly enabled wide-rendering build, choose concurrency automatically from the shared memory target, retained data and estimated workspace. Cache memory is the user-facing resource control. Do not expose a separate concurrency preference. The internal safety ceiling may bound experimental admission, but must not act as a target that displaces useful data. The first required photograph must still progress when essential memory exceeds the soft target.

Additional warm Merrill work must account for distinct frontend and pending images, retained sessions, live executions and completed results awaiting delivery, plus conservative workspace headroom. Shared storage must count once. Native admission also uses the initial 1 GiB-per-photo capacity heuristic. Unknown estimates defer additional work.

Increased concurrency is opportunistic. Admission must not trim retained processing inputs or history to make room for peers. Preserve reusable inputs rather than maximizing simultaneous work. Reconsider live peers at completed GPU windows when current storage no longer fits, before ordinary budget trimming can sacrifice completed-photo reuse. Ready results remain retained and required work must retry and settle. Ordinary cache-budget maintenance remains separate from additional-job admission. This experimental policy is a coarse safeguard, not a hard process-memory limit or a validated recommendation for every region size and quality.

Preserve per-photo dependencies, cache validity, errors and settlement. Application coordination should depend on the requested outputs and engine contract, not the engine's internal graph traversal order. Keep resource lifetimes explicit without redesigning sessions for concurrent execution in this migration.

The future loupe and focus/eye analysis are motivating consumers, not features to implement here. Their requests should eventually fit these same request, priority and resource boundaries without separate orchestration systems.

## Validation and evolution

Keep rendering bit-exact on three images per architecture, with the one available Tiger sample as the explicit exception. Cover authoritative Full and relevant reduced grids with frozen inputs/settings and a repeatable unchanged baseline. New scheduling must also preserve result identity, work avoidance, source replacement, supersession, per-photo fairness, progress settlement and memory lifetime.

Use maintained real-app scenarios and Perfetto attribution alongside ordinary Off latency comparisons. Measure first useful result, complete-current-set latency, useful throughput, UI response under worker load, speculative interruption, unnecessary work, submissions/completion boundaries and peak in-flight resources. Validate mixed recipes, cache hits/misses and individual as well as multi-photo requests under serial engine execution. Detailed evidence and per-slice gates live in the [migration plan](../../plans/render-runtime.md).

The [architecture guide](../architecture/AGENTS.md) separates principles, vocabulary, implemented facts and open questions. Fill it in as slices land. A successful slice reduces dispersed decisions and improves the ability to reason about ownership. Measure any claimed coordination/latency improvement separately. The [deferred wide-rendering investigation](../../plans/archived/wide-rendering.md) retains its hypotheses and evidence without becoming part of this migration's acceptance.
