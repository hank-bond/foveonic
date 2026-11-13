---
summary: "Clarify render ownership and responsibilities through one owner-reviewed concept at a time while preserving serial engine execution."
---

# Render runtime migration

Requirements: [render orchestration](../docs/specs/render-orchestration.md), [interactive rendering](../docs/specs/interactive-rendering.md), [multi-edit](../docs/specs/multi-edit.md), [preparation](../docs/specs/render-preparation.md) and [diagnostics](../docs/specs/performance-diagnostics.md). Start discussions at the [architecture guide](../docs/architecture/AGENTS.md). Current code ownership stays in the [implemented map](../docs/interactive-rendering-architecture.md).

## Resume here

- **Current phase:** requested source loading is separated from rendering and committed. The owner requested an architecture review of the next Smart, layout, persistent zoom and loupe workflows.
- **Checkpoint:** `33453b3` separates requested source loading, `ab317e5` corrects shared-source retry readiness, and `02104b4` groups browser sources. `075bbcc` contains the earlier behavioral coverage and focused delivery fixes. See the [behavioral plan](./behavioral-testing.md) for checks and remaining coverage limits.
- **Completed unit:** source acquisition is independent of the serial renderer. Reuse remains before loading, and an already-ready photo can render while another load is held.
- **Decided:** protect UI responsiveness, use one runtime for individual and multi-photo editing, isolate responsibilities and support subordinate speculative work. Keep engine execution serial throughout this migration.
- **Separate future effort:** [wide rendering](./wide-rendering.md). Clear application/engine ownership should make that experiment easier, without building its execution machinery here.
- **Not decided:** later state ownership, shared foreground/background admission policy and backlog limits.
- **Communication direction:** ordinary typed messages, fixed consumers and directly called handlers. Keep existing library queues across threads. Add a broadcast/watch dependency only when an actual consumer needs the full primitive.
- **Next action:** finish the [portable test-input setup](./behavioral-testing.md#reproducibility-follow-up), then discuss the [viewport architecture review](./viewport-workflows.md), starting with one preview-quality policy. Implementation requires agreement on the next small slice. The earlier cached-switch failure and its subsequent fix remain recorded in the behavioral plan.

## Alignment for every slice

The [three agreed goals](../docs/specs/render-orchestration.md#agreed-goals) are the alignment criteria throughout this migration: maintainability, latency and future execution flexibility. Each review records the ownership improvement, the work or waiting affected and its measured evidence, and the assumptions about serial execution that remain. Improvements need not advance all three equally, but must not obscure a regression in another goal.

For the approved source-loading slice:

- **Maintainability:** foreground source acquisition has a distinct owner. Requested disk-cache access and decoding leave render execution, including native recovery for retained reduced sessions.
- **Latency:** a load cannot occupy the serial renderer while another photograph has ready inputs. Measure held-load behavior and ordinary warm reuse separately rather than assuming a speedup.
- **Execution flexibility:** decoded sources and per-photo readiness are independent of the order in which the engine executes ready rendering work. Keep the current engine serial.

Use existing cache and CPU decoding facilities. Keep the UI nonblocking, source identity explicit, errors per photo, and source handoff bounded. Validate cold loading, supersession, reuse, native recovery, frozen export and picking through the same loading boundary. Record progress and corrected assumptions here as the implementation proceeds.

The first slice covers requested rendering, native recovery, picking and frozen export. Existing idle Thumb preparation still reads decoded bundles inside its speculative worker operation. Moving that optional acquisition requires reviewing its admission and foreground-yield policy together, so it remains an explicit next boundary rather than silently extending this slice into a background scheduler redesign. An obsolete source read may finish, but its unwanted result is discarded. The renderer can continue using already-ready inputs during that read.

### Source-loading checks and lessons

- [x] CPU checks cover per-source edit coalescing, removed/re-added demand, bounded handoff, sibling service and replacement jobs that can reuse retained input.
- [x] The headed `source-loading` case holds A's native load while B's visible Exposure control remains enabled and B delivers its newly edited, settled and admitted Thumb frame. It passes with original access and with the decoded disk cache enabled. Perfetto independently records B's rendering inside the source-pause interval.
- [x] Eight related native scenarios pass: held original load, held decoded-cache load, warm disk, eight-photo cached switching, eight-photo selection under pressure, seeded transitions, lifecycle recovery and foreground editing during batch work. Smart smoke also passes through refinement, crop and retained reopening.
- [x] Nine focused GPU checks pass for reduced preparation/recovery, retained native state, latest multi-edit delivery, continuous edits with scopes, color picking, and Full TIFF/DNG export from reduced preview modes.
- [x] Forty frozen Full/Fast/Draft/Thumb samples remain bit-exact across three Classic, three Merrill, three Quattro and one Tiger source. Image-capturing selection, transition and background journeys also match fresh processing.
- [x] Ordinary workspace tests, the new focused checks, strict Clippy, formatting, portable profiling/artwork checks and documentation/index checks pass.

Only a job that has actually discovered missing input waits for acquisition. Marking an entire path unavailable would incorrectly block a new request that can reuse presentation or session data. Keep per-job waiting separate from per-source load coalescing. Preserve another photo's native session until the requested source is ready, rather than discarding it while waiting.

The first held-load run exposed a harness assumption: the single-photo Exposure widget has a label but no multi-edit JSON pointer. Its enabled/visible check now observes that actual control by label as well. The trace declaration also explicitly expects A's deliberate source read. Neither correction relaxes B's required delivery during the hold. Export checking encountered a full disk, then passed after duplicate generated float samples were removed while preserving the frozen baseline and reports.

Two natural warm-edit Off trials pass. The comparison tool rejects the earlier baseline because recipes and workload metadata differ, so these samples do not establish a before/after latency change. The demonstrated improvement is independent progress during source loading. General throughput remains a separate matched-measurement question.

### Shared-source readiness review

The owner subsequently required a code-design, cleanliness and Rust-practice review subagent for each completed implementation slice. Reviewing `33453b3` exposed a liveness gap that the independent-photo checks had missed. Current and Camera renders can coexist for one photograph. If both deferred on one acquisition, its first consumer emptied the handoff and left the other waiting, even when a native session was now reusable. Picks could encounter the same readiness condition.

The focused correction uses the existing acquisition membership rather than adding a notification protocol or availability registry. `can_retry` permits a deferred consumer when its handoff exists or no acquisition remains outstanding for that path. Consumption by a sibling therefore allows session reuse to be reconsidered. A new miss synchronously registers acquisition again and restores waiting. A shared load failure may require sequential reacquisition to report each consumer's error, but cannot leave the second request permanently pending.

The CPU regression failed on the original predicate, then passed with the correction. It exercises shared Current/Camera failure, pick readiness and a second miss returning to waiting. A real-GPU regression holds native loading until both renders have deferred, then requires both native frames and a successful color pick with only one shared native acquisition. Sixty-four focused CPU tests and strict GUI Clippy pass. Astra's focused implementation review found no remaining substantive design, correctness or idiomatic-Rust issue. Headed Before/After workflow and held-load application checks also pass. The rebuilt application was opened for owner review. It changes neither processing arithmetic nor cache-retention policy.

## Result-handler review unit

**Render result handler at one owner.** The owner agreed this ordinary name for the function or component consuming a render job's result and deciding what the application does with it. It does not name GPU synchronization or UI drawing. Before this unit, `App::receive` called settlement, classification, retention and admission through several methods that could each republish a work plan. The `Worker` handle still owns a UI-thread `RefCell<RenderCoordinator>`.

`RenderCoordinator::handle_result` now handles settlement, reuse and view admission together. It takes the existing borrowed `delivery::ViewContext` and returns `ResultUpdate { current, update_photo, destination }`. The worker handle publishes once for each non-export result. `App` applies photo/view/progress effects and keeps Active-image acknowledgement explicit. Frozen exports retain their distinct result behavior. The removed `finish_image`, `retain_pixels` and `admit_image` worker methods and their test consumers have migrated to this handler.

The unit adds 20 net production lines, including comments and the small return record, while replacing three caller-assembled coordination operations with one handler. The simplification is ownership and ordering rather than source reduction. No new service, transport, subscription abstraction or execution policy was introduced.

Preserve these observed distinctions:

- Frozen exports do not enter replaceable-preview settlement. Their completion and subsequent preview request keep their existing behavior.
- Allowed per-photo metadata/default updates and pixel retention can occur even when the current view rejects an image. Do not discard those effects merely because admission fails.
- Only a classified image destination with an actual image receives image admission. Errors and non-image outcomes retain their existing handling.
- Active-image acknowledgement differs from Overview delivery. Do not silently turn admission into a UI acknowledgement.
- Foreground progress also includes opening/export state. Return coordination facts without moving unrelated interaction state into the transaction.

Read [worker methods](../crates/foveonic-gui/src/rendering/worker/mod.rs), [UI receipt](../crates/foveonic-gui/src/main.rs), [delivery classification](../crates/foveonic-gui/src/rendering/delivery.rs) and [coordinator](../crates/foveonic-gui/src/rendering/coordinator.rs). This unit preserves serial engine execution, thread placement, numeric cache policy and pixels. The next review can move the consolidated coordination off the UI without also redesigning completion semantics.

### Validation

- [x] Capture all 40 frozen Full/Fast/Draft/Thumb baseline cases and confirm the unchanged baseline repeats exactly.
- [x] Pass 533 ordinary processor tests and 428 ordinary GUI tests. The three added handler cases cover current/superseded/sibling failures, permitted photo feedback despite view rejection, removed targets and frozen export routing.
- [x] Pass Clippy, formatting and the five artwork checks.
- [x] Pass the unchanged-build two-photo mixed Thumb rapid-supersession native scenario.
- [x] Compare all 40 after-change cases against the frozen baseline. Full-precision pixels, geometry and resolved settings remain exact.
- [x] Pass GPU checks for latest multi-edit Thumb delivery, including settled/admitted assertions, and Crop/Rotate reuse across Thumb, Smart fit and Smart Full without numerical dispatches.
- [x] Pass Quattro Fast-to-Full and Merrill Smart-to-Full export checks.
- [x] Pass the after-change two-photo mixed Thumb rapid-supersession native scenario.
- [x] Pass the after-change native cached-switch scenario through all switches and removal/re-addition.
- [x] Regenerate indexes and pass documentation and diff-whitespace checks.

The unchanged-build cached-switch native scenario timed out at step two, while one after-change run passed all switches and removal/re-addition. Retain both observations. One before/after pair does not establish a causal fix or performance improvement, and a pre-existing intermittent scenario issue remains possible. This unit does not diagnose or repair that baseline timeout. No latency improvement is claimed.

## Off-UI ownership proposal

Use **render coordinator** for the existing owner of render requests and results. Move that owner outside the UI thread, with the current serial worker consuming its work plans. Continue to use **request**, **job**, **result** and **handler** at this boundary. A **view context** describes what the viewport can show now. A **view update** is an image or status change for the UI to apply.

Before this unit, the worker handle let the UI mutate the coordinator and read state immediately. The result handler borrowed view facts from the UI. Progress and retained-photo switching read settlement synchronously, and the UI inventoried the coordinator's retained images. See the current [worker handle](../crates/foveonic-gui/src/rendering/worker/mod.rs), [view context](../crates/foveonic-gui/src/rendering/delivery.rs) and [memory accounting](../crates/foveonic-gui/src/memory.rs).

| Handoff | Proposed contract |
| --- | --- |
| UI to coordinator | Publish current requests and the relevant view context without waiting for a reply or queue capacity. Coalesce replaceable preview intent. Preserve ordered commands and acknowledgements that carry effects, including frozen exports and clearing a folder. |
| Worker to coordinator | Deliver results directly to the coordinator so settlement, reuse and plan updates can progress between UI paints. Keep result ownership and accounting per photo, including siblings in a multi-edit group. |
| Coordinator to UI | Deliver per-photo view updates and a small current status. Use the published context to admit images, then check current view compatibility and order when the UI applies one. Feed the actual Active display acknowledgement back to the coordinator. |
| Memory accounting | Count coordinator-retained images at their owner and combine them with the existing frontend inventory for budget enforcement. Preserve shared-resource deduplication and count images in transit. |

The risky race is between a UI action and a result already headed to the UI. Exercise it by changing exposure or selection after the coordinator admits an image and before the UI receives it. A compatible interim preview can still be displayed under the current rules. An incompatible image cannot replace the current view, and an older status cannot clear progress for newer work. A delivery acknowledgement refers to the image actually applied. Exercise the same handoff with two photo results ready together, a retained photo switch, folder clearing and a frozen export.

The owner approved continuing with this boundary. The implementation uses Crossbeam channels and selection. UI commands enter one queue without waiting for capacity. The coordinator applies queued commands in batches before publishing a replacement plan. Worker results and UI image updates each use a capacity-two queue. The coordinator holds at most one result while forwarding and selects new commands while UI delivery is full. A missing initial view context postpones image handling until the UI supplies it. Status is a small immutable snapshot, read with `try_lock` and the last observed value as fallback. A local command sequence keeps a newly submitted request busy until the coordinator acknowledges it.

Retained images stay with the coordinator. It supplies immutable snapshot handles to the worker, which inventories them together with frontend and in-transit images through the existing resource-accounting methods. The UI no longer reads coordinator internals or scans its retained images. Source acquisition, speculative planning and engine execution retain their current worker ownership for later review.

This unit adds one coordinator thread and Crossbeam channel selection. It uses ordinary typed commands and explicit handlers. It adds no custom broadcast, wakeup or subscription primitive. The existing worker plan's `Mutex`/`Condvar` stays between background threads. The command queue is unbounded and queued intent is coalesced at the coordinator, so sustained producer backlog remains a measurable limit of this first version. Image payloads have explicit queue bounds. The new `coordinator.command` and `coordinator.result` scopes use the existing Perfetto diagnostics.

### Off-UI validation

- [x] Pass six controlled threaded checks for replacement, independent sibling settlement, coalesced elapsed time, full UI queue handling/shutdown, nonblocking status reads and a newer UI edit overtaking an already coordinated failure.
- [x] Pass ordinary workspace tests and strict Clippy during implementation.
- [x] Compare the current processor build against all 40 frozen Full/Fast/Draft/Thumb cases. Full-precision samples, resolved settings and output dimensions match across three Classic, three Merrill, three ordinary Quattro and one Tiger image.
- [x] Pass GPU checks for latest multi-edit delivery, framing reuse, continuous dragging and scopes, viewport scope replacement, native-source clearing with reduced retention, color picking and three Full export paths.
- [x] Pass two isolated Off trials each of cached-switch and rapid-supersession against the previous checkpoint and the first off-UI build. Both builds pass all actions, including removal/re-addition. Repeat the current build after consolidating ready-image repaint notifications at the coordinator.
- [x] Pass an eight-photo native warm-edit scenario across Classic, Merrill and Quattro. The observed median over three warm edits is 116.44 ms for the group. Initial decoded-cache preparation is 2121.52 ms. These observations describe this build and workload rather than a demonstrated speed improvement.
- [x] Capture and analyze a loss-free native cached-switch Wall trace. All three unchanged switches perform zero source payload reads and zero photographic dispatches. The coordinator thread handles 58 command scopes and 12 result scopes. The maximum observed durations are 0.190 ms and 0.147 ms. The maximum observed UI update scope is 6.943 ms, with processing on the render worker. Physical display latency and arbitrary UI workload are outside those measurements.
- [x] Pass formatting, strict Clippy, documentation/index and artwork checks. Ordinary checks pass 533 processor and 432 GUI tests. Build and open the review app through `cargo xtask run` with the example-photograph folder.

The final delivery pass removes the render worker's direct image/scope/pick repaint notifications. The coordinator requests a repaint after placing a ready update in the UI queue. Command acknowledgements and memory-status changes retain their own notifications. This avoids waking the UI for a raw result that is still awaiting coordinator handling.

## Source acquisition map

Before the source-loading slice, the coordinator accepted per-photo render requests and published the current work plan. One render worker chose a job. It checked reusable presentation pixels, then compatible reduced or native processing sessions. A miss synchronously opened the decoded cache or read and decoded the original on that same worker, preventing it from choosing another render job. The slice preserves those reuse checks and moves requested acquisition to [source_loader.rs](../crates/foveonic-gui/src/rendering/source_loader.rs). See [worker dispatch](../crates/foveonic-gui/src/rendering/worker/mod.rs), [processing session lookup](../crates/foveonic-gui/src/rendering/worker/processing.rs) and [cache open](../crates/foveonic/src/decode_cache/mod.rs).

| Data | Current owner and readiness |
| --- | --- |
| Completed pixels and display textures | The render worker and coordinator retain compatible presentation snapshots. Reframing can use them without acquiring source data. |
| Reduced and native processing sessions | The worker processing owner maps verified photographs to native and reduced sessions in one shared working set. A session hit may allow a render without reopening the source. Both kinds share the memory and eviction policy. |
| Decoded disk entry | The decode cache tracks verified identity and disk readiness. A cache hit still reads, checksums and converts sensor codes into an in-memory `Source`. A green cache indicator describes disk readiness, not an in-memory processing session. |
| Original file | A decoded-cache miss reads and decodes the original. CPU decode uses the shared host-sized Rayon pool. Cache population can follow asynchronously. |
| Speculative preparation | The disk-cache worker prefetches a bounded neighborhood, admitting at most three photos concurrently to shared CPU decoding. Separately, the render worker may open a verified cached decode and prepare a reduced Thumb session while idle and within its memory budget. |

For file-backed sources, identity combines canonical path, byte length and modification time. The cache exposes a previously verified identity snapshot without touching the original file. A cold lookup now obtains identity from the loaded source rather than probing filesystem metadata on the render thread. The coordinator carries the known origin with a render task and checks source identity during admission. A source path can therefore have multiple identities over time.

Superseded render requests are replaced by the coordinator. The worker checks the current plan during graph execution. The foreground `Cache::open` call has no render-request cancellation argument, so an obsolete foreground load can finish on the source thread. Coordination discards it when no longer wanted. Speculative prefetch has separate generation and foreground-interruption checkpoints. These are distinct lifecycles.

The approved interface distinguishes ready source data, retained processing sessions and reusable presentation pixels. The renderer asks for missing input and defers that job without emitting a completion. Coordination admits loads for currently wanted paths and publishes successful or failed handoffs. A separate source thread does cache I/O and calls existing decoding facilities. At most one decoded source waits for consumption, and the next load starts once that handoff is consumed. Existing native and reduced input retention remains with the renderer. Optional idle acquisition and cancellation of an already-started read remain follow-up decisions.

### Reusable presentation ownership

The owner questioned why a request for an existing display texture must pass through the render worker. The coordinator already retains eligible per-photo snapshots and the UI holds displayed frames. The worker also retains a whole-image presentation and currently owns compatibility checking, composition and texture preparation. These owners share GPU resources through reference-counted handles. A matching presentation bypasses graph construction and photographic operations. If the existing display texture matches the framing, it is shared. A different view can require a new GPU texture from the retained image.

An older native Thumb crop capture, before the off-UI coordinator change, recorded one existing-texture reuse at 0.014 ms worker-queue wait, 3.07 ms worker elapsed and 7.97 ms from queued request to UI receipt. It issued no photographic or display GPU dispatches. These overlapping observations cover one interaction and do not isolate the worker handoff cost. A separate Smart framing interaction in that capture set waited about 83 ms for the worker and needed a new texture, whose GPU completion wait was about 37 ms. This shows why queue ownership and texture materialization must be measured separately. The current coordinator implementation needs a matched trace before claiming its latency or savings.

Review whether the coordinator can decide that an existing texture and its presentation metadata satisfy a view request, then deliver it directly. Keep source identity, revisions, framing, memory accounting, scopes and UI acknowledgement explicit in that review. A request requiring a new texture or new photographic data still needs the corresponding execution owner. This question affects the source-readiness boundary because presentation and processing-session hits can avoid source acquisition entirely.

The current decoded disk cache uses a versioned JSON header, checksums, signed or unsigned 16-bit sensor samples and retained camera metadata. It reconstructs floating-point samples in memory. An uncontended OS-cache-warm eight-sample test measured the Merrill hazy-coast entry at 98.5 MB and 19.1 ms median to open, including 17.0 ms for payload reading, checksum and conversion. An earlier in-memory conversion microbenchmark measured 4.9 ms median for allocation and conversion of that photo's 16-bit samples to floats. That isolated benchmark is not a subtraction from the streamed cache-open time. Both measurements depend on hardware, cache state and image size.

## Conceptual pass before each unit

Describe the affected concept in plain language before choosing types or moving files. For each term, record its meaning, authoritative owner, input/output and distinction from neighboring terms. Map existing symbols to those meanings, identify synonyms and overloaded names, and review the proposed words with the owner. Use the [shared vocabulary](../docs/architecture/concepts.md) consistently in the plan, contracts, code and tests as that unit is implemented. A vocabulary entry is not a demand for another struct or lifecycle layer.

For the first unit, use the agreed request, job, result and handler meanings, with render qualifying the domain as needed. Image remains the ordinary output term. Keep request settlement, reusable image retention, view admission and UI application as observable effects of handling a result rather than calling all of them completion. Rename current callers when their agreed concept changes, without unrelated project-wide renaming or compatibility aliases.

## Provisional sequence

The rows describe likely boundaries, not authorization to implement several at once. Split or reorder them when the current slice exposes a better complete unit.

| Concept | Intended improvement | Review checkpoint |
| --- | --- | --- |
| Result handling | Replace caller-assembled coordination with one operation and explicit UI effects. | Existing current/superseded/removed/source-changed result behavior, metadata feedback, exports, sibling progress and acknowledgement. |
| Runtime ownership | Move coordination, planning and availability/resource decisions off the UI. Publish the latest requested set and per-photo display/status updates without blocking either side. | Real-app input under worker load and planning without a repaint. Engine stays serial. |
| Requested state | Make authored per-photo edits authoritative in one existing owner and derive requests for one or many targets consistently. | Relative edits, selection switching, Import/Browse persistence and Before. No accidental active-photo settings propagation. |
| Execution control | Make priority, cancellation/yield and completion policy explicit at current boundaries. Include auxiliary GPU producers. | Foreground arrival during speculation, dependency promotion, bounded GPU backlog and resource overlap. |
| Collection journey | Exercise repeated groups across an owner-reviewed 100-photo collection. | Total cull/edit readiness and UI response, with actual cache/source states recorded. |

The engine keeps its current serial execution contract. Execution cursors, stage interleaving, grouped submissions/completion waits and multi-image shader dispatches belong to the separate wide-rendering investigation. A future step may revisit any high-level ownership boundary, but numerical node optimization, loupe and focus detection are outside this plan.

## Architecture-specific options follow-up

The owner requested a focused design pass on camera-architecture option support. Known unsupported option combinations should produce an error before execution. Shared nodes should receive coherent option groups rather than individually checking unrelated fields from every architecture. Evaluate architecture-typed option objects, with mutually exclusive camera groups represented together, while retaining shared controls where their meanings actually match.

- [ ] Map authored settings, capability validation and resolved node parameters across direct, retained-session and reduced-session entry points.
- [ ] Agree on one validation/resolution boundary and names for common versus architecture-specific control groups.
- [ ] Evaluate typed architecture variants that prevent invalid combinations without distributing per-parameter architecture checks through nodes.
- [ ] Cover supported and rejected combinations across entry points, including shared nodes and transient scale/region requests.

Distinguish stored authored choices from resolved active node options. Inactive values can legitimately remain saved when switching rendering modes or methods. They must not be mistaken for supported active edits in tests. The profiling fixtures now separate color reconstruction from monochrome controls and omit Universal HDR/local-contrast edits from Reference-tone recipes.

The checkpoint investigation exposed a concrete inconsistency: a Quattro highlight-amount setting was ignored by Merrill session execution but rejected by direct rendering. Its fixture now uses Merrill's threshold control and validates authored choices before adding transient execution geometry. Keep this production consistency issue on the docket rather than extending the profiling slice into an option-model rewrite. Preserve the separate alpha-sidecar policy for unknown JSON fields and missing defaults. That policy does not authorize silently ignoring known controls unsupported by the selected architecture.

## Evidence per slice

1. Complete the conceptual vocabulary pass, then record the approved concept/name, old and new owners, affected callers, behavior preserved and expected benefit. State what is deliberately unchanged.
2. Freeze the build, input identities and resolved settings before edits. Use maintained `profile_pipeline --goldens` checks on three Classic, three Merrill, three ordinary Quattro and one available Tiger image, covering Full/Fast/Draft/Thumb. Confirm that the unchanged baseline repeats exactly. Use the corpus recorded in the [local cleanup acceptance](./render-coordination-consistency.md#local-round-acceptance-gates). No numerical tolerance replaces exact equality.
3. Choose existing deterministic and native checks for the boundary being moved. Use the [implemented map's test locations](../docs/interactive-rendering-architecture.md#where-policy-is-distributed). Keep pixel equality, result identity/settlement, work avoidance and latency as separate claims.
4. When making a performance claim, run repeated matched Off scenarios and separate Wall/GPU attribution through the existing [profiling tools](../docs/profiling.md). Check for competing app/compiler workloads before runs and record interference. Report first-result and complete-set latency, variability, work counts and retained/transient memory. Do not add overlapping waits to GPU time.
5. Inspect UI-facing methods for I/O, processing, GPU waits, expensive scans, joins and blocking sends. Reuse existing UI update/frame-gap scopes and test input while workers are occupied. Test display-ready results separately from physical screen presentation.
6. Run applicable ordinary checks and the repository documentation/index checks. Explain removed coordination points and new concepts, including source-size changes. Launch a review build when useful. Wait for owner review before the next conceptual unit. Commit only with approval.

No new test framework is needed. Existing retained-versus-fresh verification remains useful. Performance checks for this migration measure the same serial engine under clearer application coordination.

## Existing evidence and gaps

The [overhead investigation](./coordination-overhead-profile.md) separates warm edits, cold-processing selection and prepared selection. Historical warm eight-photo Wall measurements include about 3.27 ms graph planning, 20.47 ms graph completion waits and 23.09 ms display completion waits. These are inclusive, overlapping observations, not recoverable savings. They justify testing completion boundaries separately from graph traversal.

Source findings about per-operation waits, internal GPU submissions and the single protected reduced session are retained in the [future wide-rendering investigation](./wide-rendering.md). They do not require engine restructuring in this ownership migration.

Record these harness changes with the slice that first needs them:

- Native scenarios currently accept one to eight photos. Add a journey through groups for the 100-photo goal rather than changing the editing limit.
- Scenario readiness currently includes `worker_busy == false`. As background ownership changes, completion must mean the requested foreground set settled, not global worker idleness.
- Add controlled foreground input during speculative work when execution control changes. Measure interruption at actual safe boundaries, not inferred priority from a thread label.

## Communication approach and primitive use

Start with fixed typed messages and explicit calls to known handlers. The coordinator can apply an update and call foreground and background planning against the same state. Full publish/subscribe is not required merely because two handlers need an update. Preserve the existing cross-thread queues in the first unit.

For each implemented unit, record application glue added, any ready-made primitive that would provide it, and why the selected approach is simpler for the current contract. The owner requires an established library if full broadcast/watch behavior becomes necessary. Do not build a subscriber registry, broadcast queue or notification primitive ourselves. Independent subscribers would require defining current-state versus event delivery, late subscription, slow-consumer behavior, capacity and shutdown. Also review any custom waiting/wakeup bridges and synchronization stalls on UI publication.

First result-handler unit, observed primitive use:

| Application work | Primitive use |
| --- | --- |
| Call `RenderCoordinator::handle_result` with a typed result and borrowed view context. | Ordinary function calls and existing decision types. No custom concurrent primitive or subscription registry. |
| Return `ResultUpdate` containing current-request, photo-feedback and destination facts. | One small application return struct. A transport library does not decide these application-specific effects. |
| Keep the bounded `std::sync::mpsc` image reply queue. | Existing library transport is unchanged. |
| Keep the latest `Pending` plan under `Mutex`/`Condvar`. | The existing application-owned publication/wakeup protocol remains. Plan publication is consolidated to one call per non-export result. |
| Consolidate result decisions at the coordination owner. | No dependency, async runtime, handler registry, notification bridge or independent subscriber added. |

The unit adds zero custom synchronization primitives. Libraries would provide transport and subscription mechanics, but the application still needs to decide request settlement, reuse and view updates. This first unit needs ordinary typed calls and a small return struct. It retains the existing hand-written publication/wakeup protocol and does not yet migrate the off-UI event loop or foreground/background handler wiring. Therefore it does not establish that the broader communication design is cheaper or simpler than a library. Evaluate that fit in the next relevant unit, including whether the existing protocol should be replaced.

Deferred library candidates, if independent subscriptions become necessary:

- Tokio's [`watch`](https://docs.rs/tokio/latest/tokio/sync/watch/index.html) retains the latest state with independent receiver observations. [`broadcast`](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) retains a bounded event history and reports lagging receivers. Its [synchronization primitives](https://docs.rs/tokio/latest/tokio/sync/index.html#runtime-compatibility) are runtime agnostic except timeout methods that need its timer. Using channels does not itself require migrating the application to the Tokio runtime.
- [`async-broadcast`](https://docs.rs/async-broadcast/latest/async_broadcast/) supplies standalone broadcast channels with configurable overflow and inactive receivers. Its sender/receiver APIs include blocking and nonblocking operations, making it a candidate for existing dedicated threads.

Both packages already appear transitively in the workspace lockfile, which is not a direct dependency choice or proof of architectural fit. No package was added, installed or selected. Crossbeam remains a possible work queue. Revisit these candidates only when an actual communication boundary needs them.

## Delegation and integration

The integrating agent owns this plan, shared vocabulary and interface decisions. Delegate source mapping, bounded implementation or independent review within the current approved unit. Each assignment states the contract, exclusive files/worktree, non-goals, required evidence and return artifact. Agents may identify a better boundary but must report it before expanding scope. They do not make separate concept or naming decisions.

Use separate worktrees for overlapping implementations and one shared absolute `CARGO_TARGET_DIR`. The integration agent coordinates builds and runs GPU measurements serially on a quiet machine. Do not cherry-pick an implementation whose input/output contract has drifted. Parallel work is useful inside the approved concept, not permission to implement later rows or the separate wide-rendering effort early.

After each unit, update this plan with its source checkpoint, accepted name/owner, files/callers changed, exact and behavioral checks, measured result or lack of claim, lesson learned, remaining question and next action. Keep only one active unit. Update concepts and the implemented map when they become true, and close the corresponding open question.

## Progress

The expanded [behavioral acceptance checks](./behavioral-testing.md#pending-edit-re-add-failure) exposed a pending edit that was admitted but skipped at the final UI view check, then coalesced without re-delivery when its photo was re-added. The owner approved focused result-retention and cache-off browser-preview fixes. Both now pass their original cases and related selection, Smart, Before/After and lifecycle checks. These changes retain the current coordinator and execution boundaries. See the behavioral plan for validation before choosing the next ownership slice.

- [x] Record the owner's six goals and incremental review/delegation requirements.
- [x] Review current source seams and existing profiling evidence with three read-only agents.
- [x] Establish the architecture documentation starting structure and replace the earlier speculative type catalog with a smaller provisional model.
- [x] Separate wide rendering from the current ownership migration and require a conceptual vocabulary pass for every unit.
- [x] Agree on request, job, result and handler as familiar reusable terms, with render identifying the domain and image naming its output.
- [x] Approve result handling at the existing coordination owner and try fixed typed messages/direct handlers before adding messaging infrastructure.
- [x] Implement the first result handler and record its primitive use without changing serial execution, queues or thread placement.
- [x] Complete code/image/application validation and prepare the unit for owner review before selecting the next concept.
- [x] Obtain owner approval to checkpoint result handling and continue with off-UI ownership.
