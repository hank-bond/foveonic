---
summary: "Compare serial, threaded and stage-interleaved rendering in an isolated probe with exact outputs and bounded resource ownership."
---

# Wide-rendering investigation

The owner authorized this isolated investigation after the viewport functional slices passed native acceptance. The selected probe lives on `experiment/wide-render-probe`, starting from `65ae1cf`, whose core execution matches the functional application baseline. Application execution and defaults remain unchanged. The probe tests the hypothesis before any proposal to integrate wider execution. Follow the [performance and input contract](../docs/specs/performance-diagnostics.md) and retain the frozen serial baseline.

## Hypothesis

Advancing ready operations across several photographs may reduce multi-edit latency compared with completing each whole graph serially. Separate traversal order, CPU concurrency and completion-wait grouping. Submission grouping and packing photographs into shader dispatches remain later mechanisms. Single-photo execution uses the same operation runtime as a one-photo group.

## Findings to retain

- The executor keeps per-photo graph/cache, live-value, scratch and progress state. Every executed operation waits when a cancellation callback exists, including one that always returns false. Changing the outer loop alone does not remove those waits.
- GPU helpers submit internally. Stage interleaving does not automatically combine submissions.
- The current work plan has one consuming cursor, which owns its served-job set. A worker pool would need one authority for claiming jobs. Independent copies of that cursor could claim the same job. Per-photo result identity and settlement already have their own coordinator owner.
- Reduced working sets currently protect one active entry. Several in-flight sessions need explicit protection until submitted operations retire, within their existing resource ownership.
- Recipes and cache hits differ. Advance independent ready work without aligning graph ordinals or imposing an all-photo stage barrier. Preserve per-photo source identity, cache outcomes, errors and settlement.

Sources: [executor](../crates/foveonic/src/render/execute.rs), [GPU facade](../crates/foveonic/src/gpu.rs), [interactive sessions](../crates/foveonic/src/interactive.rs), [reduced working sets](../crates/foveonic/src/render/reduced.rs) and the [overhead investigation](./coordination-overhead-profile.md).

## Selected implementation slice

Extract a private `GraphExecution` from `execute_controlled`. It owns the next-node position, live values, cache/required masks, dependency reads, consumer counts, scratch, trace, progress and diagnostic state. Its step borrows the graph, input context, `Execution` and profile, then advances one required operation. Keep the operation dispatcher in one place. Ordinary `execute_controlled` drives the same state to completion.

Separate existing [recipe preparation](../crates/foveonic/src/render/recipes/mod.rs) and [artifact planning/packaging](../crates/foveonic/src/render/artifacts.rs) around that cursor. Each probe photo owns its prepared recipe, session, cursor, profile and result identity. Plan checkpoints once, retain each output through existing cache rules, and commit/finish each photo independently on success, cancellation or failure. Avoid self-referential job types and duplicate recipe builders.

Use a single-thread round-robin loop over admitted photos. Advance each photo's next executable operation, regardless of its ordinal or operation name. Settle a completed photo and admit another without waiting for the entire group. Start with individually owned `ReducedSession`s prepared through existing APIs. Preserve each session's engine/source identity and hold admitted owners through submitted-work completion. Do not widen `ReducedWorkingSet` for this probe.

| Control | Traversal | Completion policy |
| --- | --- | --- |
| Frozen serial | One complete photo at a time | Existing operation and display waits |
| Threaded whole graphs | Existing `profile_editing --workers N` | Existing waits |
| Interleaved | One required operation per admitted photo | Existing waits |
| Interleaved with grouped waits | Same cursor and traversal | Fence a bounded submitted-work window |

Widths are 1, 2, 4 and 8. Width-one cursor results must match the frozen serial build exactly before wider measurements. Grouped waits are a separate follow-up mode. Preserve internal CPU readbacks, helper submissions and guarded scratch reuse. A submitted-work window need not contain the same stage, or one stage from every photo.

The initial harness has no background producer. Real background interruption additionally requires the managed submission scope to surround the bounded window. The existing `BackgroundStage` guard assumes completion before `complete()` and waits on unwind. Do not retain nested per-photo guards across yields or claim completion after merely encoding work.

## Measurements and gates

- [x] Inspect execution ownership and primary API contracts, then select the isolated cursor proposal.
- [ ] Freeze build/input hashes, recipes, environment and serial results. Run threaded whole-graph controls.
- [ ] Review the cursor extraction and prove width-one complete float samples and storage/view geometry against the frozen serial result.
- [ ] Compare warm Merrill Thumb exposure edits at all widths. Then run the maintained mixed-camera set and an upstream edit requiring more operations.
- [ ] Compare existing waits with bounded grouped waits, keeping submission and numerical work unchanged.
- [ ] Verify independent settlement and supersession after an observed submitted operation. Cancelled work must not settle, unchanged peers must settle, and the replacement must be exact.
- [ ] Record benefits, first-result costs, peak memory and limitations before deciding whether any integration is justified.

Use matched effective recipes, cache states, output retention and display endpoints. Report first completed useful texture, complete-group latency, per-photo completion order, graph preparation, executed nodes/cache hits, submissions, waits, production readbacks, obsolete work and interruption-to-replacement latency. Retain images and textures through an actual GPU completion boundary. Enqueue duration is not completion latency. Exact image readback and fresh verification happen outside measurement.

Keep Off latency runs separate from Wall attribution and selected GPU captures. Exclude first-use setup or report it separately. Include recipe preparation in end-to-end measurements so preparing all admitted graphs cannot hide a first-result delay. Record repetitions and variability, and serialize compiler/GPU workloads as required by the performance contract.

Inventory decoded sources, metadata, preparation, live cursor values, scratch, retained checkpoints and delivered images/textures. Deduplicate shared buffers. Existing GPU allocation counters measure traffic, not peak residency. Report logical live-owner peaks and process peaks separately, with driver/allocator overhead unaccounted by logical totals. Admit only a bounded active set and record actual width when memory prevents the requested width. The existing 2 GiB submission allocation window is not a total-memory limit.

This probe can attribute gains to traversal, CPU overlap or fewer completion boundaries. It cannot establish shader-level parallel execution, production working-set behavior, application latency or physical screen presentation. Preserve synchronous readbacks and current numerical algorithms. Stop or narrow the experiment if the preparation/cursor changes require a new general scheduling framework.

## Primary API evidence

- Wgpu 24.0.5 is the locked GPU dependency. [`Maintain`](https://docs.rs/wgpu/24.0.5/wgpu/type.Maintain.html) distinguishes polling, waiting for a particular submission and waiting for the most recent submission. [`Device::poll`](https://docs.rs/wgpu/24.0.5/wgpu/struct.Device.html#method.poll) documents concurrent queue submissions, so a device-wide wait cannot be treated as photo-local attribution.
- The [wgpu 24 queue source](https://raw.githubusercontent.com/gfx-rs/wgpu/v24.0.5/wgpu/src/api/queue.rs) exposes submission indices and completion callbacks. Callbacks require subsequent submission or polling and should only record a short observation. Buffer writes are staged until submission. These contracts support explicit completion fences without changing shader work.
- [Buffer mapping](https://docs.rs/wgpu/24.0.5/wgpu/struct.Buffer.html#mapping-buffers) waits for safe CPU access. Preserve algorithmic readbacks and keep verification readbacks outside timed runs.
- The locked Rayon version is 1.12.0. [`num_threads`](https://docs.rs/rayon/1.12.0/rayon/struct.ThreadPoolBuilder.html#method.num_threads) bounds pool workers. [`scope`](https://docs.rs/rayon/1.12.0/rayon/fn.scope.html#task-execution) does not guarantee task order. The existing threaded control measures whole-graph CPU concurrency, not a controlled graph-stage traversal.
