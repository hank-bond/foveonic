---
summary: "Measure memory ownership and latency across editing workflows before recommending cache budgets or changing retention policy."
---

# Cache workflow measurements

The owner requested this investigation after [regional preview completion](./regional-preview-completion.md). Establish practical memory guidance for low-latency workflows, then reassess retention strategy. Every workflow remains supported at lower budgets. A recommendation describes avoiding repeated work, not a minimum feature requirement. Keep disk cache and memory cache separate. Follow the [measurement contract](../docs/specs/performance-diagnostics.md) and retain the 16 GB target-machine assumption, roughly 2 GB steady-state and 4 GB peak aspiration from the [multi-edit plan](./multi-edit.md).

## Questions

- Which owners retain decoded sources, prepared metadata, native/reduced inputs, numerical checkpoints, output images and display textures?
- Which references alias the same allocation, and which create real old/new or Before/Current overlap?
- What changes on use, speculative preparation, replacement, eviction and clearing?
- At what budget does each workflow stop rebuilding useful work, and what latency remains after that point?
- Which limits are slot/count or coordination limits that extra bytes cannot solve?
- Does background Whole preparation evict the regional working set needed by the next edit?

The decoded disk format and resident source format differ: the disk cache stores compact sensor codes, while `Source.pixels` remains a `Vec<f32>`. Account the resident decoded samples independently of the disk payload and of GPU uploads.

## Workflow matrix

| Workflow | Actions and low-latency evidence |
| --- | --- |
| Two-photo multi-edit | Fit Smart, repeated exposure edits on both, one and then both zoomed; compare cheap finishing and a sensor-level edit. Separate foreground completion from background preparation. |
| Full Before/After | Warm Current and Camera/Defaults, toggle repeatedly at Full and while zoomed. Record image/texture reuse, checkpoint reuse, source reads and extra dispatches independently. |
| Eight-photo multi-edit and loupe | Fit Thumb/Smart, edit all, inspect several points on one photo, alternate photos, revisit the same points and request new points. Distinguish retained detail images from retained native processing. |
| Cull and switch | Switch among warmed groups and return to a prior group. Record history admission/eviction, active owners and speculative recency. Keep warm decoded disk with cold processing distinct from warm memory. |

Use representative Merrill and Quattro sizes plus the declared Tiger and lower-resolution Classic controls. Record recipes, image dimensions, window geometry, display density and native visible-area sizes. Keep fixed example recipes and action manifests in the repository. Start with 512 MB, 1 GB, 2 GB and 4 GB diagnostic budgets, then refine only where a measured reuse or latency transition warrants it. Do not change the application default during this investigation.

## Ownership and measurements

Inventory allocation, shared ownership, retention, lookup/reuse, invalidation, admission, eviction and release at the existing owners. Add missing instrumentation at those common boundaries rather than individual call sites. Count unique buffers and textures once. Keep required live data, reusable working sets, historical images and speculative data distinguishable without first inventing a new policy hierarchy.

Record logical retained and observed in-flight bytes alongside whole-process footprint. Boundary counts do not establish driver residency or within-node GPU peaks; process RSS does not establish GPU residency. Do not add overlapping CPU/GPU/shared-memory totals. Separate baseline application cost from per-photo resources and temporary replacement overlap.

For each cell report foreground delivery, complete foreground group, background settlement, source reads, GPU work, cache hits/misses, eviction/rebuild counts, steady retained bytes and observed peak footprint. Use repeated uncontended Off runs for latency and separate traces for attribution. Lower-budget runs must pass the same behavioral and numerical checks. Keep source-loading and native-session capacity limitations explicit.

## Current owner findings

The serial worker retains one native decoded source and `RenderSession`; switching to another native source replaces that session regardless of the byte budget. The [reduced working set](../crates/foveonic/src/render/reduced.rs) separately holds one active source-free session and inactive, actual-use LRU history. Speculative preparation enters history as the coldest item and uses spare capacity. Each active graph cache has a 1 GiB local bound. Shared-budget trimming sheds optional checkpoints, cold preparation and then inactive reduced sessions; protected active inputs and essential live resources can exceed the budget. Additional bytes alone cannot overcome the native-session slot or photo-count limits.

The owner agreed that native and reduced processing should use the same retention process under the shared budget. Full/Full, Thumb/Thumb, mixed quality and native detail from zoom or loupe must differ in resource demand rather than single/multi ownership policy. A photo may retain several resolution or region working sets with shared allocations counted once. Regional work should reuse available decoded/native preparation; Whole continuation remains lower-priority preparation. Replace the one-native slot in a separate measured slice, and test whether continuation displaces the regional state needed by the next edit.

The [photo owner](../crates/foveonic-gui/src/multi_edit_runtime.rs) retains a whole frame and a detail frame per retained photo, with bounded history and an extra transient loupe target. [Before](../crates/foveonic-gui/src/editor/before.rs) can retain Current while showing a separate rendition. Presentation snapshots, coordinator seeds, frontend frames and pending replies may share image buffers and textures, while genuinely different old/new and Before/Current results overlap. The [memory inventory](../crates/foveonic-gui/src/memory.rs) deduplicates shared image storage, metadata and texture identities and includes frontend pools in the shared reservation.

The [processing inventory](../crates/foveonic/src/render/memory.rs) reports logical retained CPU, GPU and reserved bytes, budget and excess; cache events identify checkpoint and reduced-session admissions, misses and evictions. GPU activity counts dispatches, transfers and allocation traffic, not peak residency. Boundary snapshots do not cover all decoder and prefetch activity, within-node scratch, driver allocations or transient peaks, and aggregated totals do not separate all active, historical and speculative owners. Process RSS and physical footprint provide different evidence; do not add either to logical CPU/GPU totals on unified memory. The existing [resource sampler](../scripts/profiling/resources.py) captures settled headless boundaries, not the GUI's transient overlap.

The [native scenario runner](../scripts/profiling/scenarios.py) already covers warm edits, switches, memory pressure, multi-photo selection and the transition journey; its existing Smart, Before and loupe actions provide a starting point. The [headless profiling tools](../docs/profiling.md) separately support completed regional/Whole timings, resource snapshots and exact-image comparisons. Add focused workflow checkpoints and separate latency and memory runs before drawing budget guidance: a broad journey's total includes unrelated actions and does not isolate these working sets.

## Deliverables

The first implementation slice measures two declared photographs in Smart: Fit, zoom the first to 200%, apply shared exposure offsets of +0.125, +0.25, +0.125 and +0.375 from each initial value, then return to Fit. Record each photo's first current image, foreground group completion and background Whole settlement independently. Exact UI admissions and later observations of retained images remain distinguishable. Capture image readbacks and fresh numerical verification after the timed workflow. Test-only result hooks and worker trace snapshots add no memory-usage display.

Start with 2 GB and 4 GB budgets, disk disabled and verified primed disk with fresh processing. Use unrecorded runs for latency and separate Wall captures for attribution. Wider workflows follow once this small report establishes useful resource and timing evidence. This slice changes no retention policy or default.

- [x] Map current cache/resource owners and their accounting boundaries.
- [x] Add only missing shared-boundary observations needed to distinguish the major operations.
- [ ] Run reproducible workflow/budget cases and identify working-set transitions.
- [ ] Produce practical budget ranges, including image-size and recipe qualifications.
- [ ] Explain residual latency that extra memory does not fix.
- [ ] Propose the smallest retention changes justified by those results for owner discussion.

Prefer guidance such as “this workload retains about X MB and stops rebuilding near Y MB on these photographs” over a universal photos-per-GB promise. Evaluate whether a simple base cost plus per-working-pixel and native-detail costs fits the measurements before proposing any automatic estimator. Implementation of a new cache policy requires a separate agreed slice.

## First two-photo results

Three uncontended Off trials per cell used the declared blue parking sign in color/Natural and hazy coast in monochrome/Natural. Smart Fit chose divisors four and eight respectively; the sign then requested native detail. Each cell contains twelve measured shared edits. All twenty-four final images matched fresh processing exactly. Reported medians combine these four declared edit actions; individual action samples remain available in runner output.

| Retained budget | Disk setup | Visible-group completion | Whole settlement observed |
| --- | --- | --- | --- |
| 2 GB | Disabled | 128 ms | 201 ms |
| 4 GB | Disabled | 69 ms | 144 ms |
| 2 GB | Primed | 125 ms | 205 ms |
| 4 GB | Primed | 71 ms | 143 ms |

Separate Wall controls confirmed no payload reads during the repeated edits. Smart Fit and initial native zoom read original payloads with disk disabled and decoded-cache payloads with primed disk. Native preparation remained on the same photo throughout this case. It does not test the one-native-session replacement limit.

At 2 GB, edit-boundary snapshots reached roughly 2.15 GiB before trimming and about 1.97 GiB afterward, with sixteen checkpoint evictions on the first edit and eight on subsequent edits. At 4 GB, retained snapshots reached roughly 2.17 GiB and each edit recorded four checkpoint evictions. These observations suggest useful working-set pressure near 2 GB for this recipe, rather than a disk bottleneck. They do not establish a universal budget or process peak.

The native region's backing allocation was about 86 MiB with an 8 MiB display texture; the Whole result referenced about 184 MiB backing and 225 MiB display texture. Regional backing includes processing support beyond the requested visible rectangle. These references can overlap other retained owners and must not be summed into a separate memory total. The next measurements remain Full/Full, Before/After and eight-photo/loupe workflows, followed by shared-budget native retention.

The first slice passed largest-model design/code review, 518 ordinary GUI tests, 134 portable profiling-tool tests, strict release Clippy, formatting and documentation checks. Four diagnostic cells saved complete, loss-free traces with matching action/resource attribution. Final Off and Wall integration runs exercised the maintained runner after report/comparison integration; Off milestone comparison accepts the resulting report. This completes the first measurement slice, not the wider workflow matrix or retention change.
