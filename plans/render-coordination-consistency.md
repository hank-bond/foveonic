---
summary: "Inventory coordination, scheduling and cache inconsistencies before choosing small current-architecture cleanup slices."
---

# Render coordination consistency

Requirements: [interactive rendering](../docs/specs/interactive-rendering.md), [multi-edit](../docs/specs/multi-edit.md), [small previews](../docs/specs/small-previews.md) and [render preparation](../docs/specs/render-preparation.md). This audit continues [latency stabilization](./multi-edit-latency.md). The [implemented architecture map](../docs/interactive-rendering-architecture.md) describes current ownership. The [orchestration proposal](../docs/specs/render-orchestration.md) remains a design direction, not authorization to replace the runtime.

The local cleanup round below is finished. The owner has opened the [render runtime migration](./render-runtime.md), which now owns the active conceptual review and continuation plan. Keep this source inventory and its unselected candidates as supporting evidence rather than another implementation queue.

## Scope and status

The owner requested a broad inventory before choosing implementation slices. The earlier request-policy extraction is one candidate, not the selected first step. Audit current responsibilities, duplication, names, lifecycle contracts and reuse boundaries. Keep numerical node optimization separate. Include shared pipeline intermediaries, preparation and graph reuse where they concern coordination rather than image arithmetic.

This inventory is based on source inspection at `0fa7bbe`, with no production changes. The existing ordinary GUI baseline passes all 420 tests. The preceding downsampling checkpoint includes exact numerical and source-free coverage. This audit adds no new runtime or latency measurements. An observed structural concern is not proof of a user-visible defect. The reported Dehaze slowdown still needs controlled attribution.

The [coordination overhead investigation](./coordination-overhead-profile.md) separately measures data movement, preparation, waiting and delivery to help decide what can improve within the current architecture.

The owner subsequently narrowed the next rounds to local improvements whose correctness follows from existing contracts. Keep the broader ownership, lifecycle and vocabulary questions for architecture design. Use read-only subagent reviews across bounded code areas to identify candidates, then review concrete findings before selecting fixes. The broader sequence below remains context rather than authorization to reshape those boundaries now.

### Local-round acceptance gates

Each selected round must preserve bit-exact rendering on three images per architecture. Tiger has one available sample, which is the explicit coverage exception. Freeze input identities, recipes and relevant Full/reduced preview grids before editing production code. Compare full-precision output samples and image geometry with the unchanged baseline through the maintained rendering/golden tools. Confirm the baseline itself repeats exactly. Do not replace a failed baseline or substitute numerical tolerance. Retained-result and request/delivery checks remain necessary alongside image equality when those paths change.

The selected corpus uses the existing example photographs: Classic sd9, sd10 and sd14; Merrill blue-parking-sign, merrill-hazy-coast and low-light-speaker-and-desk; ordinary Quattro dp0-quattro, dp2-quattro and sd-quattro; Tiger sd-quattro-h. The Tiger sample has processing version 4.0.1.0, CAMERAID 41 and sensor extent 6656×4480. Capture all four Full, Fast, Draft and Thumb grids with CLI defaults plus explicit `--wb Sunlight --mode Standard`, independent of owner sidecars. Record input SHA-256 identities, resolved settings, output extents and full-precision sample digests in the capture manifest.

Every change must simplify. Prefer removing code, branches, duplication and repeated work. A small source increase is acceptable only when it removes conceptual complexity or makes an existing invariant explicit. Do not introduce a manager, wrapper, generic worker or new cache/scheduling policy to implement a local fix. Report code/branch changes and the removed responsibility or repeated work. Keep performance claims separate from proven control-flow redundancy until measured.

### First read-only local audit

Three subagents inspected GUI rendering, application/auxiliary integration and processor host bookkeeping at `fd674d0`. The coordinating review checked the strongest findings against the source. No production edits, builds or GPU runs were made during the audit. The following are candidates for owner review, not selected implementation.

| Candidate | Evidence and smallest change | Simplification and checks |
| --- | --- | --- |
| Identical-delivery matching | [matching.rs](../crates/foveonic-gui/src/rendering/matching.rs), `same_delivery_request`: exact settings and quality equality already imply the pixel/geometry/region comparisons performed by `can_share_pixel_work`. Compare context, framing flag, exact settings and quality directly. Keep source admission unchanged. | Similar source length, fewer nested decisions and two fewer settings clones. Existing matching/coalescing checks plus focused equality-boundary checks. Strong first-round candidate. |
| Multi-edit readiness filtering | [multi_edit_runtime.rs](../crates/foveonic-gui/src/multi_edit_runtime.rs), `apply_edits`: the group-level filter requires every target to accept an edit, then the per-photo loop repeats those same predicates before modifying that target. Remove the second filter and use the already-approved edits. | About 32 deleted lines, one readiness decision, no per-photo filtered Vec or redundant camera-resolution clones. Check pending defaults, dependent controls, relative edits and mixed-camera behavior. Strong first-round candidate. |
| Borrow presentation resources | [presentation.rs](../crates/foveonic-gui/src/rendering/worker/presentation.rs), `Snapshot::memory_resources`: a complete `Whole` is cloned only to build a temporary `Presentation` for a borrowed inventory loop. Put that loop on `Whole` and borrow it from both existing callers. | Roughly neutral source length. Removes a fabricated owner and settings/string/vector copies. Verify identical reservation identities and deduplication. This does not change snapshot ownership. |
| Metadata pointer comparison | [preparation.rs](../crates/foveonic/src/render/preparation.rs), `same_metadata`: upgrading a Weak only to compare allocation identity can use its pointer directly against the supplied Arc. The live Arc and retained Weak preserve the required allocation identity guarantees. | Shorter helper and no temporary strong-reference traffic. Check live, distinct and expired metadata identities. Keep weak ownership and cache policy unchanged. |
| Repeated decoded-bundle path derivation | [lru.rs](../crates/foveonic/src/decode_cache/lru.rs), `priorities`: hash/format/join the same identity once, then derive the original/metadata extensions from that path. | Removes two identity-name computations and root joins without a helper. Keep the separate preview identity lookup and source-replacement behavior. Existing complete-bundle and budget-order tests. |
| Redundant preview metadata lookup and navigator guard | [previews.rs](../crates/foveonic/src/decode_cache/previews.rs), `save_preview`, resolves the same immutable family twice. [navigator.rs](../crates/foveonic-gui/src/navigator.rs), `update`, repeats a sent-key condition already excluded by an early return. | Delete one binding and one branch. Low-impact hygiene with existing preview/navigator checks. No material latency claim. |

Two additional candidates need separate review: shortening the pending-plan lock around immutable request interpretation, and skipping unchanged/under-budget history enforcement. They can remove contention or repeated inventories but add control-flow guards and require stronger concurrency/retention reasoning. Do not bundle them into the deletion-oriented first round.

The coordinating review also confirmed that some recipe/session preparation timers depend on aggregate profiling mode and therefore disappear from shared-only capture. Repairing that coverage remains useful, but is deferred behind the simpler local candidates rather than adding instrumentation plumbing to this first round.

### Completed local simplifications

The owner selected identical-delivery matching and multi-edit readiness filtering, then requested that this cleanup round stop. Other audit candidates remain unimplemented.

- [x] Compare context, framing flag, exact settings and quality directly in `same_delivery_request`. This removes two settings clones and the nested pixel-reuse decisions. Keep source identity and admission with their existing owners. Extend existing checks to isolate framing-only and quality-only delivery differences.
- [x] Use the group-approved edits in each photo's application loop. Readiness is decided before any target changes, so the second filter repeats the same predicates. Remove its allocation and 32 net production lines. Keep individual defaults, Auto exposure materialization, supported ranges, normalization and persistence in that loop.
- [x] Freeze the unchanged `fd674d0` rendering tool and capture 40 cases before production edits. Repeat all 40 exactly against those full-precision goldens, including geometry, resolved settings and sample digests.
- [x] Rebuild and compare all 40 cases after the cleanup. Full-precision pixels, geometry and resolved settings remain exact. The image gate supplements request/coalescing and multi-edit behavior checks because these changes are in the GUI.
- [x] Pass all 533 ordinary processor tests and 425 ordinary GUI tests, including pending defaults, dependent manual controls, mixed-camera clamping, relative edits and individual resets.
- [x] Pass the existing GPU multi-edit Thumb delivery check, all 425 GUI tests against the final source, and final formatting, Clippy, documentation/index and artwork checks.

The production diff removes duplicated decisions without adding an abstraction, cache policy or scheduling owner. The redundant work is established by control flow. This round makes no measured latency claim.

- [x] Trace active-photo, multi-photo, Smart, presentation-only, inspection and export routes.
- [x] Inspect idle preparation, decoded-cache prefetch, browser previews, navigator, scopes and picks.
- [x] Inspect source acquisition, numerical checkpoints, shared preparation, memory accounting and invalidation.
- [x] Inspect completion, cancellation, priority, progress, persistence and worker lifetime contracts.
- [x] Compare the source map, proposed architecture and existing test boundaries.
- [x] Review priorities with the owner and approve the proposed staged cleanup sequence.
- [ ] Resolve each unit's remaining contract questions before changing its behavior.
- [ ] Implement only the selected slices, with appropriate checks and an app-review checkpoint between meaningful behavior changes.

Inventory order below is organizational, not implementation priority. Preserve existing thread, channel, coordinator and cache owners while evaluating small cleanups. Do not treat all repeated worker code as a reason to build a universal worker framework.

## Proposed cleanup sequence

The owner clarified the order of priorities after the source audit and runtime investigation:

1. Simplify the current design and make ownership, terminology and control flow coherent.
2. Remove unnecessary waits, contention, repeated work and coordination overhead while preserving the experience.
3. Consider improved cache concepts and behavior later. Prewarming is useful evidence, not the focus of this cleanup.

Future multi-photo execution should advance a group through each pipeline stage, rather than completing every stage for one photo before starting the next. Keep operation implementation separate from the policy that advances work so this can be introduced later. Do not implement group execution, a new scheduler or speculative batching interfaces now.

The passes below are a proposal for owner review. Each contains smaller reviewable units, not one large commit. Preserve ordinary behavior during extraction. Isolate behavior fixes and performance changes from moves and renames. Update the implemented architecture map as boundaries actually change.

| Order | Proposed changes | Existing owners and review boundary |
| --- | --- | --- |
| 1. Request meaning and shared decisions | Move generic edit/geometry comparisons out of Smart. Name authored-edit equality, pixel reuse, compatible continuation and delivery acceptance separately. Consolidate repeated purpose/grid interpretation where callers make the same decision. Keep requested quality distinct from actual rendered scale. | Existing request, Smart and delivery owners remain. First review comparisons alone, then request resolution. Covers B1–B5, A2 and H4. Preserve differences instead of replacing them with one broad equality predicate. |
| 2. Settings and preparation boundaries | Document authority for active controls, per-photo requested edits and sidecar persistence. Give camera interpretation, effective settings and numeric/reset defaults clear homes. Remove the worker's dependency on multi-edit UI preparation. Consolidate foreground/idle precedence rules while preserving their different access permissions. | Existing App/runtime/sidecar owners remain. Review default preparation separately from state handoffs. Covers A1–A5, E3, E6–E8 and H3. Do not introduce a replacement application state store or shared cross-executor cache. |
| 3. Readable worker execution | Separate request/resource selection from source acquisition, numerical execution, display preparation and reply construction. Use explicit alternatives for presentation reuse, reduced execution and native execution where parallel options currently encode those states. Remove confirmed unreachable branches. Keep service order visible in one place and keep helpers that perform work independent of the pending-plan lock. | Existing worker, fair cursor, engine and caches remain. Extract one operation at a time while preserving ordering and cancellation. Covers C1–C4, C6 and H3. Export remains frozen work. This is not a generic task framework. |
| 4. Result and lifecycle consistency | Name rendered, reused, superseded, exported and failed outcomes. Separate task settlement, result admission and UI effects. Make publication occur at deliberate boundaries. Define requested/attempted/successful settings explicitly wherever continuation depends on them. Give progress, execution activity and outstanding demand distinct meanings. | Existing coordinator and delivery classifier remain. Review outcome naming first, delivery ordering second, then progress derivation. Covers D1–D4 and E8. Preserve compatible older feedback and per-photo retained edits. |
| 5. Resource and auxiliary consistency | Separate resource reporting from retention decisions. Share allocation-counting primitives where scopes genuinely match. Make resource-change invalidation explicit at each owner and consolidate repeated clear/configuration fanout. Clarify decoded, reduced, checkpoint and display readiness and reuse domains. Give each auxiliary GPU producer an explicit priority/completion contract. | Existing memory, decoded-cache, presentation and auxiliary worker owners remain. Review memory accounting, invalidation, then auxiliary lifetimes separately. Covers E1–E2, E4, E6–E7, E9, F1–F4 and G1–G5. Preserve cache limits, access recency and frozen-write behavior. Do not merge every worker or cache into one abstraction. |
| 6. Measured overhead reductions | Through the clarified boundaries, separate cancellation checks from GPU completion policy. Test combining compatible submissions before necessary completion. Remove redundant accounting and preparation only where measurements prove equivalent work is repeated. Check auxiliary GPU interference and shared-intermediary reuse, including Dehaze enable/disable. | Retain serial photo execution and existing queue/cache ownership. One behavior change and matched measurement per unit. Covers C5, F2, G2, E5–E6 and E9. Completion changes must preserve responsiveness and bound temporary overlap. |

Diagnostics support these passes rather than forming a separate redesign. Close the shared preparation-scope gap in H1 using the existing recorder, and maintain a small behavior-to-evidence matrix for H2. Add meaningful checks for the boundary being changed. Do not duplicate the entire audit as new tests or build another profiling protocol.

### Review and acceptance

Start with pass 1's comparison ownership and names, plus the confirmed dead branches as a separate mechanical unit. This makes the existing decisions easier to inspect before changing state handoffs. After those units, review request resolution before undertaking pass 2. Dependencies may justify moving one small prerequisite earlier, but do not mix a new completion policy into a mechanical extraction.

For each unit, identify what rule now has one home, which callers migrated and which observable behavior is preserved or intentionally corrected. A file split alone is insufficient. Keep numerical implementations unchanged. Use existing deterministic checks, cached-versus-fresh comparisons and maintained native scenarios as applicable. Offer an app-review checkpoint after meaningful interaction changes before continuing across another boundary.

For pass 6, compare matched Off workloads before and after and use separate Wall/GPU captures for attribution. Check one-photo and multi-photo edits, rapid supersession, foreground arrival during background work and memory pressure where affected. Do not trade slower cancellation or unbounded GPU backlog for a faster completed-render number.

### Vocabulary and source organization

The owner approved physical module/file reorganization after committing the request-policy checkpoint to obtain a clean tree. Move the GUI request, matching, coordinator, delivery and Smart modules under `rendering/`. Keep execution-specific presentation, reduced-session and preparation adapters under `rendering/worker/`. Preserve behavior and visibility during this mechanical unit, then continue settings/default-preparation cleanup.

Resolve broader concept names together during the new architecture design. Current extraction names are working names, not a settled architecture vocabulary. The owner suggested `AppDefault`, `UserDefault` and `Current` or `Requested` as clearer candidates for the existing Before/edit choices. These remain candidates. Distinguish which settings are used from the lifecycle of a render request before choosing the final terms. Do not implement isolated enum renames during this cleanup.

Use [UIX's documentation strategy](https://github.com/hank-bond/uix/tree/main/docs) as design inspiration. Its [documentation layers](https://github.com/hank-bond/uix/blob/main/docs/AGENTS.md) separate open design, accepted decisions, specifications, implementation plans and living architecture. Its [concept vocabulary](https://github.com/hank-bond/uix/blob/main/docs/architecture/concepts.md) and [knowledge-placement guidance](https://github.com/hank-bond/uix/blob/main/docs/contributing/knowledge-placement.md) connect stable terms and durable knowledge owners. Review which practices to adopt alongside architecture design. A repository-wide documentation migration has not been selected.

### Deferred changes

New preparation coverage, prewarming heuristics, cache formats, cross-executor shared caches, revised eviction policy, centralized CPU/GPU scheduling and stage-grouped multi-photo execution belong to later decisions. Existing cache correctness and duplicate work can be fixed within the passes above without introducing these concepts. Keep the wider orchestration proposal as the destination for ownership changes that exceed the current boundaries.

### First implementation checkpoint

- [x] Give settings/request comparisons one home in [matching.rs](../crates/foveonic-gui/src/rendering/matching.rs). Migrate every caller from Smart, worker and presentation helpers. Retain distinct authored-edit, image-geometry, pixel-settings, compatible-work and exact-delivery rules.
- [x] Keep transient scale/region/estimate and descriptive range-name normalization in one private helper. Preserve framing, output-size, workspace-policy, calibration-snapshot and source-admission distinctions.
- [x] Remove the reduced renderer's unreachable native-session lookup after `native.take()`, along with redundant assignments of that already-empty slot. Preserve taking the matching source for refresh and releasing native preparation.
- [x] Remove export-only alternatives from preview execution after the early export return. Keep frozen export behavior and the separate export path unchanged.
- [x] Pass all ordinary workspace tests, including 423 GUI checks, and four existing GPU checks covering framing reuse, reduced refresh/native promotion and Merrill/Quattro TIFF/DNG export round trips. Export checks compare independent native outputs exactly. Framing/reduced checks verify fresh pixels and work avoidance at their existing boundaries.
- [x] Pass workspace Clippy with warnings denied, formatting, generated documentation/index checks, diff whitespace checks and the five artwork checks.
- [x] Commit the comparison/dead-branch checkpoint as `063617d` before continuing with request resolution.

The inventory below records the original source findings at `0fa7bbe`. This checkpoint addresses comparison ownership in B1 and C6's dead branches. Remaining purpose/grid resolution, request-type ownership, cancellation policy, cache behavior and delivery lifecycles are unchanged.

### Request policy checkpoint

- [x] Move `Request` and `Purpose` into [request.rs](../crates/foveonic-gui/src/rendering/request.rs). Migrate callers directly instead of retaining worker/coordinator re-exports.
- [x] Derive coordinator targets through `Request::purpose()`. Preserve precedence among export, Before renditions, inspection/comparison and detail.
- [x] Put nominal working scale, camera-aware preview application and reduced-session selection beside the request. Remove these decisions from worker/resource-owner helpers. Preserve Smart's native history and camera/default Before previews at reduced quality.
- [x] Share camera-aware scale selection between foreground requests and idle preparation through the existing core capability policy. Keep framing-plane reuse at its existing actual grid and keep request metadata independent of graph cache identity.
- [x] Move the five existing purpose/scale/eligibility tests alongside the request contract. Add checks for mixed-purpose precedence and the independence of Before rendition and processing resolution.
- [x] Pass 425 ordinary GUI checks and five existing GPU checks for multi-photo Thumb delivery, framing reuse, pending-exposure composition, explicit quality changes and Merrill/Quattro TIFF/DNG export round trips. Pass workspace Clippy with warnings denied, formatting, generated indexes/documentation, artwork and diff checks.
- [x] Pass the full ordinary workspace run, including 531 processor tests and 425 GUI tests. This checkpoint is ready for review, with scheduling, cache eviction and numerical processing unchanged.
- [x] Owner reviewed the request-policy checkpoint and approved committing it, reorganizing the render modules from a clean tree, then continuing cleanup.

### Module organization and numeric preparation checkpoint

- [x] Commit request-policy cleanup as `1a696d7` and verify a clean tree before moving files.
- [x] Group request, matching, coordinator, delivery and Smart under `rendering/`, with execution adapters under `rendering/worker/`. Use ordinary Rust module layout, move adjacent tests, migrate callers and update documentation links/indexes. Preserve the previous effective visibility of Smart helpers.
- [x] Pass 425 GUI tests and workspace all-target Clippy after the mechanical move. Keep application-wide diagnostics, progress and memory inventory outside this module.
- [x] Move numeric control/reset preparation from multi-edit runtime to the processor's existing [editing-settings code](../crates/foveonic/src/render/edits.rs). Export the existing `numeric_settings` helper through the renderer and migrate its worker caller directly.
- [x] Keep authored automatic choices separate from derived numeric/reset snapshots. The worker still supplies render-produced Dehaze estimates. Preserve absent-value/error handling in this unit. Add checks for Auto exposure, disabled automatic sharpening, authored-value preservation and manual/reset separation.
- [x] Pass the full ordinary workspace suite after extraction, including 533 processor tests and 425 GUI tests.
- [x] Pass workspace all-target Clippy with warnings denied, formatting, documentation/index checks, artwork tests and diff checks. Pass three existing GPU checks for multi-edit Thumb delivery and Merrill/Quattro TIFF/DNG export round trips. Verify the moved numeric-preparation body matches its previous implementation after namespace changes.
- [x] Owner reviewed the combined checkpoint and approved committing it before local-improvement audits. Further settings-state handoff changes remain deferred.

This addresses A3/H3's worker-to-UI preparation dependency. It does not settle requested/effective settings authority, missing-value error presentation, recipe restoration precedence or the shared presentation-snapshot boundary. Broader vocabulary changes remain deferred to architecture design.

## Coverage and existing boundaries

| Path | Current integration points | Boundary to preserve |
| --- | --- | --- |
| Active photo and rapid edits | `App::select`, `submit_with_quality`, worker render closure, coordinator and delivery classifier | Authored controls can lead displayed pixels. Compatible feedback must not replace requested edits. |
| Multi-photo editing | Viewport selection, multi-edit runtime, overview requests, per-photo completion | Browser membership, editing subset, active controls and retained inactive frames are distinct. |
| Smart and framing | Smart controller, worker request resolution and presentation cache | Refinement can require new pixels. Compatible Crop/Rotate changes can reuse existing pixels and their actual grid. |
| Native inspection and export | Request purpose, inspection roots, interactive export and batch export | Inspections have authoritative inputs. Export is frozen work, not replaceable preview demand. |
| Idle and decoded prefetch | Cache neighborhood, immutable recipe snapshot, preparation attempts and cold reduced insertion | Optional preparation cannot become foreground use or displace active work. |
| Auxiliary presentation | Scopes, navigator, browser loaders and persisted preview worker | Results need image/context pairing. Auxiliary work must not delay main-image feedback unnecessarily. |
| Core reuse and resources | Render sessions, reduced working set, graph cache, engine preparation and shared inventory | Numerical validity remains graph-owned. Accounting must deduplicate shared allocations and distinguish retained from peak memory. |

## A. Authored state and metadata

Sources: [main](../crates/foveonic-gui/src/main.rs), [multi-edit runtime](../crates/foveonic-gui/src/multi_edit_runtime.rs), [sidecars](../crates/foveonic-gui/src/sidecars.rs), [Before](../crates/foveonic-gui/src/editor/before.rs), [camera resolution/export](../crates/foveonic-gui/src/export.rs).

| ID | Observed concern | Contract to establish before changing it |
| --- | --- | --- |
| A1 | Requested edits exist in `App::settings`, runtime `Photo::settings`, sidecar `requested` and `unsaved` maps. `sync_multi_edit`, `observe_active`, `observe_requested` and restoration establish precedence through call order. | Identify authoritative values and allowed handoffs for active edits, inactive edits, restoration and persistence. Preserve Import's memory-only edits. |
| A2 | One `Settings` type carries authored controls, transient grid/region, estimates, effective defaults and workspace tuning. Submission, worker resolution, recipe construction and reply construction each normalize portions of it. | Name requested versus effective versus displayed settings and document which boundary transforms each field. Avoid another independently normalized recipe copy. |
| A3 | Camera defaults and optional numeric/reset values are resolved across UI normalization, export resolution and `multi_edit_runtime::numeric_settings`, which the worker calls after rendering. Some failures become `.ok()` and missing values. | Separate camera interpretation, editable numeric values and reset defaults. Define what absent values mean before consolidating the preparation. |
| A4 | Selection, sidecar restoration, source loading and render replies can independently provide metadata/defaults. The UI update loop integrates them in a specific sequence before normalization and submission. | Distinguish metadata availability from recipe readiness and image readiness. Check delayed restoration and switching during edits. |
| A5 | Foreground `restore` and idle `CachedRecipes::resolve` implement related recipe precedence with different inputs. Foreground can load/probe a source while idle preparation consumes a local snapshot. | Write the common precedence rules and preserve deliberate access differences. Do not generalize idle preparation into a second sidecar reader. |

## B. Request meaning and identity

Sources: [worker request/resolution](../crates/foveonic-gui/src/rendering/worker/mod.rs), [presentation compatibility](../crates/foveonic-gui/src/rendering/worker/presentation.rs), [processing eligibility](../crates/foveonic-gui/src/rendering/worker/processing.rs), [Smart comparisons](../crates/foveonic-gui/src/rendering/smart.rs), [target identity](../crates/foveonic-gui/src/rendering/coordinator.rs).

| ID | Observed concern | Contract to establish before changing it |
| --- | --- | --- |
| B1 | Generic edit and geometry comparisons live under Smart, while pixel/context and delivery comparisons live under presentation and worker modules. Before, controls, multi-edit and coordinator depend on those helpers. | Distinguish same authored edits, same pixels, compatible work and identical delivery. Preserve framing, estimate, scale and range-name exclusions intentionally. |
| B2 | Requested quality, workspace policy, actual grid and regional eligibility are resolved across active submission, overview submission, Smart, worker resolution, reduced eligibility and idle preparation. | Identify the decision owned at each boundary. Reuse core camera-capability policy without conflating intent with actual rendered geometry. |
| B3 | Purpose is represented by `Purpose`, rendition, output string, comparison flag, region and export presence. `Target::of`, native-grid checks and reuse guards reconstruct overlapping parts of that meaning. | Establish one vocabulary for request purpose and document which distinctions affect work, resource choice and display acceptance. |
| B4 | Source validity combines path lookup, optional known `SourceOrigin`, session identity and sometimes a file-identity read. Unknown identity is permitted in several compatibility checks, with validation deferred. | State when identity is known, borrowed or newly verified. Preserve replacement-file handling without adding filesystem probes to UI compatibility. |
| B5 | Request compatibility compares calibration allocations with `Arc::ptr_eq`. Idle configuration and delivered views use signatures. | Determine whether these intentionally represent snapshot identity versus numerical equivalence. Similar names do not justify replacing one with the other. |

## C. Work selection, execution and interruption

Sources: [worker loop](../crates/foveonic-gui/src/rendering/worker/mod.rs), [service cursor](../crates/foveonic-gui/src/rendering/coordinator.rs), [priority gate](../crates/foveonic/src/priority.rs), [CPU admission](../crates/foveonic/src/decode_pool.rs), [graph executor](../crates/foveonic/src/render/execute.rs), [GPU completion scope](../crates/foveonic/src/gpu/scheduling.rs).

| ID | Observed concern | Contract to establish before changing it |
| --- | --- | --- |
| C1 | The render closure chooses representation, loads resources, resolves settings, defines cancellation, executes, retains results, prepares display and assembles replies. Parallel `Option` variables and booleans encode mutually exclusive paths. | Make the existing decisions and valid states readable independently of their effects. Moving the entire closure into another file would not resolve its responsibility mixing. |
| C2 | `WorkPlan` holds images, scopes and picks. Frozen export, memory updates, clear/stop and idle configuration live beside it in `Pending`. Worker order and cursor fairness jointly select work. | Preserve replaceable intent versus frozen commands and define who decides service order. This does not require moving everything into a new job graph. |
| C3 | Foreground demand is held by queued-plan publication, active worker scopes and selection opening. The cache facade exposes priority APIs, and thread-local context supplies batch/speculative priority. | Distinguish demand lifetime, actual execution and interruption. Establish how each producer participates without duplicating demand ownership. |
| C4 | Image cancellation re-resolves the pending settings and allows compatible continuation. Picks cancel by task ID. Idle work combines gate epoch, config identity, neighborhood generation and pending control flags. | Describe obsolescence, cooperative yield and terminal cancellation separately. Test their composition before introducing a common control interface. |
| C5 | Providing a graph cancellation predicate also selects per-operation GPU waits. Background submission wait deferral depends on a thread-local stage scope. Allocation-window and display waits add other completion rules. | Make completion ownership explicit independently of whether cancellation is enabled. Preserve bounded backlog and immutable storage. No concurrent GPU scheduler is selected. |
| C6 | There are concrete redundant branches: reduced rendering checks `native` again after an unconditional `native.take()` with no intervening assignment. The image closure also repeats export alternatives after export has already returned. | Confirm reachable states with existing behavior checks, then remove dead control flow in a scoped cleanup. Do not infer a runtime latency bug from dead code alone. |

## D. Completion, delivery and progress

Sources: [reply handling](../crates/foveonic-gui/src/main.rs), [view planning](../crates/foveonic-gui/src/rendering/views.rs), [coordinator settlement/admission](../crates/foveonic-gui/src/rendering/coordinator.rs), [runtime completion](../crates/foveonic-gui/src/multi_edit_runtime.rs), [progress](../crates/foveonic-gui/src/progress.rs).

| ID | Observed concern | Contract to establish before changing it |
| --- | --- | --- |
| D1 | `Result<Option<Frame>>` means rendered, superseded, exported or failed depending on request fields and error state. Reduced execution has richer outcomes that the GUI partly collapses. | Name normal execution outcomes and preserve exact settlement, retry and frozen-export behavior. Keep error presentation separate from scheduling decisions. |
| D2 | UI receipt settles the task, classifies, may retain pixels/update per-photo metadata, then performs image source/order admission and applies a destination. Multiple coordinator methods publish intermediate plans. | Document allowed effects before and after admission. Test replaced-source and out-of-order completions for both image and non-image metadata. The ordering concern is not yet a demonstrated bug. |
| D3 | Validity guards remain in delivery, retained runtime frames, active-selection reuse, Smart detail and Before restoration. Some are distinct display contracts, while some repeat request compatibility. | Separate what a completion may update from whether a held frame can be drawn or reused. Preserve useful older feedback and immutable requested settings. |
| D4 | `busy`, `exporting`, `opening`, coordinator settlement, gate activity and a revision-keyed progress tracker describe different lifetimes. One progress snapshot summarizes a multi-target set through UI settlement. | Give each state one meaning and make derivation visible. Preserve the sibling-completion regression check. An idle worker or completed image alone cannot settle the whole set. |

## E. Cache reuse and preparation

Sources: [decoded cache API](../crates/foveonic/src/decode_cache/mod.rs), [disk retention](../crates/foveonic/src/decode_cache/lru.rs), [preview identity](../crates/foveonic/src/decode_cache/previews.rs), [source identity snapshot](../crates/foveonic/src/decode_cache/original.rs), [processing adapter](../crates/foveonic-gui/src/rendering/worker/processing.rs), [idle preparation](../crates/foveonic-gui/src/rendering/worker/preparation.rs), [graph cache](../crates/foveonic/src/interactive.rs), [shared analysis](../crates/foveonic/src/render/recipes/linear_edits.rs).

| ID | Observed concern | Contract to establish before changing it |
| --- | --- | --- |
| E1 | Disk readiness, a decoded-source hit, a reduced-session hit, retained graph inputs, photographic pixels and a display texture are different forms of reuse. Their status and invalidation are spread across owners. | Use representation-specific readiness and hit terminology. Green disk readiness must not imply a prepared render or retained GPU result. |
| E2 | The decoded cache facade also exposes priority, neighborhood planning, recipe/default updates, calibration policy, metadata and display-preview persistence. Its control object combines these concerns. | Map which capabilities genuinely share ownership before separating interfaces. Preserve bundle identity, retention and foreground priority instead of adding another universal cache manager. |
| E3 | Native load, reduced load/refresh, idle preparation and batch import each acquire sources through different combinations of live native data, verified cache snapshots, local bundles and original access. | Specify acquisition intent, identity verification and fallback rules. Consolidate shared steps without making speculative work read a card or changing import's local-copy behavior. |
| E4 | Persisted photographic previews explicitly reject Adaptive Tone because workspace tuning has no persistent preview identity. Memory display snapshots and numerical checkpoints have different rules. | Make the exception visible in the cache contract and investigate complete preview identity. Do not silently remove the guard or treat it as an engine rendering limitation. |
| E5 | Shared analyses author separate camera branches, append scoped outputs and retain selected intermediates. Graph identities include output roles and ancestry. Aliases can have distinct cache identities despite sharing storage. | Trace which common inputs are actually reused across consumers and option transitions. Keep analysis policy, numerical identity and buffer ownership separate. Dehaze reset is the reported test case, not a proven explanation. |
| E6 | Source-statistic misses calculate while holding the statistics-cache mutex. Metadata repair preparation invokes its build closure under the preparation-cache mutex. | Record lock scope and work performed inside it. Determine actual sharing and contention before changing synchronization. Independent engine executors currently have separate preparation caches. |
| E7 | `Engine::executor` shares GPU infrastructure but creates a new owner and fresh preparation/statistic caches. Idle and foreground share one engine, while batch has another. | Document the reuse domain rather than assuming a shared device means shared preparation. Sharing more caches is a separate decision with ownership and invalidation consequences. |
| E8 | Native `RenderSession::last_settings` changes before an attempt runs. Reduced-session `last_settings` changes only after a successful render. Both feed changed-parameter reporting used by continuation policy. | Decide whether this means last requested, attempted or successful settings. Check supersession/failure sequences before aligning the lifecycle. |
| E9 | Retention combines generic changed-dependency frontiers, recipe-declared sealed inputs, explicit session hints and `linear_edits::prioritize_cache` matching selected operation kinds. These mechanisms serve different purposes but interact under one graph-cache budget. | Identify required restart inputs, optional expensive intermediaries and priority hints independently. Check shared consumers and budget pressure before adding another per-operation retention rule. |

### Shared intermediaries inspected

| Data | Current owner and reuse domain | Review question |
| --- | --- | --- |
| Whole-photo RGB for Dehaze and Adaptive analysis | Shared linear-edit recipe constructs camera branches at consumer-selected grids. The graph cache owns exact ancestry and output identity. | When consumers share compatible inputs, do their branches reach the same retained work, including enable/disable transitions? |
| Brightness guides, classifiers and spatial-edit inputs | Numerical nodes describe the data. Recipes declare dependencies and selected retention priorities. | Are shared inputs retained once with sufficient validity, and are optional hints being mistaken for required inputs? |
| Neutral estimates and channel means | Engine statistic lookup plus session-pinned `SourcePreparation` records, keyed by source origin, statistic kind and descriptor. | Are lookup lifetime, pinned lifetime, calculation lock scope and source-free refresh consistent? |
| Tone curves and metadata repair tables | Engine preparation cache uses metadata identity and stage/parameter keys with weak source ownership. | Are identical metadata snapshots reused within the intended executor domain without retaining decoded samples? |
| Native clipping and working recovery evidence | Recipe/node-owned preparation and graph buffers, with explicit native-preparation release during source transitions. | Can consumers reuse compatible evidence, and do representation changes release only the native data that is no longer needed? |
| Photographic pixels and display textures | Presentation snapshot, delivered-frame and auxiliary-preview owners. | Does each consumer require new numerical work, new presentation work, or only another immutable reference? |

## F. Memory, retention and invalidation

Sources: [frontend inventory](../crates/foveonic-gui/src/memory.rs), [shared trimming](../crates/foveonic/src/render/memory.rs), [working-set ownership](../crates/foveonic/src/render/working_set.rs), [preparation cache](../crates/foveonic/src/render/preparation.rs), [source-statistic ownership](../crates/foveonic/src/render/source_preparation.rs), [idle admission](../crates/foveonic-gui/src/rendering/worker/processing.rs).

| ID | Observed concern | Contract to establish before changing it |
| --- | --- | --- |
| F1 | Shared user budget, graph-cache cap, historical working-set limit, per-cache preparation bounds and idle pre/post admission are different policies. Some are hard retention bounds and some protect an oversized active owner. | Name target versus cap, active protection and optional admission. Preserve peak-headroom qualifications instead of presenting one number as a hard process limit. |
| F2 | Shared-buffer and metadata deduplication appears in graph-cache byte counts, reduced-session reporting, working-set totals and global inventory. Some count local owners and some deduplicate across them. | Identify the common accounting primitive and distinct reporting scopes. The linked overhead investigation measures about 10 ms across repeated trim/accounting intervals per eight-photo warm edit. Individual scan costs remain unattributed. |
| F3 | Inventory refresh depends on a manually assembled key of frame pointers, reply epochs and resource revisions from several producers. Worker trimming consumes snapshots from UI updates. | Every owner change must invalidate the right snapshot. Test resource removal and dropped replies as well as insertion. Keep accounting updates off the paint path in any later redesign. |
| F4 | Frontend inventory holds strong shared references. Coordinator delivery seeds, task seeds, worker presentation, runtime frames, Before and queued replies can also retain the same pixels. | Distinguish deliberate reuse lifetime from accounting-snapshot lifetime. Preserve deduplication and queued-image coverage while checking that old snapshots do not prolong retention unnecessarily. |

## G. Background and auxiliary worker consistency

Sources: [idle preparation](../crates/foveonic-gui/src/rendering/worker/preparation.rs), [cache prefetch](../crates/foveonic/src/decode_cache/worker.rs), [browser loader](../crates/foveonic-gui/src/browser/loader.rs), [navigator](../crates/foveonic-gui/src/navigator.rs), [rendered previews](../crates/foveonic-gui/src/previews.rs), [capture metadata](../crates/foveonic-gui/src/capture.rs), [folder discovery](../crates/foveonic-gui/src/folder_loader.rs), [batch](../crates/foveonic-gui/src/batch.rs).

| ID | Observed concern | Contract to establish before changing it |
| --- | --- | --- |
| G1 | Several workers repeat latest-pending, generation/epoch rejection, active-job bookkeeping, wakeup and reply patterns. Payload and persistence lifetimes differ materially. | Inventory shared mechanisms and differing replacement rules before generalizing. Frozen writes/batches must not acquire latest-preview semantics. |
| G2 | Navigator and persisted previews each defer enqueueing with a 16 ms presentation delay and `busy` checks. Preview persistence also uses the foreground gate, while navigator directly submits to the ordered queue. Immediate browser thumbnails are prepared from UI receipt. | Establish priority and completion policy for every GPU producer. Similar delays are not evidence that their workloads should share one queue or wait policy. |
| G3 | Idle attempts are remembered by source/window/configuration and budget. Foreground uses promote history, whereas speculative reduced entries are cold. A speculative decoded payload read still updates disk recency. | Make access and retention intent explicit per cache. A source-presence check does not prove the requested recipe is fully prepared. Preserve the no-prepare/evict-loop rule. |
| G4 | Shutdown and backpressure differ: image/auxiliary replies use bounded blocking sends, most workers signal stop without joining, while sidecar shutdown drains writes and joins. | Document drop, stop, drain and queued-resource lifetimes. Test the existing needs on close/clear without imposing a universal cancellation or shutdown framework. |
| G5 | Calibration, workspace Adaptive tuning, folder changes and explicit clear each invalidate different combinations of runtime photos, Smart/detail state, browser previews, persisted previews, cache policy and worker owners. The fanout lives in several UI paths. | Record which input invalidates which representation. Check that a new workspace setting or retained owner cannot silently miss an invalidation, while preserving unaffected work. |

## H. Evidence, naming and source organization

Sources: [GUI timing adapter](../crates/foveonic-gui/src/timing.rs), [aggregate profiles](../crates/foveonic/src/profile.rs), [native scenarios](../crates/foveonic-gui/src/smoke/scenarios.rs), [trace analysis](../scripts/profiling/trace_analysis.py), [coordinator tests](../crates/foveonic-gui/src/rendering/coordinator_test.rs), [architecture map](../docs/interactive-rendering-architecture.md).

| ID | Observed concern | Contract to establish before changing it |
| --- | --- | --- |
| H1 | GUI `timing::mode()` deliberately returns aggregate profiling Off while SDK capture can be active. Some preparation timers start only when aggregate profiling is enabled, leaving native CPU recipe authoring unscoped. Typed events coexist with JSON adapters and event-name-derived categories. | Clarify aggregate measurements versus shared causal tracing. Make shared preparation coverage independent of aggregate mode. Preserve disabled-recording cost and Wall behavior. Avoid a second instrumentation system during cleanup. |
| H2 | Deterministic coordinator/delivery checks, GPU session checks and real-app scenarios cover different boundaries. No single coverage index joins selection, reuse, supersession, auxiliary work and final settlement. | Build a behavior-to-evidence matrix before using passing helper tests as proof of the full application flow. Distinguish work avoidance, pixels, latency and physical presentation. |
| H3 | `worker.rs` contains request policy, execution, resource integration and large adjacent test groups. More important than its size, worker execution calls multi-edit UI preparation and coordinator retention calls worker-owned presentation types. | Record dependency direction and make responsibility visible before moving files. Preserve the distinction between pure decisions, numerical execution and UI effects. |
| H4 | Names such as active, current, ready, revision, source and cached refer to different domains. Plans also combine historical experiments with active acceptance work. | Use the vocabulary below and maintain one current home for each unresolved requirement. Keep the implemented map distinct from the proposed architecture. |

## Vocabulary to preserve

| Concept | Current meaning |
| --- | --- |
| Active photo | The UI controls/display context. It is not the entire viewport editing subset. |
| Active reduced entry | The working set's one currently protected execution entry. It is not every selected photo. |
| Browser selection / viewport selection | Processing membership versus bounded editing targets. Neither alone establishes cache residency. |
| Target and purpose | The consumer/result being coordinated, including Before, detail and inspection distinctions. |
| Task ID / stamp | Compatible work lineage versus the exact target/revision being settled. Neither is a graph version. |
| Requested revision / presentation revision | Authored request ordering versus a presentation of retained pixels. Reusing pixels can advance presentation without executing the recipe. |
| Source origin / session handle | Immutable source identity versus a worker-local lookup handle. |
| Numerical checkpoint | An immutable result with effective operation, input ancestry, geometry and source validity. |
| Resource revision / prefetch epoch | Inventory invalidation versus optional-attempt invalidation. Neither means settings changed. |
| Render completion / admission / draw | Finished execution, permission to update a view, and actual UI use. These are separate milestones. |

## Evidence required to choose slices

For each candidate, identify the current invariant, source owner, duplicated or misplaced rule, affected interaction paths and smallest observable check. Decide whether it is mechanical cleanup, contract clarification or a behavior fix. Resolve contract questions before changing semantics. Similar-looking code with intentionally different meanings should remain distinct.

The review should include these combinations when relevant: one photo and an eight-photo set, unchanged selection/re-add, rapid same-slider and different-slider edits, enabled/disabled transitions, Crop/Rotate, Smart refinement, inspection/comparison, calibration replacement, upstream native refresh, foreground arrival during idle work, memory trimming, export and shutdown. Select checks for each slice rather than running every combination after a comment or file move.

The reported Dehaze nonzero-to-zero jump needs a controlled trace of requested purpose/grid, checkpoint hits, executed work and waits. Test shared Adaptive/Dehaze analysis with enabled and disabled consumers before attributing the result to a specific alias policy. Keep the engine black box intact except where graph reuse or preparation ownership is the subject of the selected slice.

## Deliberate boundaries and limits

Camera recipes own interpretation, typed graphs own dependency validity, nodes own arithmetic and GPU guards own immutable storage. Those are useful existing boundaries. Separate disk and memory caches, frozen versus replaceable work, accounting versus numerical identity and compatible feedback versus current delivery are necessary distinctions.

This is an exhaustive source-review inventory of the current coordination paths listed above, not a proof that every possible runtime interleaving is correct or a list of forty-two bugs. No new scheduler, job table, cache manager, concurrency framework, node algorithm or production behavior has been selected. Slice selection follows owner review of the inventory.
