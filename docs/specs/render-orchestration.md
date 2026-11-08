---
summary: "Proposes single-owner editing state, immutable demand, shared jobs and explicit execution controls around the existing processing engine."
kind: reference
status: draft
implementation: incomplete
---

# Render orchestration

This is the concrete proposal for the boundaries agreed in the architecture discussion. The type names and transition rules below are proposed interfaces, not implemented Rust APIs. Review them before implementation. The [current architecture map](../interactive-rendering-architecture.md) records what the code does today. The [latency plan](../../plans/multi-edit-latency.md) tracks measurements and migration.

The owner subsequently chose to improve correctness, flexibility and latency within the current architecture first. Keep this proposal as a design direction. It does not authorize replacing the current coordinator, worker or channels. Small usable changes receive application smoke testing before discussing a broader migration.

## Scope

Keep the processing engine's node graph, recipes, numerical cache keys and arithmetic intact. The orchestration layer handles application intent, application-level work, execution controls, resource admission and result delivery. It must not reproduce the pipeline DAG or enumerate photographic nodes to schedule work.

Use Rust threads, the existing shared CPU pool, Crossbeam channels and GPU interfaces. Add one dedicated orchestration thread. Keep one interactive image worker, the existing batch worker, and bounded I/O and auxiliary workers. A new asynchronous runtime or generic actor framework is not required.

## Smallest implementation

The owner selected Rayon and Crossbeam as the concurrency primitives. Keep the application-specific coordination small:

- Editing state and scheduling state are the two control-state owners. The background planner, request resolver and completion policy are functions around those owners, not independently mutable managers or threads.
- Use a small keyed job table and the existing service cursor. Keep fixed application work kinds. Engine nodes, arbitrary dependency graphs and a general executor/plugin registry remain inside or beyond the engine boundary.
- Coalesce exact work specifications. Do not build a scheduler-level solver for semantic equivalence between different recipes. The engine already owns finer numerical reuse.
- Keep the existing priority classes and simple limits for source admission, CPU execution, GPU backlog and retained memory. Make those limits resource-specific. Cost prediction, automatic tuning and a general resource-allocation solver are not needed for the first version.
- Use one latest-plan publication mechanism and Crossbeam wakeup/message channels. Avoid maintaining a condition variable and a channel notification for the same change. Use standard `Arc`, short locks and atomic control signals where appropriate.
- On speculative yield, return control and reuse valid engine state when retrying. Stack continuations, arbitrary suspended-node execution and parallel image engines are separate future changes.
- Keep cache ownership with its current data owners. The coordinator consumes summaries and controls admission, rather than maintaining a second copy of graph/cache validity or moving every cache into a new universal cache manager.

These constraints retain shared demand/job identity, foreground promotion, fair service, resource-specific priority and explicit completion policy. Those concepts address current observed needs and should survive the simplification.

## State and planning

`AppState` has one writer on the UI thread. It owns requested per-photo edits, browser and viewport selections, active context and workspace policy. Widget bindings, sidecar persistence and render requests consume or commit through this owner. Completed images never write requested edits back into it.

On a relevant state change, publish an immutable `IntentSnapshot` to the orchestration thread. It describes requested edits, selection/browser context, preview/view intent and user-set policy. Share unchanged recipes and path arrays with `Arc`. Do not copy all browser images or recipes on every slider event. A snapshot revision gives a coherent view of state, not a reason to invalidate every photo's work.

The orchestration thread combines the latest intent with availability and resource summaries from their actual owners to construct `PlanningSnapshot`. Cache and resource updates go directly to orchestration. They must not pass through the UI to trigger planning. The UI receives only the images and status/view models needed for display, including cache indicators and settings-panel summaries.

Foreground demand and the background plan are derived from the same snapshot. The background planner is a small pure function over that snapshot and a read-only availability/resource summary. It does not read files, mutate cache recency, submit GPU commands or resolve sidecars independently. Reconsider its replaceable plan when relevant intent or availability changes, not on every paint or graph operation.

Source loading and completed jobs publish availability updates through their owners. UI drawing results are not the source of planning truth. Camera-relative settings that require metadata are resolved through the shared request/engine adapter once metadata is available, without duplicating camera policy in the planner.

## Concrete data model

| Proposed type | Data and purpose | Single writer |
| --- | --- | --- |
| `AppState` | Photo records, requested recipe revisions, selections and workspace context | UI state owner |
| `IntentSnapshot` | Immutable requested edits and user/view context, sharing unchanged data | Constructed by UI state owner |
| `PlanningSnapshot` | Latest intent combined with source/cache availability and resource summaries | Constructed on orchestration thread |
| `WorkSpec` | Immutable source-acquisition, engine-image or presentation request. Source version, recipe, output, grid/region and calibration are explicit where applicable. | Request resolver |
| `Demand` | Consumer identity, requested revision, work specification, priority and retention/delivery intent | Coordinator |
| `JobRecord` | Execution-attempt ID, work key, attached demand IDs, lifecycle and shared execution control | Coordinator, with service cursor state private to its executor |
| `WorkPlan` | Replaceable immutable set of executable jobs and their controls | Coordinator publishes, executors read |
| `ExecutionControl` | Monotonic cancellation token, current effective priority and shared admission gate | Coordinator changes controls, engine/executors observe |
| `ResourceClass` / admission account | Source I/O, processing CPU and GPU submission capacity, plus bounded in-flight source/transient storage | One shared admission component, with short synchronized permit updates |
| `Completion` | Attempt ID, work key, typed outcome and immutable artifact with actual source/configuration/geometry | Executor produces, coordinator consumes |

The relationship is approximately:

```rust,ignore
struct Demand {
    id: DemandId,
    revision: Revision,
    work: Arc<WorkSpec>,
    consumer: Consumer,
    priority: Priority,
}

struct JobRecord {
    id: JobId,
    key: WorkKey,
    work: Arc<WorkSpec>,
    demands: Vec<DemandId>,
    state: JobState,
    control: ExecutionControl,
}

enum CheckpointDecision { Continue, Yield, Cancel }
enum JobOutcome { Completed(Artifact), Yielded, Cancelled, Failed(JobError) }
```

`Consumer` describes a current display target or cache warming. Frozen exports and writes are separate admitted commands with their own command IDs. Their numeric image requests may share processing resources, but two explicit file commands must not disappear through render deduplication.

`WorkKey` establishes exact request compatibility for coalescing. It uses source version and immutable execution inputs, with equality checked by the owning subsystem. It excludes priority, consumer, UI selection and demand revision. A changed key does not prove that every pipeline operation must rerun. The engine alone decides numerical dependency reuse. Source acquisition verifies identity off the UI thread before the engine uses native input.

Keep the first job table small and bounded by editing targets, prefetch policy and admitted commands. A `Vec<JobRecord>` with an exact-key lookup and a simple service cursor is sufficient initially. A priority heap, pluggable scheduler or second node graph is not justified by current scale.

## Reconciliation and service order

The coordinator is a single control owner on a dedicated orchestration thread. It consumes current intent, worker completions and availability/resource updates, runs planning and completion policy, and publishes work plans and UI view updates. It performs no blocking I/O or numerical processing. Executors retain physical service history across replacement plans through a small cursor, as they do today. They do not create a second desired-work registry.

Reconciliation attaches compatible demand to an existing job, creates missing work, updates effective priority and withdraws obsolete demand. Effective job priority is the highest priority of its remaining consumers. Frozen commands remain until completion or explicit user cancellation where supported.

Use one typed priority ordering shared by participating executors: foreground interaction, browser/presentation assistance, requested batch work and speculation. Preserve fair service across foreground photos, including the existing bounded follow-up rule. Interactive tools can have an explicit service rank within foreground work. These are scheduling policy, not branches inside numerical operations.

Admission checks available executor capacity and resource policy. Cache readiness and budget summaries guide planning, but the resource owner checks current capacity before allocating or retaining data. One actual engine invocation per work key may run at a time. Source acquisition and engine-image requests remain different work specifications. A completed source or prepared dependency is reused through its owner, not treated as a display image.

## Throughput and resource admission

The owner wants one consistent plan that gets useful work ready ahead of demand and feeds both the shared CPU pool and GPU. Central coordination owns demand, ready work and admission policy. Computation remains distributed across the existing bounded executors. The coordinator must not become a processing thread or a global execution lock.

Priority is resource-specific. Foreground GPU work does not by itself require stopping independent speculative CPU decoding or source I/O. Lower-priority work yields when it competes for a needed resource or consumes memory headroom required by foreground work. A queued/running foreground flag that suspends every background resource would serialize useful overlap and is too coarse for this goal.

Use a small `ResourceClass` vocabulary for source I/O, processing CPU and GPU submission. Worker/engine adapters request admission at their existing resource boundaries. They declare resource needs without exposing photographic node names. The shared admission component owns capacity counters and reservations, while the coordinator remains the owner of job demand and priority. A dependency inherits its consumers' effective priority.

The first useful overlap is reading/decoding a bounded number of future sources while the GPU processes current images. Keep prepared inputs ready before the GPU needs them. Bound in-flight photo buffers and submitted GPU work so throughput improvements cannot create excessive memory retention or a long obsolete GPU backlog. CPU thread count, source-photo admission and GPU submission depth are separate settings/policies.

The engine owns command encoding, dependency order and safe completion boundaries. The GPU API accepts recorded commands and the device/driver executes them. More CPU submission threads do not establish more GPU throughput or hardware concurrency. Keep the pipeline opaque and measure queue starvation, CPU preparation ahead of GPU work, submitted backlog, completed throughput and foreground interruption separately.

Optimize completed useful work under the foreground-latency and memory targets. Idle hardware can still be appropriate when dependencies, I/O or capacity leave no useful admissible work. Do not use CPU/GPU percentage alone to claim success. Concurrent execution of several engine images remains a separately measured next step after this orchestration contract is validated.

## Threads and communication

| Execution context | Owns | Communication |
| --- | --- | --- |
| UI thread | `AppState`, interaction/widget state and displayed images | Publish latest intent and apply display/status updates. Never plan background work, reconcile jobs or wait for processing. |
| Orchestration thread | Latest intent/availability summaries, coordinator, background planning and completion disposition | Publish latest work plans and display/status updates. Consume executor completions and resource-owner updates directly. |
| Interactive image worker | One active engine/session set, reduced history and numerical execution | Consume latest work. Serve foreground and idle preparation on the same worker. |
| Existing batch worker | Frozen batch command execution and its engine executor | Share compiled GPU facilities and participate in the same priority/admission controls. |
| Bounded source I/O coordinators and cache writer | Source reads, publication and disk maintenance | Supply verified immutable source/availability results. Preserve current bounded photo admission. |
| Shared processing CPU pool | Independent decoding and other parallel CPU work | Inherit the job's execution control. CPU thread count and simultaneous source buffers remain separate bounds. |
| Auxiliary presentation workers | Bounded navigator/preview work | Use the same priority vocabulary and submission gate, with explicit completion requirements. |

Use the same small latest-value publication helper for intent and work plans: an `Arc` value behind a short `Mutex`, with a bounded Crossbeam change notification. Superseded snapshots/plans are overwritten, not accumulated. A pending notification already means the consumer must read the latest slot. Use bounded Crossbeam channels for executor completions and separately admitted frozen commands. UI publication must be nonblocking. A full frozen-command queue is explicit backpressure, never silent replacement or dropping.

Display/status publication must also avoid blocking orchestration behind a busy UI. Coalesce replaceable image updates per target so one photo cannot overwrite another photo's update. The UI applies a cheap context-generation/order check before drawing an update, since user intent can change after orchestration classifies a completion. It does not repeat scheduling, cache-validity or full completion policy. Required command/error acknowledgements remain explicit messages.

Keep Rayon as the shared CPU execution pool and use Crossbeam channels for bounded messages and waiting on several message sources. Standard synchronization remains sufficient for the latest-plan slot and short control state. Migrate the chosen orchestration boundaries consistently without building custom queues or a new concurrency runtime. Tokio is not required by the present blocking engine and dedicated-worker design. Its async facilities can be reconsidered for a concrete async workload.

Orchestration locks protect publication and small bookkeeping updates only. Do not hold them across engine execution, file I/O, GPU waits, blocking channel sends or joins. Workers own their mutable processing state. Shared source, recipe and artifact data is immutable. All shutdown paths signal cancellation/wakeup and release result receivers so blocked senders can finish before their workers are joined.

## Cancellation, yielding and priority

Cancellation, useful-work priority and GPU completion are separate controls:

- **Cancel:** no applicable replaceable consumer still wants this execution. The token is one-way for that attempt. A later request gets a new attempt after the old one acknowledges termination.
- **Yield:** the job is still useful but a higher-priority job needs its execution resource. Return control at a safe engine boundary and requeue useful work. Reuse whatever valid state the existing engine can retain. Resumable stack continuations are not required initially.
- **Promote:** foreground demand joins the same queued or running work. Raise its effective priority and attach the consumer. It must not cancel merely because it began speculatively.

The execution adapter supplies `ExecutionControl::checkpoint()` at existing bounded read, decode and processing boundaries. Source-child CPU tasks share the same control. The cancellation flag and mutable priority can use `Arc`-owned atomics. These atomics are control signals, not publication of mutable payloads. Immutable plans are published through the synchronized slot. The priority gate uses ordinary synchronization and never mutates editing state.

A worker must never pause waiting for foreground work that can run only on that same worker. In particular, idle GPU preparation must return or unwind at a processing boundary to let the interactive worker serve foreground work. Independently threaded batch work may pause at safe boundaries while the interactive worker runs.

Priority follows required work. If foreground rendering depends on a source decode already in progress, that decode inherits foreground priority. It cannot remain blocked as background work behind its own foreground consumer. Promotion and demand reconciliation precede publication of the new priority demand.

An explicit execution policy governs completion/backpressure. The mere presence of a cancellation predicate must not choose GPU waiting behavior. Initially preserve the current completed-operation boundary for interactive/background graph work, with required readbacks and display preparation expressing their own needs. Already-submitted GPU commands finish normally. Lower host QoS supplements this policy but does not provide GPU preemption or a fixed latency guarantee.

## Results and resources

Completion disposition is an explicit function of the completion, current demand and view context. It separately determines exact request settlement, reusable data retention, compatible progressive feedback and current display destination. A late or cancelled completion cannot settle a newer demand simply because its photo matches. Metadata and resolved defaults update read-only result context, not authored edits.

Cache admission checks validity, usefulness, capacity and access intent. Speculative preparation does not acquire actual-use recency. Foreground and background accesses carry that intent to both decoded-source and processing caches. Disk and memory budgets remain separate. Retained/reserved memory accounting and bounded simultaneous source buffers are separate from CPU worker count. Necessary temporary processing has measured headroom and is not represented as a guaranteed hard process-memory cap.

No orchestration decision inspects photographic node names to infer reuse. The engine's valid working sets and artifacts retain their own provenance. CPU-side graph preparation and GPU operation implementations remain inside the engine boundary.

## Verification before migration

First exercise the coordinator as a deterministic state machine with a fake executor. Advance starts, checkpoints and completions explicitly instead of relying on sleeps. Check:

- Multiple demands for identical work create one invocation, and priority changes do not alter its work key.
- A foreground consumer promotes existing source or image work. Required source work cannot wait behind its own consumer.
- Independent background CPU work can progress during foreground GPU work when capacity permits. Competing background GPU work yields, and foreground CPU demand receives priority over speculative CPU work.
- Removing one consumer preserves work needed by another. Removing the final replaceable consumer cancels its attempt.
- A yielded speculative job releases the interactive lane. Foreground work runs before it retries.
- Out-of-order, cancelled, removed-target and changed-source results cannot settle newer demand or replace incompatible images.
- Rapid edits preserve fair photo service and every latest foreground demand eventually settles. Background completion is not part of foreground progress settlement.
- Memory admission and command backpressure stay bounded, and speculative access does not promote actual-use recency.
- Completion channels at capacity and shutdown cannot create a UI/worker wait cycle.
- Planning and cache/resource updates progress without a UI paint. A busy UI cannot block orchestration publication or allow an old-context image update to replace the current view.

Then run the maintained native scenarios and exact cached-versus-fresh image checks. Trace job identity, attached demand, priority changes, admission, checkpoint/yield/cancellation, cache reuse and result disposition at common boundaries. Keep Off timing comparisons separate from Wall/GPU attribution. Preserve the current serial execution baseline before considering concurrent image execution.

## Patterns informing the proposal

This proposal combines established mechanisms while keeping the implementation in existing Rust primitives:

- [Kotlin StateFlow](https://kotlinlang.org/api/kotlinx.coroutines/kotlinx-coroutines-core/kotlinx.coroutines.flow/-state-flow/) illustrates read-only current state and conflated updates where slow consumers skip intermediate states. The proposed latest-plan slot adopts that behavior without adding Kotlin or a reactive framework.
- [Rust bounded channels](https://doc.rust-lang.org/std/sync/mpsc/fn.sync_channel.html) provide explicit channel capacity and backpressure. UI publication must account for their blocking-send semantics.
- [Rayon](https://docs.rs/rayon/latest/rayon/) supplies the existing data-parallel CPU pool. [Crossbeam channels](https://docs.rs/crossbeam-channel/latest/crossbeam_channel/) provide bounded message passing and channel selection without an asynchronous runtime. Neither chooses this application's work priority or cache policy.
- [Tokio's cancellation guidance](https://tokio.rs/tokio/topics/shutdown) separates signalling termination from waiting for completion. The same cooperative pattern can use existing threads and atomic controls without adding Tokio.
- [Tokio's blocking-task guidance](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) recommends dedicated threads for persistent blocking workloads and notes that started blocking tasks are not stopped by aborting their handles. An async runtime would not replace the engine's cooperative checkpoints.
- [darktable's pixelpipe architecture](https://github.com/darktable-org/darktable/blob/master/dev-doc/pixelpipe_architecture.md) separates editing parameters from execution-context parameter copies and processing caches. The proposed immutable request boundary follows that ownership principle while preserving this project's engine.
