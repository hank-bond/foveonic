---
summary: "Measure graph stage costs and retained checkpoint ownership before changing numerical cache policy."
status: archived
---

# Checkpoint profiling

This slice continued the [cache workflow investigation](../cache-workflow-measurements.md). The owner requested per-stage latency and memory evidence to improve checkpoint selection within the existing 1 GiB per-session bound. This slice changes diagnostics and maintained measurement tools. Cache policy and application defaults remain unchanged.

## Measurement contract

Extend the existing [processing profile](../../docs/specs/profiling.md). Record graph-local output dependencies, node timing ranges, buffer allocation identity and generation, geometry where supplied by the value, and unique logical buffer bytes at graph/scratch/cache boundaries. Keep exact retained version ancestry and cache roles available as an explicit snapshot outside timed rendering.

Diagnostics own scalar metadata only. Retaining a buffer guard would change exclusivity and scratch reuse, invalidating the measurement. Allocation identity determines physical bytes. Generation distinguishes rewritten contents. Boundary counts exclude node-internal peaks, frontend owners, queued driver lifetimes and decoded CPU memory. CPU preparation and parameter payloads remain separate categories.

Use declared example recipes and reproducible inputs. Open each decoded source once outside timing. Warm compiled shaders, then create a fresh executor and session for each independent control/trial. Record base, unchanged, edit A, distinct edit B and return A. These phases distinguish initialization, established restart frontiers and historical reuse. Verification runs compare each phase with fresh processing outside its timed interval and are separate from clean latency runs.

## Steps

- [x] Consult a largest-model design review about minimal shared-boundary instrumentation.
- [x] Add scalar graph/output, boundary and retained-version inventories.
- [x] Complete a small headless driver and portable report analyzer.
- [x] Check diagnostics preserve reader counts, scratch behavior and exact pixels.
- [x] Measure ordinary Off latency separately from Wall/GPU attribution and observation overhead.
- [x] Profile representative Merrill color, monochrome and alternate noise recipes at native and reduced scales.
- [x] Summarize costly retained allocations and propose checkpoint experiments grounded in measured edit reuse.
- [x] Obtain integrated review, address findings and commit the completed slice.

An ancestry sum describes measured contributing GPU work, not marginal latency saved by retaining that output. Shared producers, multi-output operations and CPU recipe preparation prevent that inference. Policy changes need a subsequent agreed experiment.

## Initial measured findings

Measurements used an Apple M1 Pro and the [declared checkpoint recipes](../../scripts/profiling/test_inputs.json), with three independent Off trials per active control and separate Wall/GPU attribution. Early inactive-control probes are excluded. The portable driver and input preparation procedure reproduce the maintained workloads.

The maintained fixture set separates active color and monochrome controls. Color cases use reconstruction controls. The monochrome case uses its mix, noise, banding and texture controls. Natural cases cover HDR and local contrast. The Reference-tone speaker cases omit those inactive controls. A nonzero dispatch count alone does not prove an edit was active, because a changed request can still rerun only final tone/output work. Early probes with inactive controls are excluded from edit-performance conclusions. A Quattro-only highlight amount also passed session execution but failed direct-render validation. Maintained Merrill probes now use highlight threshold and validate authored controls before attaching request geometry. The [runtime docket](../render-runtime.md#architecture-specific-options-follow-up) records that production validation inconsistency separately.

### Native memory decomposition

The blue parking sign's color/Natural exposure sequence retained 993.53 MiB of unique GPU cache storage after the second distinct edit. Group allocations first, then classify each by required cut, restart input, other selected output, or history:

| Disjoint retained role | MiB |
| --- | ---: |
| Required cuts | 187.34 |
| Current restart inputs | 202.95 |
| Other current selected output | 184.08 |
| Historical allocations | 419.16 |

The history consists of 235.08 MiB of noise tiles and a 184.08 MiB prior response. `base-response.rgb` and `linear.edits` share each response allocation. Counting names instead of allocations would overstate memory.

Separate CPU owners report 185.64 MiB of decoded source, 7.24 MiB of metadata, 1.94 MiB of parameter payloads and 2.60 MiB of prepared repair. Engine preparation is about 1.34 MiB in this phase. The delivered final image references another 184.08 MiB. These categories describe this direct session. They do not include GUI textures or establish an intrinsic 2 GB cost per photo.

The largest observed graph/scratch/cache boundary in the initial sign render is 2123 MiB, versus 939 MiB retained afterward. The same recipe's post-edit cache is about 495 MiB at divisor two, 132 MiB at divisor four and 33 MiB at divisor eight. Those reduced-grid runs still retain a CPU source through `RenderSession` and are not measurements of the app's source-free reduced owner. Their cold boundary also includes native sensor processing before reduction.

### Stage costs and reusable sets

One separate synchronized GPU diagnostic for the native sign gives the following producer costs. These are operation attribution, not ordinary edit latency or marginal cache savings. Outputs from one producer can be requested separately.

| Stage | GPU ms | Observed output storage |
| --- | ---: | --- |
| Noise base | 119 | 235 MiB tiles and 78 MiB pattern seed |
| Pattern | 87 | 78 MiB ratio tiles |
| Adaptive brightness guide | 78 | 15 MiB |
| Sensor banding | 68 | 184 MiB |
| Column correction | 50 | 235 MiB |
| Highlight recovery | 46 | 184 MiB |
| Despeckle | 16 | 184 MiB |
| Camera repair | 14 | 184 MiB |
| Adaptive response | 18 | 184 MiB |

The full machine-readable reports preserve every observed node, output dependency, allocation identity, CPU phase and retained version. Small analysis guides are promising checkpoints relative to their size. Large producers must be evaluated as reusable sets.

The sign's exposure-to-detail sequence demonstrates why: its 235 MiB `noise.tiles` remains available, but NoiseBase reruns to produce the missing 78 MiB `noise.pattern-seed-tiles`. Column and ratio tiles also rebuild. The speakers retain noise, column and ratio and resume at Texture for the comparable control change. Retaining one large producer output does not guarantee avoiding that producer's work.

Three ordinary Off trials put sign exposure edits at about 110 ms for the first change, 37 ms for the next distinct value and 16 ms when returning to the retained value. Switching from exposure to detail instead takes about 639 ms, versus 109–111 ms for the speaker recipes. These recipes differ, so the contrast identifies an investigation target, not a causal speedup from a different policy.

### Observation cost and next experiment

Wall metadata scans cost a median 0.07 ms per render and a maximum 0.44 ms across the first 200 measured phases. All 200 matched Off/Wall activity records are identical, including normal allocations, dispatches, transfers and timestamp resolutions. Off versus Wall elapsed differences remain subject to run variation. The scan counter excludes graph metadata collection and trace emission, and cannot be subtracted wholesale from node totals.

The next agreed discussion should compare complete upstream checkpoint sets with late RGB/restart sets and historical-response retention. Measure repeated same-control edits, cross-control re-entry and continuing distinct-value sweeps. Returning to a prior response benefits from history, but a forward sweep may not. Inspect the large required highlight cut separately from optional history. Keep small shared analyses and required reduced cuts explicit. Do not implement a size-only eviction change or derive savings by adding ancestor timings. No cache policy or application default changes in this slice.

## Completed checks

Independent design and final implementation review approved the metadata-only ownership, deduplication, timing boundaries and report limitations. Review also corrected inactive fixture choices and the warmup/executor lifecycle. All 340 fresh-session comparisons passed: 240 native phases, 60 corrected monochrome phases, 20 regional phases and 20 Thumb phases. Some native checks deliberately overlap later corrected fixtures. The meaningful maintained matrix contains 44 active edit paths across four recipes, not 340 distinct behaviors. All forty frozen render comparisons stayed bit-exact across three Classic, three Merrill, three ordinary Quattro and one Tiger input at Full/Fast/Draft/Thumb.

Four focused GPU diagnostic tests passed, including unchanged buffer-reader counts after snapshots and exact output/activity behavior. Ordinary release workspace tests passed, including 538 processor and 519 GUI tests. Strict workspace Clippy, 146 portable profiling tests, five artwork tests, formatting and documentation/index checks passed. New measurement processes participate in the existing workload monitor. Controlled diagnostic and supplemental latency captures recorded no competing known workload. Cache retention policy and defaults remain unchanged.
