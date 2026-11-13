---
summary: "Integrate measured preparation reuse, bounded reduced sessions, faster decoding and scale-aware previews, then verify presentation-only framing and architectural ownership."
status: archived
---

# Scaled editing optimization

The owner approved isolated agent worktrees, a shared Cargo build cache, integration commits and a final architecture review. The experimental preview and profiling baseline is `f927c26`. Implements [render preparation](../../docs/specs/render-preparation.md), [small previews](../../docs/specs/small-previews.md) and [profiling](../../docs/specs/profiling.md), preserving the existing [interactive](../../docs/specs/interactive-rendering.md) and [resource](../../docs/specs/render-resources.md) contracts. Unrelated help-example and release edits remain untouched.

## Completion

- [x] Reuse immutable preparation and compact exact operation keys with bounded ownership.
- [x] Seal eligible Merrill Draft/Thumb sessions into a byte-bounded reduced working set and use it across GUI photo switches.
- [x] Validate scale-aware Merrill, ordinary Quattro and supported Classic color paths without bypassing native corrections.
- [x] Finish decoder experiments, retain measured exact-output wins and remove rejected SIMD/paired paths.
- [x] Keep useful multi-photo, memory and graph-boundary profiling in maintained tools.
- [x] Verify presentation-only crop/rotation at unchanged Thumb and Smart resolution, including in-flight edits.
- [x] Complete integrated repository and native app checks, then the final independent architecture pass.

## Architecture and scope

Camera recipes own correction order and working-grid geometry. Numerical nodes receive explicit parameters and dimensions. Shared immutable keys, preparation, GPU ownership and RGB operations remain camera-independent. No Fuji decoder or speculative camera framework was added.

Preparation has one typed bounded owner. Immutable payloads retain their backing allocation, cache their rejection fingerprint and use exact equality after a fingerprint match. Tone curves are bounded across metadata owners. Native clipping preparation can be released independently. Byte inventories retain and deduplicate the allocations they count.

`ReducedSession` owns corrected reduced pixels, metadata, effective upstream identity and downstream checkpoints. It does not pretend reduced pixels are an original source. An unsupported branch or changed upstream/analysis dependency explicitly requests native input. At that checkpoint, `ReducedWorkingSet` owned the budget and LRU. GUI loading reuses the existing decoded-source loader and verified identity snapshots. Quattro retains reduced GPU frontiers through its ordinary native session, which still owns the original CPU samples.

The owner considered replacing the persistent decoded-source disk cache with corrected Fast sensors as a future idea and explicitly requested no action. Disk-cache policy is unchanged. Stage-major execution across photographs remains a separate future attempt. This pass measures existing whole-photo concurrency without changing production submission or wait policy.

## Preparation results

Matched optimized headless runs used eight distinct Merrill originals, identical settings and owned before/after executables. Each edit completed GPU work and display-texture preparation with profiling Off. Separate Wall runs attributed costs. Verification readbacks occurred after timing. Every final image matched an independent fresh render, old/new checksum sets matched, and recorded GPU activity counts were unchanged.

| Settled eight-photo edit group | Before | After |
| --- | ---: | ---: |
| Exposure, one worker | 129.96 ms | 29.04 ms |
| Exposure, two workers | 76.59 ms | 22.35 ms |
| Exposure, four workers | 56.20 ms | 22.65 ms |
| Reconstruction change, one worker | 197.34 ms | 112.81 ms |
| Reconstruction change, two workers | 142.23 ms | 102.81 ms |

Final Wall medians were 0.467 ms for total recipe preparation, 0.00158 ms for tone preparation and 0.302 ms for checkpoint planning. Nested spans overlap. CRC32 is only a rejection fingerprint, with exact fallback. It removes the first candidate's cold large-payload hashing regression. Cold clipping preparation plus planning is approximately unchanged from the old combined cost.

## Reduced ownership and memory

An eight-photo Thumb trial with a 128 MiB working-set budget retained 46,255,104 bytes of GPU buffers and 67,970,810 bytes of CPU metadata/identity. Combined deduplicated ownership, including engine preparation, was 114,226,082 bytes, about 109 MiB. A separate final run verified all eight cached images exactly against fresh processing without warm reloads. Upstream invalidation, changed sources, LRU eviction, native promotion and active Smart detail-to-Full reuse have dedicated checks.

Separate whole-process trials included loading, warmup and forty exposure groups. Peak macOS footprint fell from 4.94 GiB with eight native sessions to 1.91 GiB with the reduced set. The earlier one-native-photo peak was 1.80 GiB. These are process high-water observations, not retained-cache totals or GUI predictions. The app has additional display, prefetch and background-writer owners.

Quattro's six-photo Thumb retained GPU buffers fell from 804.1 to 87.23 MB for exposure and 847.2 to 65.17 MB for reconstruction changes. The first reconstruction-changing group fell from 1484 to 543 ms. Separate Wall checks executed no native-prefix or pooling nodes on the first edit. All six final images remained fresh-byte-exact. Original CPU samples remain owned by ordinary Quattro sessions.

## Scale-aware recipes

Draft is one quarter of native width and height. Thumb is one eighth. The existing `FOVEONIC_SMALL_OVERVIEW=1` opt-in remains required. No release default was changed.

- Merrill applies selected native row correction before pooling and scales high-pass support in physical native-pixel units. Reviewed Full/Fast behavior remains exact.
- Classic pools calibrated signed codes with explicit symmetric rounding, preserves odd native edges and adapts recursive IIR distances. Six full-resolution originals cover DP1s, DP2, SD9, SD10, SD14 and SD15. High-ISO range reconstruction, reduced RAW inputs and monochrome retain Full fallback.
- Ordinary Quattro HIGH preserves native RGB/blue repair, shading and highlight evidence before coordinated RGB, blue and mask pooling. Existing color/noise reconstruction continues on the working grid. Tiger, LOW and monochrome small grids retain established fallback.

The reduced Classic recursive filter approximates native boundary transients. Interior flat response is within two codes in the numerical fixture, channel symmetry is exact, and an example boundary response changes from 512 to 504. This approximation is distinct from exact-output cache optimizations. Fitted photographic comparisons show coherent framing and color, with expected Thumb detail loss. Full/Fast comparisons against pre-scale outputs remain byte-exact across the tested families.

An integration regression caught internal color guides being forced to Full while the photograph used Draft/Thumb. Internal guides now use the same working grid. Public inspection remains Full. GPU coverage checks all three families, both scales, basic and Advanced Color, changed settings and fresh equality.

## Decoder investigation

The [completed decoder plan](./decoder-optimization.md) and [research report](../../docs/decoder-optimization-research.md) contain methods and limits. A fused short Huffman prefix/payload table reduces complete opening by 27–43% across four measured originals while preserving all 29 available original sample arrays. Stable NEON predictor reconstruction is faster in isolation but loses end-to-end after an additional image buffer and pass. NEON interleaving gives negligible benefit. A bounds-compatible paired table regresses three of four measured inputs and was removed.

Classic format-6 rows have independent predictor resets and real directory boundaries. Bounded row decoding reduces complete SD9/SD10/SD14 opening from 98.5/99.5/135.3 ms to 27.1/28.5/37.4 ms, about 3.5–3.6 times faster. Foreground work uses at most four inherited workers. Speculative work uses only the existing configured FIFO pool. Output slices are disjoint, every task joins, and row-order errors and cancellation are preserved. Separate SD14 peak footprint increased about 208 KiB and peak RSS 368 KiB, without an additional full-image allocation.

## Profiling and validation

The final independent code audit at `3d77e44` found no remaining correctness or ownership blocker and recommended no further abstraction. Recipes, numerical operations, execution, resource ownership and presentation retain separate responsibilities. Rejected decoder paths were removed from production. Test-only serial/category comparators remain for reproducible exactness checks.

Crop/rotation use one worker-owned immutable whole-image result. Regional and output-sized results cannot supply that plane. Source, root, rendition, calibration and workspace policy guard reuse. Actual Worker GPU checks show zero source opens and zero photographic dispatches for framing across Thumb, Smart Fast plus a completed regional Full result, and Smart Full. Display bytes match fresh composition. Pending exposure work remains numerical, explicit quality changes still render, and a new explicit zoom can request Full while framing. The viewport owns zoom intent, so Fit or entering a tool does not create a detail request. Rotation cursors are half their former size with a centered thin black outline.

The maintained editing harness distinguishes first and settled edits, requested/effective quality, source setup, per-photo and group completion, native versus shared reduced ownership, budgets and reloads. Host timeline exports show graph-node overlap without inventing a GPU-to-host clock mapping. Off avoids optional recording. Wall adds no GPU waits. Memory sampling and fresh readbacks stay outside acceptance timing.

These local measurements used optimized binaries on an Apple M1 Pro with uncontrolled OS/GPU activity. They establish neither cold removable-card throughput nor physical input-to-screen latency. Owned evidence binaries were hashed before shared-target rebuilds. Measured work and builds were serialized across worktrees.

The integrated workspace suite passes 863 tests. Workspace all-target Clippy with warnings denied, formatting, documentation indexes, 22 profiling-tool tests and five artwork tests pass. Final root GPU reruns pass reduced-session native release/fresh edits, exact Auto Haze reuse, working-set eviction, preparation invalidation and Smart region-to-Full reuse. Track checks additionally cover scale numerics, old Full/Fast goldens, three-family color guides, worker framing, native promotion and Full exports.

An optimized native bundle passes Crop, Rotate and Smart/cache smoke workflows with 51 captured frames. Crop/rotation entry, commits and exits record presentation reuse with zero photographic dispatches. The smaller cursor and framing were visually reviewed. Only the owned earlier review process was closed. The replacement review bundle uses the final integrated code. All ten protected help/release file hashes remain unchanged.

The metadata-only working set currently seals supported Merrill color branches. Active Adaptive, Dehaze and color-guide branches use ordinary native sessions, as do Classic and Quattro. Their supported Draft/Thumb processing remains available, but the Merrill memory result must not be generalized to those owners. Further family sealing, stage-major scheduling, persistent-cache changes and enabling a release default are separate decisions. This completes the agreed investigation and integration scope. No release or push is part of this attempt.
