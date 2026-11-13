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
- [x] Freeze build/input hashes, recipes, environment and serial results. Run contextual threaded whole-graph controls.
- [x] Review the cursor extraction and prove width-one complete float samples and storage/view geometry against the frozen serial result.
- [x] Run the extracted serial dispatcher against all 40 frozen camera/quality cases with an independently built probe binary.
- [x] Compare warm Merrill Thumb exposure edits at all widths. Then run the maintained mixed-camera set and an upstream edit requiring more operations.
- [x] Compare existing waits with bounded grouped waits, keeping submission and numerical work unchanged.
- [x] Verify independent settlement and supersession after an observed submitted operation. Cancelled work must not settle, unchanged peers must settle, and the replacement must be exact.
- [x] Record benefits, first-result costs, sampled memory and limitations before deciding whether any integration is justified.

Use matched effective recipes, cache states, output retention and display endpoints. Report first completed useful texture, complete-group latency, per-photo completion order, graph preparation, executed nodes/cache hits, submissions, waits, production readbacks, obsolete work and interruption-to-replacement latency. Retain images and textures through an actual GPU completion boundary. Enqueue duration is not completion latency. Exact image readback and fresh verification happen outside measurement.

Keep Off latency runs separate from Wall attribution and selected GPU captures. Exclude first-use setup or report it separately. Include recipe preparation in end-to-end measurements so preparing all admitted graphs cannot hide a first-result delay. Record repetitions and variability, and serialize compiler/GPU workloads as required by the performance contract.

Inventory decoded sources, metadata, preparation, live cursor values, scratch, retained checkpoints and delivered images/textures. Deduplicate shared buffers. Existing GPU allocation counters measure traffic, not peak residency. Report logical live-owner peaks and process peaks separately, with driver/allocator overhead unaccounted by logical totals. Admit only a bounded active set and record actual width when memory prevents the requested width. The existing 2 GiB submission allocation window is not a total-memory limit.

This probe can attribute gains to traversal, CPU overlap or fewer completion boundaries. It cannot establish shader-level parallel execution, production working-set behavior, application latency or physical screen presentation. Preserve synchronous readbacks and current numerical algorithms. Stop or narrow the experiment if the preparation/cursor changes require a new general scheduling framework.

## Measured findings

Measurements ran on an Apple M1 Pro with 16 GiB RAM and macOS 26.6.2. The isolated `profile_wide` harness prepares individually owned reduced sessions, renders a base photograph, then measures one changed edit through completed display textures. Source opening and base preparation are outside the interval. Recipe preparation, cache settlement and display completion are inside it. Images and textures remain retained through measurement. Serial and interleaved modes use the same owners and effective settings. The frozen control uses core revision `65ae1cf` with the API-only `profile_wide_stock` harness. Both harnesses are retained on the experiment branch.

The eight Merrill inputs, in order, are blue-parking-sign, glossy-leaves-and-buds, low-light-speaker-and-desk, low-light-spider-and-web, merrill-hazy-coast, paper-and-recycling-can, rocks-and-tree-shadows and sunlit-metal-latch. The mixed set is blue-parking-sign, merrill-hazy-coast, dp0-quattro, sd-quattro, sd-quattro-h, sd9, sd10 and sd14. Resolve owner photographs through the existing example sync and other originals through the [external sample manifest](../scripts/profiling/external_raw_samples.json). Exposure changes by +0.25 EV with automatic exposure disabled. The upstream Merrill edit changes reconstruction chroma from zero to one. These use camera defaults, not owner sidecars.

Each latency case has five Off trials, with mode order rotated across trials. The first mixed trial overlapped CPU work and was replaced with a clean repeat. It remains excluded from latency statistics. Merrill exposure has 50 Off groups and four separate Wall captures. All eight outputs in every group matched the frozen control's float-file SHA-256, storage/view geometry and effective settings. Mixed and noise width-one gates passed before their 30 measured groups each. Eight cancellation/failure cases cover ordinary and grouped waits at widths one and eight. Each suppresses the affected photo's completion, settles all seven peers exactly, and verifies an exact replacement after peers finish. These injections establish recovery, not priority replacement latency.

The final serial-dispatch guard also passed all 40 frozen Full, Fast, Draft and Thumb cases using `profile_pipeline` built from the isolated cursor branch. The ten source cases cover three Classic, three Merrill, three Quattro and one Quattro H. Every result was bit-exact against the untouched serial baseline. The functional application independently passed the same matrix. This broadens the mechanical extraction guard beyond warm reduced-session edits without claiming wider scheduling coverage for Full processing.

Times below are median group completion / first completed texture, in milliseconds. They describe this fixed input order and prepared state.

| Workload | Frozen serial | Interleaved, existing waits, width 8 | Grouped waits, width 2 | Grouped waits, width 4 | Grouped waits, width 8 |
| --- | --- | --- | --- | --- | --- |
| Merrill exposure | 31.190 / 3.935 | 31.190 / 8.465 | 25.481 / 4.788 | 22.331 / 6.360 | 20.576 / 9.283 |
| Mixed exposure | 56.126 / 3.971 | 55.344 / 31.835 | 49.322 / 4.768 | 46.702 / 23.176 | 45.353 / 34.056 |
| Merrill reconstruction chroma | 98.746 / 12.026 | 98.074 / 74.927 | 63.715 / 13.499 | 44.087 / 17.191 | 37.473 / 26.108 |

Observed group ranges were 30.643–31.482, 25.370–25.713 and 20.522–21.079 ms for Merrill exposure serial/width-two/width-eight grouped runs. Corresponding mixed ranges were 55.852–56.638, 49.057–50.096 and 44.825–45.666 ms. Reconstruction ranges were 96.931–101.009, 61.515–65.558 and 36.039–39.503 ms. The extraction's serial Merrill control was 31.393 ms and interleaved width one was 31.253 ms, consistent with the frozen baseline within these small samples.

Whole-process maximum RSS ranged from 844.7–847.1 MiB for Merrill exposure, 1091.2–1097.0 MiB for mixed exposure and 845.2–849.9 MiB for reconstruction. macOS peak footprint ranged from 1322.4–1324.8, 1679.2–1685.0 and 1322.3–1327.3 MiB respectively. These include source preparation and verification. They do not establish that warm-edit overlap costs no additional memory. The optional boundary observer is measured separately from latency and counts deduplicated session buffers, live cursor values, scratch, completed images and unique display textures. It excludes within-node temporary peaks, CPU allocations, uniform/upload staging and driver-retained resources. No byte-budget admission policy is implemented.

The three separate memory runs preserved exact outputs. Merrill reconstruction had the same sampled logical GPU maximum of 195,152,896 bytes (186.11 MiB) at cursor width one and grouped width eight. Mixed exposure at grouped width eight reached 300,464,416 bytes (286.55 MiB). These boundary samples bound the observed owners at those sample points, not total GPU residency or within-operation peaks. The equal Merrill maxima do not prove unchanged temporary overlap, particularly when retained state can dominate the maximum. The observer holds only borrowed buffer references and is disabled for latency trials.

The existing `profile_editing` threaded control used native sessions at worker counts one, two, four and eight. Across five warm rounds, median group completion was 33.117, 26.459, 26.194 and 26.461 ms, with first results at 4.057, 5.766, 9.053 and 15.459 ms. These modes are matched within that control, but source-retention ownership, defaults and alternating edit patterns differ from the reduced-session cursor probe. Separate `--verify --repeats 1` runs passed all 32 fresh-render comparisons across the four widths. Verification remains outside the Off timing samples. The threaded control supplies contextual concurrency evidence, not a causal comparison with the grouped-wait results.

The measured binaries are identified by SHA-256. The follow-up pair adds the mixed/upstream and injection workloads without changing the frozen control's core.

| Binary | SHA-256 |
| --- | --- |
| Frozen exposure control | `8a6ccface387c866f8fcca48126cb3d6a5c64a5ceed2a00c149202dfd0a11421` |
| Exposure cursor probe | `2bc505f7690d2207864f28f19ccfa6537ca724fea0beef960b584a74d35fd9c9` |
| Frozen follow-up control | `59b29b4b80dddcd3a66a350f015cecc59e6dbb03fd6f7d963947d0036361dce3` |
| Follow-up cursor probe | `c7b5bbc8cb3fc56f89a634ae85f86b748bcedfd075d4996f34111e15af2cd632` |

## Recommendation

Retain the reviewed experiment on its isolated branch. Pure interleaving provides no useful group-latency improvement in these measurements and substantially delays the first result. The useful mechanism is grouping completion waits. Width two is the promising next evaluation point for this workload order, with smaller first-result costs than widths four or eight. It is not an application default recommendation.

Any next slice should isolate bounded completion grouping and protect active-photo latency, then validate live resource limits and real foreground interruption before application integration. The present grouped mode still fences each display texture separately, uses device-wide waits with no competing producer, and replaces cancelled work only after peers settle. Mixed-camera recipe preparation also contributes to the first-result delay. These limitations prevent treating the throughput gains as predicted GUI responsiveness or justification to merge the cursor extraction now.

## Primary API evidence

- Wgpu 24.0.5 is the locked GPU dependency. [`Maintain`](https://docs.rs/wgpu/24.0.5/wgpu/type.Maintain.html) distinguishes polling, waiting for a particular submission and waiting for the most recent submission. [`Device::poll`](https://docs.rs/wgpu/24.0.5/wgpu/struct.Device.html#method.poll) documents concurrent queue submissions, so a device-wide wait cannot be treated as photo-local attribution.
- The [wgpu 24 queue source](https://raw.githubusercontent.com/gfx-rs/wgpu/v24.0.5/wgpu/src/api/queue.rs) exposes submission indices and completion callbacks. Callbacks require subsequent submission or polling and should only record a short observation. Buffer writes are staged until submission. These contracts support explicit completion fences without changing shader work.
- [Buffer mapping](https://docs.rs/wgpu/24.0.5/wgpu/struct.Buffer.html#mapping-buffers) waits for safe CPU access. Preserve algorithmic readbacks and keep verification readbacks outside timed runs.
- The locked Rayon version is 1.12.0. [`num_threads`](https://docs.rs/rayon/1.12.0/rayon/struct.ThreadPoolBuilder.html#method.num_threads) bounds pool workers. [`scope`](https://docs.rs/rayon/1.12.0/rayon/fn.scope.html#task-execution) does not guarantee task order. The existing threaded control measures whole-graph CPU concurrency, not a controlled graph-stage traversal.
