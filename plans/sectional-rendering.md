---
summary: "Reduce native sectional rendering work through explicit input support and measured Whole comparisons."
---

# Sectional rendering

The owner approved an optimization loop after a loss-free manual eight-photo capture showed two zoomed exposure edits rebuilding native sessions under the 2 GB memory budget. Improve native sectional rendering first. Keep the mechanism suitable for other working scales, preserve Whole rendering, and compare sectional output against the same region of an independently rendered Whole photograph. SSD checkpoint spilling remains an unselected alternative.

Follow the [regional bounds inventory](./regional-bounds.md), [regional completion findings](./regional-preview-completion.md), [viewport requirements](../docs/specs/viewport-workflows.md) and [measurement contract](../docs/specs/performance-diagnostics.md). Keep recipe construction, numerical processing, scheduling and cache ownership separate. Do not combine this work with wide execution or a scheduler rewrite.

## Baseline findings

The final manual exposure edits requested regional native output, not Whole fallback. Their processing extents after tile projection were 3688 by 3264 and 2400 by 1872, compared with 4928 by 3264 Whole. Both numerical caches started empty following shared-budget session eviction. Worker elapsed times were 1318 and 700 ms; request-to-UI delivery was 1981 and 2739 ms. Subsequent Whole tasks were explicitly speculative and followed foreground completion. Session eviction also caused repeated source acquisition and native preparation. The decoded-cache hit did not avoid sensor processing.

The graph previously recorded output dependencies but had no general backward mapping from requested output bounds to required input bounds. Merrill recipes apply region parameters to reconstruction after native sensor corrections; some operations already expose compact regional tile windows. A reported full-sensor geometry on opaque tiles does not alone prove full-image work: compare dispatch dimensions, allocation sizes and projected output geometry.

## Approved prototype

Each operation maps requested output demand to required demand on each input slot. The graph walks backward and unions consumer demands per output, initially using one bounding rectangle with Unused and Whole alternatives. Whole describes the output's own domain, including small tables. A forward pass selects concrete storage and resolves consumer addressing before ordinary cache planning. Cached Whole storage can satisfy regional demand. Resolution operates once on a freshly assembled graph. This prototype starts at the existing regional reconstruction boundary, with earlier unconverted operations conservatively requiring Whole.

The first converted chain is Merrill row Banding into NoiseBase and Column. Reconstruction owns tile support and physical sampling rules. Banding owns contributing patch rows and correction storage. Camera recipes retain their established construction and lazy metadata preparation. The prototype does not replace those contracts with recipe-specific upstream support handoffs.

## Prototype checks

- [x] Capture native cold-processing baselines with decoded source held constant, and matched warm exposure edits.
- [x] Inventory the first chain's finite neighborhoods, tile alignment and genuine global statistics.
- [x] Implement backward per-output demand, branch union and forward storage selection, with numerical support owned by operations.
- [x] Compare center, edge, tile-seam and bank-boundary regions against independent Whole output at native and reduced grids.
- [x] Measure completed foreground processing and allocations separately from diagnostics and Whole continuation.
- [x] Obtain independent largest-model architecture and code review and address findings.
- [x] Finish ordinary checks and package the native review build.
- [x] Record the reviewed prototype checkpoint.
- [ ] Obtain owner review before another functional slice.
- [ ] Use owner feedback and the remaining cost breakdown to choose the next operation for migration.

## Results and boundaries

Three uncontended Off trials per photograph use the maintained blue parking sign, hazy coast and speaker inputs with a center region covering one tenth of each output axis. Both binaries use identical resolved settings and retained decoded sources. Timings exclude source decode and previous-image preparation. The profiler alternates regional-first and Fast-first routes and discards one warmup pair. The table reports only regional-first foreground and subsequent Whole completion.

| Photograph | Foreground before / after, ms | Whole continuation before / after, ms |
| --- | --- | --- |
| Blue parking sign | 392 / 349 | 439 / 496 |
| Hazy coast | 420 / 383 | 487 / 539 |
| Speaker | 367 / 332 | 387 / 442 |

The first migrated operation reduces foreground processing time by about 9–11%. Total foreground plus Whole time increases by roughly 1–3%. Whole now performs row correction for the larger domain rather than reusing a full correction that delayed regional delivery. Earlier Whole sensor prerequisites remain reusable. Warm regional exposure edits stay near 5 ms in the matched five-edit runs.

Separate Wall attribution on hazy coast measures Banding at approximately 76 ms before and 45 ms after. Its corrected layer shrinks from 184 MiB to 52 MiB. Cumulative node allocation traffic falls from about 384 MiB to 173 MiB. Those allocation counts are not process footprint or transient residency measurements. The region-resolution pass takes approximately 0.007 ms in these cold diagnostic graphs.

Compact Banding declares that an existing optional session checkpoint should retain its broad input instead of the compact output. This preserves the unchanged Despeckle prerequisite for Whole continuation, without retaining both correction layers as recipe hints. Exact cache probing preserves an existing compact result first, then a valid Whole result. It does not mutate recency or planning state. Required source cuts retain their existing ownership.

The graph tests cover shared-branch union, repeated input bindings, Whole table demand, unsupported/global operations, unused outputs and storage selection. GPU tests require exact compact-versus-Whole Banding and raw-tile/column samples across native and reduced grids. Real-photo checks cover all camera quarter turns, native tile seams, global noise context, continuation and immutable earlier images. Adaptive Natural/Strong, authored rotation, monochrome and multiscale regional comparisons pass their existing assertions. All forty frozen Whole Full/Fast/Draft/Thumb comparisons remain bit-exact across three Classic, three Merrill, three Quattro and one Tiger input.

The camera recipes still map viewport coordinates and construct their established regional reconstruction geometry. The new resolver owns the upstream demand and storage handoff for the first converted sensor chain. Earlier highlight recovery and ordered despeckling conservatively require Whole input. Directional and global-context reconstruction keep their complete evidence domains. Further migrations should add operation rules and tests without adding camera-recipe support handoffs or broadening the graph traversal.

Prefer exact sectional samples where arithmetic and sample order are unchanged. Any near-exact allowance needs a stated reason and measured error, especially recursive filters, changed tile phases or global estimators. Whole Full/Fast/Draft/Thumb output must retain the existing frozen comparisons unless the owner approves a numerical change. Use checked-in fixture declarations and release photographs for maintained tests; keep generated measurements outside version control.

The prototype passes 547 ordinary library tests, eight focused GPU tests, the forty-case frozen rendering matrix, strict release workspace/all-target Clippy, formatting and generated-index checks. The native release app is packaged for owner testing. No scheduling, disk-cache policy or processing outside the converted chain changed.

Owner review found slow first native loupe requests across Quattro inputs. The initial demand prototype converts a Merrill chain, so Quattro remains the next profiling target. Review also exposed a separate [Pointer context-lock deadlock](./archived/selection-context-lock.md), now repaired with a failing-before/passing-after regression. Continue sectional performance measurements after the owner can review the responsive build.
