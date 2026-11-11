---
summary: "Clarify render ownership and responsibilities through one owner-reviewed concept at a time while preserving serial engine execution."
---

# Render runtime migration

Requirements: [render orchestration](../docs/specs/render-orchestration.md), [interactive rendering](../docs/specs/interactive-rendering.md), [multi-edit](../docs/specs/multi-edit.md), [preparation](../docs/specs/render-preparation.md) and [diagnostics](../docs/specs/performance-diagnostics.md). Start discussions at the [architecture guide](../docs/architecture/AGENTS.md). Current code ownership stays in the [implemented map](../docs/interactive-rendering-architecture.md).

## Resume here

- **Current phase:** the owner accepted the first result-handler slice and asked to continue with off-UI ownership.
- **Checkpoint:** the result-handler slice follows `fd674d0` and includes the reviewed local matching and readiness simplifications. Its engine execution and thread placement are unchanged.
- **Current unit:** define the boundary for moving render coordination off the UI thread.
- **Decided:** protect UI responsiveness, use one runtime for individual and multi-photo editing, isolate responsibilities and support subordinate speculative work. Keep engine execution serial throughout this migration.
- **Separate future effort:** [wide rendering](./wide-rendering.md). Clear application/engine ownership should make that experiment easier, without building its execution machinery here.
- **Not decided:** concrete new Rust types, full state migration or exact admission/backlog limits.
- **Communication direction:** ordinary typed messages, fixed consumers and directly called handlers. Keep existing library queues across threads. Add a broadcast/watch dependency only when an actual consumer needs the full primitive.
- **Next action:** map the current UI dependencies on the coordinator and review the proposed off-UI contract with the owner. Preserve the cached-switch baseline limitation below. Update this section after every accepted slice.

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

- [x] Record the owner's six goals and incremental review/delegation requirements.
- [x] Review current source seams and existing profiling evidence with three read-only agents.
- [x] Establish the architecture documentation starting structure and replace the earlier speculative type catalog with a smaller provisional model.
- [x] Separate wide rendering from the current ownership migration and require a conceptual vocabulary pass for every unit.
- [x] Agree on request, job, result and handler as familiar reusable terms, with render identifying the domain and image naming its output.
- [x] Approve result handling at the existing coordination owner and try fixed typed messages/direct handlers before adding messaging infrastructure.
- [x] Implement the first result handler and record its primitive use without changing serial execution, queues or thread placement.
- [x] Complete code/image/application validation and prepare the unit for owner review before selecting the next concept.
- [x] Obtain owner approval to checkpoint result handling and continue with off-UI ownership.
