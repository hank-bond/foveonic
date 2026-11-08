---
summary: "Reduce multi-edit application latency through work avoidance, reusable preparation, scale-aware execution and coordinated delivery."
---

# Multi-edit latency

Implements the latency direction in [multi-edit](../docs/specs/multi-edit.md), [small previews](../docs/specs/small-previews.md) and [render preparation](../docs/specs/render-preparation.md), using [shared diagnostics](../docs/specs/performance-diagnostics.md). The owner prioritizes application responsiveness, avoiding unnecessary work, preparing reusable data ahead of demand and making processing cost follow the working pixel count. Full-resolution arithmetic optimization within individual nodes is outside this round.

## Release stabilization

The owner confirmed Draft, Thumb and multi-edit as the intended 0.8 feature set. Stabilize the existing implementation before revisiting the orchestration proposal. Correct processing boundaries, consolidate repeated state decisions and remove unused paths while retaining the current coordinator, executor and cache owners. No new execution runtime or concurrent GPU scheduler is part of this pass.

- [x] Separate the completed compact-cache and tolerant-sidecar changes into independently checked main-based commits.
- [x] Isolate the completed Perfetto foundation and application instrumentation in reviewed checkpoints.
- [x] Check unchanged group selection, active-photo switching, relative edits, rapid scrubbing and complete progress settlement through the real application.
- [ ] Record a matched work census for cached group selection and retained edits, including actual scale and cold versus retained processing state.
- [ ] Correct demonstrated pipeline/preparation inefficiencies within the current design. Keep native sensor corrections distinct from scale-aware downstream processing.
- [ ] Review the remaining current-architecture changes as small tested units before release.

## Existing owners and evidence

The current checkpoint includes explicit completion classification, source-aware admission, idle Thumb preparation, cooperative GPU waits and application-wide Perfetto capture/scenarios. Native checks below cover the demonstrated interaction paths. The [early-reduction plan](./early-sensor-reduction.md) owns the completed working-grid changes and remaining native noise stages. Further orchestration migration, wider latency acceptance and physical manual-capture controls remain open. The owner authorized committing all completed 0.8 work as a clean starting point for those follow-ups.

The UI coordinator owns desired photo/purpose requests and completion admission. One worker owns numerical execution, reduced-photo sessions and presentation history. Core graph identities and dependency cuts determine numerical reuse. Preserve these owners instead of introducing another request lifecycle or per-node cache policy.

At the start of this investigation, disk prefetch prepared complete decoded-source bundles and corrected/reduced graph inputs were built only on demand through `prepare_reduced_session`. Idle preparation now also warms eligible Thumb sessions through the same API. Green readiness still describes the decoded disk bundle and does not establish a retained processing session. Prior eight-photo captures show successful local decoded reads alongside repeated native preparation before pooling. Retained exposure edits are much faster than first selection with cold processing state.

The immediate workload is an eight-photo culling set followed by quick exposure edits. Source and decoded-cache efficiency remain central to this investigation. A future loupe motivates preserving native-detail recovery, but building that tool is deferred. The owner approved extending decoded prefetch with low-priority Thumb preparation, subject to foreground priority and the existing memory budget.

The worker waits for each newly prepared display texture before returning that photo and advancing to the next task. The coordinator already coalesces requests and preserves fair photo service. Measure queue residence, submission, required waits and delivery separately before changing their ordering or adding concurrent execution.

An unchanged-selection validator now checks zero foreground original/decoded payload reads and zero photographic dispatches, independently of accepted result identity. Unrelated prefetch remains visible separately. Existing eight-photo traces pass the cached-switch checks. A fresh two-photo run passed two switches, then timed out with the previous selection still active and the render worker idle. Competing compiler work began thirteen seconds after that step started. This is an input/selection investigation, not evidence of a slow render. Preserve the failing scenario rather than treating the completed steps as a full pass.

## Implementation sequence

- [x] Map implemented control flow, data ownership, identity meanings and distributed policies in the [architecture map](../docs/interactive-rendering-architecture.md).
- [x] Complete the approved current-architecture completion slice below and hand it back for owner smoke testing.
- [ ] Review the proposed policy boundaries below before the larger refactor.
- [ ] Review the [concrete orchestration proposal](../docs/specs/render-orchestration.md), including shared demand/work identity, executor ownership, promotion/yield/cancellation and explicit completion policy. Implement the first deterministic coordinator slice only after agreement.
- [x] Identify current ownership and distinguish decoded disk readiness from processing readiness.
- [x] Keep the node-kernel experiment out of the current source changes.
- [ ] Diagnose the failed input-to-selection transition and complete unchanged switch/re-add checks through the real app.
- [ ] Produce a work census for cold selection, retained selection, exposure edits, upstream edits and scale switches across one through eight targets. Attribute each native-grid execution, cache miss, invalidation, submission and wait to its reason.
- [ ] Verify the existing reduced dependency cuts survive unchanged target switches and scale transitions. Fix demonstrated premature eviction, invalidation or duplicate submission before adding new cache storage.
- [ ] Improve preparation and reuse for the selected culling set through existing graph, priority and memory owners. Prepare ready decoded neighbors as cold Thumb entries during idle time, without taking over the active photo. Validate interruption, admission and foreground reuse through the real app.
- [ ] Measure Full/Fast/Draft/Thumb with matched settings and distinguish one-time native preparation from repeated working-grid processing and fixed coordination cost.
- [ ] Use those results to choose the smallest submission/delivery change. Keep the current executor initially. Any interleaving or concurrent execution must preserve dependency order, bounded in-flight work, cancellation and per-photo result identity.
- [ ] Validate no unnecessary work, cached-versus-fresh output, final settlement, selection behavior and the shared memory budget through the real app and portable checks.

The owner approved this sequence and requested an exhaustive pass. Simplicity, consistent handling and clear ownership are guiding principles. A justified exception is acceptable when its purpose and measured benefit are clear. Preserve complete selected recipes and valid numerical dependencies.

## Architecture review and refactor checkpoint

### Approved current-architecture slice

The owner chose to maximize correctness, flexibility and speed of the existing design before discussing migration. The orchestration draft remains a direction, rather than the immediate implementation plan. Keep `RenderCoordinator`, `WorkPlan`, `Cursor`, the current threads and cache owners. Do not introduce a replacement demand/job registry or change scheduling policy in this slice.

- [x] Preserve the pre-slice source locally for review against the existing uncommitted work.
- [x] Extract current completion classification into `render_delivery`: reusable pixel history, per-photo feedback, active-view delivery, overview-tile delivery and explicit rejection.
- [x] Put source-origin validation beside revision ordering in coordinator admission. Both active and overview images use the same admission path.
- [x] Keep completion settlement and progress independent of whether a result can currently be drawn. Retain the existing older-frame feedback rules and frozen export behavior.
- [x] Check deterministic delivery and coordinator behavior, existing native switching/supersession scenarios and required repository checks.
- [x] Build a reviewable local app and stop for owner interaction testing before another architectural step.
- [ ] Receive owner interaction feedback before selecting the next change.

Classification borrows image/settings facts without acquiring GPU data. Before retains authored framing, so progressive delivery compares that framing directly instead of reconstructing camera/default processing settings on the UI thread. The Before test checks that invariant for crop, rotation and output size. This is a work-avoidance change, not a measured whole-application latency claim.

Validation passed: 516 core tests, 416 GUI tests, the remaining workspace suites, strict workspace Clippy, formatting, documentation generation/checking and artwork checks. Thirteen new deterministic tests cover delivery and source admission, including a combined coordinator/delivery sequence in which older feedback advances, the newest image wins and the remaining photo settles independently. Existing GPU fixture tests remain opt-in.

Two uncontended native Wall scenarios passed. Mixed Merrill/Quattro unchanged switching performed no source payload reads or photographic dispatches. Removal/re-addition around an edit completed without rereads. An eight-Merrill rapid-supersession run delivered the latest state and settled after twenty group edits without rereading sources. The existing seventeen-capture native multi-edit workflow also passed, including unchanged texture identity, relative edits, Before restoration, selection modifiers and Hand selection behavior. These checks establish observed behavior and work counts, not a matched latency improvement or fresh numerical equivalence. Owner smoke testing remains the checkpoint before further structural changes. No orchestration migration is part of this slice.

### Longer-term direction

The owner approved mapping the current implementation now, finishing the main interaction pathways, then reviewing a working commit before a larger simplification. This mapping pass changes documentation only. It does not authorize a new scheduling framework or mark all current behavior as conforming.

The [architecture map](../docs/interactive-rendering-architecture.md) records actual ownership and conditions. The main review candidates are requested edit state, effective execution-request resolution, completion disposition, cancellation and priority boundaries. The worker loop currently combines service order with source acquisition, recipe resolution, numerical execution, memory management and publication. The coordinator alone does not implement all visible-image admission policy.

The owner subsequently agreed that the pipeline should be treated as an engine behind a clear contract, with one state owner supplying consistent intent to foreground scheduling and a background planner. The concrete [orchestration draft](../docs/specs/render-orchestration.md) now describes proposed Rust data structures, a latest-value work plan, bounded result/command channels, thread ownership and deterministic concurrency tests. Demand is distinct from execution work. Cancellation of obsolete work is distinct from yielding useful lower-priority work, and matching foreground demand promotes a shared job. These details remain proposed until reviewed. The draft changes no runtime behavior.

The owner also wants central coordination to keep CPU and GPU supplied with useful work. The draft now makes admission resource-specific: independent background CPU/I/O should be able to overlap foreground GPU execution within memory limits, rather than all background work stopping behind a global foreground flag. The selected primitives are the existing Rayon pool, dedicated engine workers and Crossbeam bounded messaging/selection. Application demand, promotion, fairness and resource policy remain small explicit coordinator logic. No concurrency dependency has been added or replaced.

The draft's smallest implementation keeps two control-state owners with pure planning/resolution/disposition functions, one small job table, exact request coalescing, fixed work/resource classes and one latest-plan notification mechanism. Reuse existing cache owners and engine state across interrupted work. General actor/runtime machinery, scheduler-level recipe equivalence, automatic resource tuning and resumable node continuations are outside this first refactor. The detailed interfaces remain draft, and no runtime/dependency migration has started.

The owner corrected the proposed UI-thread placement of planning and resource availability. The draft now uses one dedicated orchestration thread for scheduling, background planning, completion policy and direct cache/resource-owner updates. The UI owns authored interaction state, publishes immutable intent, and applies image/status updates with a cheap context/order guard. Availability does not route through UI drawing, and planning must progress even without a paint. This replaces the earlier proposal to keep the coordinator on the UI thread.

Proposed boundaries for review:

- Keep authored settings with the editing state. Give persistence and render requests explicit immutable snapshots. Decide which existing owner is authoritative before moving state.
- Resolve purpose, geometry, working grid and calibration into one execution description before choosing resources. Preserve graph-owned dependency validity and presentation-only reuse.
- Make completion disposition explicit: exact task settlement, reusable pixel seed, compatible feedback, current display destination, or rejection. Keep cache reuse and current-view acceptance distinct.
- Keep task service order and cross-worker priority separate, with explicit operation completion and interruption boundaries. Evaluate every producer sharing the GPU queue, including auxiliary presentation.
- Keep resource accounting and retention policy together at their existing ownership boundaries. Use a common account of retained, reserved and temporary work when evaluating admission.

Use the smallest extraction that makes each policy readable. Keep recipe construction and operation implementation separate from scheduling. Do not invent condition combinations that merely duplicate current branches in a new layer.

The behavioral checkpoint must cover these observable rules before refactoring:

| Interaction | Required evidence | Current coverage |
| --- | --- | --- |
| Retained selection and re-add | No source payload reads or photographic dispatches, compatible image identity and final settlement | Native cases exist. An earlier intermittent selection transition remains open despite a later passing run. |
| Prepared eight-photo selection | All prepared sources are retained, no payload rereads, no repeated native preparation, every target delivered | One uncontended traced Merrill run passed. |
| Repeated group exposure | Reuse upstream work, deliver each latest target, settle progress | The same run passed five edits. |
| Upstream edit or scale change | Regenerate affected cuts, preserve unrelated dependencies, match fresh output | Core fixtures and earlier pipeline checks exist. Wider current native coverage remains open. |
| Crop/rotate and Smart continuation | Reuse compatible pixels and geometry, retain valid native dependencies, avoid old-image regression | Existing worker/presentation checks exist. Include these in the final native matrix. |
| Rapid replacement, removal, Before and calibration changes | Admit compatible ordered results only and eventually settle all desired targets | Coordinator tests exist. Current real-app mixed-condition coverage remains incomplete. |
| Foreground arrival during background preparation | Stop further obsolete stages, retain active state and service foreground work | Gate tests and exact background preparation checks pass. Controlled native interruption timing remains open. |
| Memory pressure | Bounded historical admission, no prepare/evict loop, delivered buffers remain immutable | Core ownership/retention tests exist. Physical peak memory requires separate measurement. |
| Batch plus interactive and auxiliary work | Frozen exports finish, foreground progresses, background backlog stays bounded | Existing batch/gate tests exist. A controlled shared-device native case remains open. |

Record Off baseline distributions and separate Wall/GPU attribution for the same declared work. Preserve failures and invalid timing runs. Review these results and the map before committing the checkpoint. Refactoring then uses the same external behavior checks plus exact cached-versus-fresh image comparisons.

## Current findings and changes

The initial work census distinguishes graph executions by actual scope ancestry and associates asynchronous GPU observations through their submission identities. Repeated node indices in separate graphs cannot merge their work. Reports retain actual input/output geometry and dispatch grids, cache reuse, submissions and incomplete interaction evidence.

The eight-photo Merrill capture shows no source reads or native-grid execution during warmed exposure edits. Each photo executes only its tone node, while most graph nodes are reused. Preparing the unchanged row-correction tables nevertheless takes about 30 ms across the group. Row correction now authors a small deterministic description and materializes its derived fitting tables only when the existing executor runs that node. This follows the existing lazy-preparation boundary and introduces no additional cache owner. Numerical arithmetic is unchanged.

The same capture shows a foreground `queue.submit` interval of about 49 ms overlapping a background preview's device-wide readback wait. This is evidence of possible shared-device contention, requiring a controlled before/after run. A shared readback helper now uses normal nonblocking device polls and the existing priority gate for background work. Required foreground CPU readbacks retain their blocking completion policy. Navigator textures are delivered after submission on the same ordered queue, rather than waiting on the CPU for unrelated device work. The foreground photo executor's completion policy is unchanged pending measurements and bounded in-flight design.

The cooperative readback GPU test passes: foreground work completes while the background caller yields, then the background map returns exact samples. Existing geometric and GPU row-correction fixtures pass after deferred materialization. Fresh native interaction and latency validation remains open.

Idle preparation now obtains known recipes through the sidecar owner's cached snapshot, uses the current ready decoded neighborhood, and inserts source-free Thumb sessions as cold history on the existing render worker. It checks available retained-memory capacity before loading a payload and checks complete admission before insertion. An active native session can lend its immutable source without moving that session. Existing graph keys continue to validate each later foreground render. The final tone output is not retained by this numerical cache, so foreground selection still runs that small pass and prepares its display texture.

An initial native run retained about 1.5 GB for a live native view under the current 1 GB cache target. Idle preparation declined admission, as required. The profiling runner now declares and verifies memory capacity separately from disk capacity. The application default is unchanged.

A controlled background trace exposed 775 individual submission waits totaling about 44 seconds across six prepared photographs. The cancellable executor already completed each graph operation, so these inner waits were redundant. Background submissions within that existing completed-stage scope now defer to its normal boundary. Other background GPU calls keep their submission policy, and an early stage error drains outstanding work. No operation arithmetic or foreground completion wait was changed.

One subsequent uncontended Wall capture prepared all eight Merrill photographs and passed initial selection plus five exposure edits. Prepared selection settled in 195 ms. Exposure edits settled in 100–116 ms. Every measured step had zero original or decoded payload reads and sixteen dispatches: one tone pass and one view preparation per photo. Recorded background preparation scopes totaled 11.24 seconds for eight photographs, with a 1.65-second maximum per photograph. These are instrumented observations, not matched Off baseline improvement claims. The retained logical total during edits was about 297–301 MB. This does not establish physical peak memory or interruption latency.

## Deferred node experiment

Three matched trials on two Merrill originals found approximately eight percent lower GPU blue-table time with larger cooperative loads and redundant classification removed. Whole-render latency was mixed. Final Thumb pixels and full native highlight buffers were byte-identical. A direct-read alternative was slower. All kernel and test-oracle changes were removed from the working source and preserved as local experiment evidence. This finding does not establish a multi-edit latency improvement.
