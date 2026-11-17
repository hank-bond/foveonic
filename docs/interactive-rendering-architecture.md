---
summary: "Maps implemented editing, scheduling, cache ownership and delivery boundaries, including places where control policy is distributed."
kind: explanation
---

# Interactive rendering architecture

This is a code-based map of the current implementation, including diagnostics and idle Thumb preparation. It describes who makes each decision and who retains each resource. It does not establish conformance for every combination of interactions. Required behavior lives in [interactive rendering](./specs/interactive-rendering.md), [multi-edit](./specs/multi-edit.md), [small previews](./specs/small-previews.md) and [diagnostics](./specs/performance-diagnostics.md). The [latency investigation](../plans/multi-edit-latency.md) retains earlier measurements and stabilization evidence.

The [architecture guide](./architecture/AGENTS.md) routes shared principles, vocabulary and open questions. The [orchestration proposal](./specs/render-orchestration.md) records agreed goals and provisional boundaries, separate from this implemented-code map. The [render runtime migration](../plans/render-runtime.md) records the broader ownership work. The [wide cleanup](../plans/archived/wide-rendering-cleanup.md) tracks final shared-execution convergence and verification.

The [coordination consistency inventory](../plans/render-coordination-consistency.md) records the broader source audit and contract questions to review before selecting incremental cleanup slices. Its findings are structural observations unless runtime evidence is stated explicitly.

## Request and delivery flow

```mermaid
flowchart TD
    Edit[Selection, controls, tools or Smart refinement] --> UI[App and multi-edit runtime author requests]
    UI --> Commands[Typed commands and immutable view context]
    Commands --> Coordinator[Coordinator thread: desired targets and generations]
    Coordinator --> Plan[One replaceable WorkPlan]
    Plan --> Worker[Render worker: Cursor chooses work]
    Worker --> Reuse[Reuse presentation, reduced session, or native source]
    Reuse -->|Missing source| Loading[Source loader: disk access and CPU decode]
    Loading --> Ready[Coordinator publishes source readiness]
    Ready --> Worker
    Reuse --> Numeric[Engine: author graph and execute required dependencies]
    Reuse --> View[Prepare or reuse display texture]
    Numeric --> View
    View --> Reply[Bounded image reply channel]
    Reply --> Handler[Coordinator result handler: settle, classify, retain and admit]
    Handler --> Frames[UI applies active, detail or overview updates]
    Handler -->|Completion and optional continuation| Coordinator
    Frames --> Draw[Native drawing consumes immutable textures]
    Frames --> Aux[Browser previews, navigator and image-matched scopes]
```

The processing and presentation paths are alternatives. Reframing compatible pixels can skip photographic execution. A reduced-session hit can skip source loading and reuse graph dependencies, while still executing a changed finishing operation. A decoded disk hit skips original-file access and entropy decoding but can require all native preparation before pooling.

The entry points are `App::select`, `submit_with_quality`, `receive` and `update` in [main.rs](../crates/foveonic-gui/src/main.rs), and `sync_multi_edit`, `sync_render_demand` and `submit_photos` in [multi_edit_runtime.rs](../crates/foveonic-gui/src/multi_edit_runtime.rs). The UI sends typed commands and immutable view context through [runtime.rs](../crates/foveonic-gui/src/rendering/runtime.rs). Its dedicated coordinator thread receives worker results, calls `handle_result` and publishes the next plan independently of UI paints. The UI rechecks returned `ResultUpdate` facts against its latest edits and view before application. Smart publishes a frozen `Refinement` pair with ordered revisions. The coordinator admits the regional result before activating optional whole-image work, independently of UI receipt or other photographs. Each displayed photo derives its region from its own viewport and compatible whole frame. Current details and continuations follow the complete demanded set across active selection. Camera/Defaults Before follows the viewport editing targets with each photograph’s own baseline and geometry. Non-editing visible peers remain Current. Inspection remains active-only. The existing bounded photo owner retains alternate whole/detail frames, while the selected photograph transfers them through its existing Before presentation state. Per-photo content ordering uses the immutable delivered request revision, independently of presentation revisions used by drawing and scopes. Panning does not restart the initial region for unchanged edits. The engine retains ownership of regional capabilities and numerical reuse.

Result handling settles the matching request, classifies reuse, retains useful pixels and checks source/revision admission as one operation before publishing the resulting plan. Allowed photo feedback survives image rejection. The UI applies progress/opening/export state and acknowledges Active image delivery. Request publication is nonblocking with respect to queue capacity. The coordinator reads queued commands in batches, so per-photo replacement and coalescing happen before another plan is exposed. UI status reads use the last observed immutable snapshot if publication is in progress.

[request.rs](../crates/foveonic-gui/src/rendering/request.rs) owns the immutable request data, consumer-purpose classification and preview geometry policy. The coordinator derives target identity from the request's purpose. The worker applies its preview policy after resolving camera controls and chooses the corresponding execution resources. Foreground preview resolution and idle preparation share camera-aware scale selection through processing capabilities. Reusing a framing plane can still preserve an already-rendered grid instead of applying a new preview scale.

The photo runtime keeps editing selection and bounded render demand separately. Layout supplies actually displayed paths. Render demand includes editing targets and displayed photos already admitted to the existing photo owner. It supplies request snapshots, coordinator retention and delivery visibility. Thumbnail-only display does not acquire a source or promote photo history. Active inspection uses its own submission path and is excluded from ordinary overview requests.

Completed photographic frames remain in the existing photo owner while inspection is active. Returning to the photograph restores a compatible owned frame directly and still publishes current demand normally. Source, settings, calibration and scale checks are shared with cached selection. A new presentation revision preserves the original processing-task identity.

## State and ownership

| Concept | Current owner | Boundary |
| --- | --- | --- |
| Browser membership | `Browser` | Determines which photographs appear and the scope of file operations. It is independent of the bounded editing subset. |
| Viewport editing subset | [viewport_selection.rs](../crates/foveonic-gui/src/viewport_selection.rs) | Pointer interaction changes the editing targets. Photograph tools use those targets without owning selection. |
| Requested edits | `App::settings`, `Runtime::photos`, `sidecars::Session` | Mutable representations are synchronized between active controls, per-photo editing state and persistence. There is currently no single container holding all three roles. |
| Desired render work | [RenderCoordinator](../crates/foveonic-gui/src/rendering/coordinator.rs), run by [runtime.rs](../crates/foveonic-gui/src/rendering/runtime.rs) | Owns task identity, replacement/coalescing, settlement and admitted revision order per photo and purpose on the coordinator thread. |
| Request purpose and preview policy | [request.rs](../crates/foveonic-gui/src/rendering/request.rs) | Describes requested work independently of worker execution. Consumer purpose, nominal scale, camera-resolved scale and actual reused grid remain distinct. |
| Numeric editing values and reset values | [render/edits.rs](../crates/foveonic/src/render/edits.rs) | The processor derives snapshots from camera metadata and requested settings. The worker adds render-produced estimates and publishes them. Multi-edit retains/applies them without owning camera interpretation. |
| Execution service order | `Cursor` and the [render worker](../crates/foveonic-gui/src/rendering/worker/mod.rs) | Cursor preserves service across plan replacements. A job that needs source data defers without settling, allowing other ready jobs to execute. The worker retains processing sessions and prepares presentation resources. |
| Requested source acquisition | [source_loader.rs](../crates/foveonic-gui/src/rendering/source_loader.rs) | The coordinator retains wanted loads and admits one active or unconsumed decoded source. A separate I/O thread opens the existing decoded cache or original. Decoding uses the existing shared CPU pool. The renderer consumes the bounded handoff, then builds its processing session. |
| Recipe and numerical validity | [recipes](../crates/foveonic/src/render/recipes/AGENTS.md), [graph](../crates/foveonic/src/graph/AGENTS.md), [interactive.rs](../crates/foveonic/src/interactive.rs) | Recipes describe operations. Effective operation keys and dependency versions determine reuse. The executor runs required missing work. |
| Visible image | `App::frame`, per-photo `Runtime` whole/detail images, `Before` detail | UI-owned immutable completions. Their requested settings and pixel provenance remain separate from newer controls. |
| Shared retained-memory policy | [render/memory.rs](../crates/foveonic/src/render/memory.rs) | The UI inventories its resources. The coordinator publishes retained-image handles, which the worker combines with the frontend inventory for core accounting and trimming. Essential active inputs can exceed the target. |
| Cross-worker priority | [background.rs](../crates/foveonic/src/background.rs) | Shared demand counters and interruption epochs govern participating I/O, CPU and GPU checkpoints. It is not a hardware GPU priority queue. |

`sidecars::Session` owns asynchronous recipe restoration and persistence policy. `CachedRecipes` is an immutable bounded snapshot used by idle preparation, rather than another sidecar reader. The active edit and multi-edit runtime still exchange requested values through explicit synchronization calls.

## Data representations and caches

```mermaid
flowchart LR
    Original[Original RAW bytes] --> Decode[CPU decode and metadata]
    Disk[Decoded disk bundle] --> Source[Immutable native Source]
    Decode --> Source
    Source --> Native[Native RenderSession]
    Source --> Cut[Graph-declared corrected sensor and analysis dependencies]
    Cut --> Reduced[Source-free ReducedSession]
    Native --> Graph[Complete selected recipe]
    Reduced --> Graph
    Graph --> Pixels[Owned photographic GPU image]
    Pixels --> Texture[Display texture and logical framing]
    Texture --> Display[UI frame and comparison tiles]
```

Disk and memory caches retain different representations. They cannot share one readiness flag or be treated as equivalent hits.

| Retained data | Owner and validity | Release or admission policy |
| --- | --- | --- |
| Original bytes, native decoded samples, selection metadata and disk previews | [decode_cache](../crates/foveonic/src/decode_cache/AGENTS.md). Verified file identity and format version. Preview validity also includes its recipe. | Separate disk budget, neighborhood preference and file recency. Green describes a complete local source bundle. |
| Decoded CPU samples for a live native photo | `RenderSession` holds an `Arc<Source>`. | Native session lifetime. A reduced session releases its source after sealing. A pending cache write may temporarily share it. |
| Immutable CPU preparation and scalar source statistics | `Engine` preparation caches and session-pinned preparation. | Bounded owners. Metadata, source and effective parameter identities govern reuse. These are separate from GPU checkpoints. |
| Native graph checkpoints | `RenderSession::cache`. | Retains successful edit restart inputs and optional alternatives within the graph cache limit. |
| Native and reduced processing sessions | [ProcessingWorkingSet](../crates/foveonic/src/render/working_set.rs) holds both session kinds. The worker [processing owner](../crates/foveonic-gui/src/rendering/worker/processing.rs) maps verified photographs to session handles. | At most one native and one reduced session per verified photograph share a budget and recency policy. Foreground use protects the required session. Execution claims independently protect participating sessions until completion and finalization. Preparation stays cold and does not promote it. |
| Whole-image presentation snapshots | Worker `Presentation`, plus coordinator-delivered `Snapshot` seeds. | Reuse compatible photographic pixels and display textures across reframing and retained target requests. These are budgeted image owners, not numerical dependency keys. |
| Delivered frames, queued replies, browser and navigator textures | GUI owners plus worker pending-image reservations. | The shared inventory counts distinct allocations. These references can keep storage alive after a processing entry is evicted. |
| Execution scratch | [gpu/scratch.rs](../crates/foveonic/src/gpu/scratch.rs) and guarded storage. | Reuse follows reader ownership and ordered submission. Retained immutable checkpoints are not available for scratch writes. |

The shared memory target is an accounting and retention policy, not a hard process or driver allocation limit. `Engine::trim_memory` trims optional versions, cold preparation and inactive entries. It protects essential active inputs and reports excess. Idle preparation checks its cold candidate against the shared inventory before retaining it, without evicting other sessions to make room. Ordinary view clearing parks retained sessions. Explicit processing clearing releases both session kinds. Temporary source loading and reconstruction still require peak headroom.

Session execution protection is separate from foreground recency. A private [session execution](../crates/foveonic/src/render/driver.rs) owns one boxed job and its claim, immutable configuration, prepared recipe, graph cursor, scratch, profile and retirement state. Sessions and source identity remain in the processing working set. `RenderExecution` borrows the engine and native or reduced session locally for preparation, advancement and finalization. Refresh operations acquire their own claims. Maintenance skips executing sessions and protects their preparation inputs. Removal, replacement and clear reject a busy photo before changing its owners. Deferred parking resumes after the claim retires.

All numerical work uses the same graph start, advancement and finalization lifecycle. The driver owns synchronous recipe preparation, artifact planning and packaging as well as resumable claimed executions. Fresh rendering, DNG and image encoding use queued completion with allocation overlap bounded at existing boundaries. Borrowed interactive sessions, reduced sealing and ordinary claimed execution complete each operation. Grouped jobs use caller-completed submission windows. Completion policy is explicit and independent of cancellation. Picking, exports, prefetch and other-camera rendering retain their necessary input, cache transaction and output adapters without a separate numerical render loop. The native claimed entry point accepts one or more requested photographic outputs and preserves selective preparation and caller output order.

Synchronous rendering, grouped rendering and presentation reuse converge on the worker’s `Ready` result before view preparation. It carries the completed artifacts, source identity, resolved context, original edit classification, comparison input and any reusable display textures. The common consumer checks cancellation, prepares or reuses views and publishes the normal reply inside its priority attempt. Executing profiles stay with live jobs. Export completion remains separate from image extraction. At completed windows, cached viewport delivery and initial source deferral can proceed without retiring useful peers. Genuinely cold or unsupported numerical preparation remains exclusive. Ordinary trimming waits until grouped live and ready results retire, while window admission inventories those owners and reconsiders concurrency.

The opt-in worker owns several executions and advances them through bounded submission windows. Supported warm Current, Detail and whole-photo Preview work share this lifecycle. Each group uses one current coordinator priority tier. Preview preparation and advancement use the existing auxiliary or thumbnail execution gate, and yield safely to higher-tier demand without settling wanted work. Fitted whole-photo main output already seeds browser thumbnails directly, so it does not need a separate photographic pass. Candidate eligibility checks remain separate from camera/sampling resolution after a task is claimed. Ordinary and wide work share that resolver while presentation reuse preserves its actual photographic grid. One identity-deduplicated inventory covers job containers, live graph values, scratch, session/preparation buffers, frontend reservations and completed artifacts awaiting delivery. The feature build chooses concurrency automatically under an internal ceiling of eight. Cache memory is the user-facing resource control. Additional warm work requires room under the shared cache target for actual storage plus coarse workspace headroom. Native jobs also use a total 1 GiB-per-photo capacity floor. Admission checks this inventory without trimming it. At completed windows, the worker reduces surplus live peers when retained/frontend/live storage no longer fits, preserving ready results before ordinary post-delivery budget maintenance. It does not evict processing inputs, history, sources or sessions to force concurrency. Mandatory width-one work may exceed the soft target. Within-operation temporary bounds and driver/allocator overhead remain incomplete, so this is an experimental admission heuristic rather than a process-memory cap.

Foreground source loads are coalesced by path independently of authored edits. Presentation and processing-session reuse are checked before requesting a source. Only the render job that actually found missing input waits for loading. A replacement request can still reuse GPU data. The coordinator receives source outcomes even when UI image delivery is full. Removed loads are discarded, and a ready source waiting for the renderer is included in retained-memory accounting. In-flight source reads can finish after their request becomes obsolete. Idle Thumb preparation still performs its cached-source access inside the existing speculative operation.

## Identities are for different decisions

| Identity | Meaning |
| --- | --- |
| `SourceOrigin` / verified file identity | Which immutable source the data belongs to. A path alone is insufficient. |
| `Target { path, purpose }` | Which requested result is being coordinated. Current, Before renditions, detail, inspection and export have different purposes. |
| `TaskId` | A coordinator generation for pixel work. Compatible presentation changes can retain it. |
| Task revision | Requested delivery/presentation version. It is used for exact settlement and admission ordering. A compatible older completion can still provide feedback. |
| Processing session handle | A worker-local lookup handle. It does not establish numerical validity or replace source identity. |
| Graph version and effective operation key | Which numeric result can be reused, including upstream ancestry and working geometry. |
| Calibration signature and view context | Compatibility of a completion with the current processing and display context. |

Source identity, cache validity, task settlement and display acceptance are separate decisions. For example, a superseded completion can contain reusable compatible pixels without settling the newest request. A valid cached result can also belong to a photograph no longer visible.

[matching.rs](../crates/foveonic-gui/src/rendering/matching.rs) owns the shared settings and request comparisons used by Smart, presentation reuse, worker continuation, coordinator coalescing and delivery. Authored-edit equality ignores transient processing requests and range labels. Pixel-settings equality also ignores crop and rotation. Geometry equality checks crop, rotation and output size. Request comparisons add consumer context and quality/region requirements. They do not establish source identity, graph validity or permission to update a view. Those decisions remain with their existing owners.

## Scheduling and completion boundaries

One interactive host thread owns execution. In the experimental build it advances supported photographs through bounded GPU submission windows. The UI, navigator, preview persistence and batch executor can also submit work to the shared ordered device queue. Compiled kernels and GPU storage mechanisms are shared through the engine/device interfaces. The CPU processing pool is separately sized from host logical CPUs, with bounded photo admission for speculative decoding.

The interactive worker's current service order is:

1. Apply pending memory configuration and handle stop/clear.
2. Serve a new color-pick task.
3. Take a frozen interactive export, one compatible follow-up, or the next ready image from the fair cursor at viewport, auxiliary or thumbnail priority.
4. Analyze image-matched scopes after viewport work and other foreground demand settle.
5. Attempt an enabled full-image continuation only when higher-priority work is absent and its native source remains retained. Yield at existing safe boundaries when any higher-priority tier arrives.
6. Release foreground demand and attempt one idle preparation job, then recheck work or wait.

This ordering is implemented inside the worker loop. The coordinator's `WorkPlan` contains image tasks, scopes and picks. Memory commands, frozen interactive export and idle preparation configuration live alongside that plan in `Pending`.

The shared gate orders viewport demand, navigator/scopes, browser thumbnails, requested batch processing, then optional background work. Full-image continuation uses an idle-only attempt that observes arrivals at every higher tier, including transient demand that already finished. It does not count as foreground busy state or restart foreground progress. The persistent Settings option “Complete full images while idle” disables this continuation while preserving explicit whole-image requests and compatible cached results. Optional continuation does not initiate source acquisition. Missing retained sources leave it unserved until useful work restores them.

In-memory speculative decoding retains its separate interruption policy. A paused batch keeps its progress. A prefetch attempt is invalidated by a foreground epoch change and retries later. These policies share the gate while retaining their distinct lifecycles.

Regional Current intent now includes an independently requested reduced `Preview` target. After regional admission the coordinator publishes it regardless of the native-completion setting. Active photographic preview demand uses navigator/scope priority, and other photo previews use thumbnail priority. The existing source loader carries this priority through shared acquisition, cancellation, retry and same-path promotion. Source diagnostics retain the original producer context when acquisition is shared or promoted.

The bounded photo owner retains one preview separately from viewport whole/detail frames and includes it in the shared memory inventory. Dedicated preview delivery updates browser textures without changing authored settings or viewport progress. Navigator and whole-photo scopes select a compatible preview, preferring an existing matching finer whole image. Covered viewport scopes still use native detail. Thus whole-photo scopes can use reduced-preview sampling, as in a fitted view, while native completion is disabled. Native viewport pixels and overlays are not replaced by preview arrival. Source, edit, calibration and quality checks reject stale preview delivery.

| Boundary | Implemented behavior |
| --- | --- |
| Recipe preparation and required graph operation | Advisory cancellation checks stop obsolete work at safe boundaries. The worker's compatibility predicate can allow useful same-edit continuation. |
| Graph operation or submission window | Explicit operation completion waits before advancing or reporting progress. Grouped execution defers progress until its caller completes the window. Background submissions inside either scope defer individual waits through [gpu/scheduling.rs](../crates/foveonic/src/gpu/scheduling.rs). Early errors drain the outstanding stage. Queued completion retains its allocation-overlap boundary. |
| Other background GPU submissions | The GPU facade retains its per-submission completion policy. These calls do not inherit a graph-stage completion boundary automatically. |
| Required photographic CPU readback | Waits for mapped data. The shared background readback helper polls cooperatively with foreground priority. |
| Main display preparation | The interactive worker waits for a newly prepared display texture before publishing its frame. Reused textures skip this wait. |
| Reply publication | Worker-to-coordinator and coordinator-to-UI image queues each hold at most two updates. The coordinator can hold one result between them. Crossbeam channel selection lets it keep processing commands while UI delivery is full. Status reads use immutable snapshots with a nonblocking UI read. |
| Navigator | The UI coalesces/defer requests while busy and briefly delays enqueueing so the main image can submit first. Auxiliary demand covers that delay through submission. Its separate worker waits below viewport priority, submits ordered texture work without a CPU completion wait, and releases demand on submission or cancellation. |
| Browser preview persistence | Immediate GPU thumbnails are retained separately from optional background downsampling/readback for disk persistence. Disk policy is checked before the optional handoff. |

GPU work already submitted is not preempted. These boundaries limit backlog and define when priority takes effect. They do not establish a fixed interruption deadline under arbitrary load.

Guarded ownership and ordered-queue reuse provide storage correctness independently of these completion waits. Waiting after an operation is currently a scheduling, cancellation and temporary-overlap policy. It is not what makes an immutable cached image safe.

## Where policy is distributed

These are source observations and review targets. They are not all demonstrated runtime defects.

| Decision | Current locations | Review concern |
| --- | --- | --- |
| Requested edit state | `App`, multi-edit `Runtime`, sidecar `Session` | Synchronization establishes precedence across three mutable representations. A new interaction must follow that precedence correctly. |
| Result handling | Coordinator `handle_result` owns settlement, invokes [delivery classification](../crates/foveonic-gui/src/rendering/delivery.rs), retains reusable pixels and performs source/order admission. The runtime publishes the resulting plan off the UI. `App::receive` checks current view compatibility, applies photo/view updates and acknowledges Active delivery. | Ordinary engine execution is serial. The opt-in wide worker groups supported warm jobs. Runtime retention and Smart drawing enforce the validity of their held frames. |
| Effective execution request | `App::submit_photos` publishes ordinary photo demand, and `submit_with_quality` publishes specialized active requests. Smart freezes its first request and optional whole-image continuation. `Request` owns purpose, preview-scale application and reduced-session selection. Presentation reuse can retain the actual existing grid. | Request policy now has one home. Camera/default resolution and actual retained representation remain separate boundaries. |
| Cancellation | Worker foreground predicate, graph `Execution::check`, gate epochs, source/cache checkpoints and idle preparation predicate | Request obsolescence and priority interruption are distinct mechanisms. Their composition is intentional but currently implicit in nested closures and thread-local context. |
| GPU completion policy | Shared graph advancement takes explicit queued, operation or caller-completed window policy. Background stage scope, GPU submission facade and display preparation apply their resource-specific boundaries. | Cancellation checks decide whether work remains useful. Completion policy independently decides when submitted work must finish and progress can publish. |
| Memory admission | GUI inventory, worker trimming, core graph/working-set limits and idle admission checks | The accounting is shared, while policy is applied at several resource lifetimes. Retained targets and physical peak memory are different quantities. |
| Speculative reuse | Idle preparation, decoded cache access and reduced cold insertion | GPU preparation does not promote reduced history. Its local source read uses ordinary `open_cached`, whose successful payload load updates disk recency. Access intent is not explicit across both caches. |
| Priority for auxiliary work | Shared gate, worker ordering, navigator enqueue policy and GPU facade | Not every GPU producer participates in the same gate. Request deferral and submission backpressure currently supply different parts of the priority behavior. |
| Preparation cost | Recipe authors, lazy descriptions, executor materialization and engine preparation caches | Graph reuse cannot suppress eager work done while authoring a graph. The row-correction fix moved materialization behind execution without changing its arithmetic. |
| Busy and progress | `App::busy`, coordinator settlement, `Pending::activity`, worker execution and progress tracker | These mean different things. Idle prewarming, pending foreground work and a displayed timer must not be inferred from one another. |

The numerical layer has clearer boundaries: camera recipes author graphs, operation identities determine validity, the executor selects dependencies, and processing nodes implement arithmetic. The largest orchestration concentration is still the GUI worker loop and its per-request closure. Extracting helpers without making the above policy explicit would move the nesting without resolving that concentration.

`Request::uses_reduced_session` checks explicit preview mode, final output, comparison and regional demand. It does not enumerate cameras or photographic option combinations. Recipes and dependency cuts decide which native inputs are required, and shared post-reconstruction nodes consume explicit working geometry. This is an existing structural boundary to preserve during orchestration cleanup.

The current deterministic behavior checks are concentrated in [request-policy tests](../crates/foveonic-gui/src/rendering/request_test.rs), [coordinator tests](../crates/foveonic-gui/src/rendering/coordinator_test.rs), [delivery tests](../crates/foveonic-gui/src/rendering/delivery_test.rs), [presentation handling and tests](../crates/foveonic-gui/src/rendering/worker/presentation.rs), [graph cache tests](../crates/foveonic/src/interactive.rs), and [reduced ownership/output tests](../crates/foveonic/src/render/reduced_test.rs). [Native scenarios](../crates/foveonic-gui/src/smoke/scenarios.rs) exercise the application composition. Their work counts, timing results and output-equality checks are separate kinds of evidence.

## Using the map

For each remaining bottleneck, identify the governing decision and resource owner first. Use the trace to establish whether work was required, where it waited, and which result was admitted. Add a behavioral check at that boundary before changing its implementation. Keep timing comparisons separate from detailed attribution and numeric equality checks.

The refactor should preserve source identities, graph validity, immutable results, per-target service and completion rules. Further cleanup should be driven by observed work or ownership problems. Input/cache transactions and file publication remain distinct adapters around the shared numerical lifecycle. A new actor runtime, universal job graph or concurrent GPU scheduler is not required by the current evidence.
