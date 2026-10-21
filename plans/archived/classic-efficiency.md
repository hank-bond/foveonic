---
summary: "Output-equivalent Classic optimizations reduce full SD15 processing from 749 to 173 milliseconds."
status: archived
---

# Classic pipeline efficiency

Implements the owner-approved efficiency pass after the [Classic cleanup](classic-cleanup.md). Work remains isolated and uncommitted for review under [Classic rendering](../../docs/specs/classic-rendering.md).

## Work

- [x] Retain full-stage and final-output baselines before changing execution.
- [x] Replace serial flare sample fetching while preserving ordered Float32 accumulation.
- [x] Parallelize independent highlight work and skip unaffected rows and columns.
- [x] Tile ordered despeckling with exact dependency halos.
- [x] Reuse reconstruction scratch and improve its memory layout.
- [x] Reuse invariant host lookup tables.
- [x] Complete full-stage and original-file comparisons for the final candidates.
- [x] Measure completed wall time, separate GPU diagnostics and allocation totals.
- [x] Run repository checks and remove redundant experimental outputs.

## Measurement contract

Use release builds on the same Apple M1 Pro. Fresh renders start with a decoded source and a reused engine, without edit-session result caching. Sunlight/Standard, reconstruction, repair, highlights, despeckle and unit sharpening are enabled. Measure completed wall time with GPU diagnostics disabled, keeping timestamped runs separate. File decoding and TIFF export are separate costs. Warm timings do not include engine initialization or shader setup. Numerical comparisons occur outside timed intervals.

The pre-optimization SD15 median is 748.874 ms across three warm fresh runs. Its separate GPU diagnostic totals 706.452 ms, led by highlight streaming at 312.173 ms, flare statistics at 218.996 ms and despeckle at 62.872 ms. The physical buffer contains 4.816896 megapixels. The same Merrill comparison is 1046.985 ms over 16.084992 processed megapixels. These are selected photographs and settings, not universal camera-family scaling claims.

## Selected changes

Flare statistics originally loaded and accumulated every sparse sample in one GPU invocation. Cooperative tiled fetches now feed three independent channel workers. Within each channel the original addition order is preserved, including Float32 rounding and the Q14 product residual. An unordered sum is not an equivalent substitute. A synthetic regression distinguishes ordered accumulation from a regrouped exact integer sum and covers a partial tile. Larger shared tiles did not improve measured time enough to justify their resource use.

Highlight neutralization is independent per input pixel and runs in parallel. Bitpacked full-saturation flags and row summaries identify the affected ordered work. Seven-row summaries cover the current smoothing stencil and delayed writes. Horizontal bounds restrict that work, with unchanged RGB used when a delayed lookup falls outside a previously computed interval. The original integer arithmetic, dependency order and final unflushed rows remain intact. Extra storage is a bit per pixel plus small row summaries and the original three-row scratch.

Despeckle uses independent 192-column cores with 32-column halos and 32-row batches. Each batch reads the already completed preceding row. Within a tile, repeated halo computation retains every dependency that can reach the output core. Original horizontal and lower neighbors remain unchanged inputs. The scheme supports the full signed16 domain without packing assumptions. Ordered defect records remain sequential.

Log reconstruction reuses one forward scratch buffer across six recursive filter pairs instead of allocating six. Scratch uses transposed indexing so adjacent row workers access adjacent records. Source, guide, output, recurrence order and floating operations remain unchanged. The reuse saves five allocations and 385,351,680 cumulative allocation bytes for SD15. Cumulative allocation is not peak GPU residency.

Invariant exponential, root and photographic response base tables use immutable `OnceLock` storage. Prepared table contracts and exact operation keys remain unchanged. This avoids repeated transcendental table generation without introducing a general caching layer.

## Validation boundaries

The first combined candidate retained all twelve complete SD15 intermediate buffers exactly, including calibration, highlights, despeckle, reconstruction, detail and final working RGB. Native bounded fixtures additionally check stream tails, bulb behavior, signed repair and recursive-filter borders. An adversarial despeckle fixture crosses column tiles and row batches with signed16 extremes.

The first candidate reduced median processing to 245.603 ms. Separating flare channels and prefetching ordered additions reduced it further to about 215 ms. Transposing reconstruction scratch brought the measured median to about 200 ms. Final highlight bounds and workgroup choices remain subject to the final measurements and checks below.

## Final measurements and evidence

The retained build uses 256-thread flare and highlight workgroups. A 4096-sample flare tile did not beat the 256-sample tile. Highlight workgroups of 32 and 64 threads also did not improve the measured stage enough to replace 256. These trials preserve exact arithmetic and were rejected on measured performance rather than appearance.

| SD15 measurement | Before | After |
| --- | ---: | ---: |
| Median completed fresh render | 748.874 ms | 173.074 ms |
| Separate diagnostic GPU total | 706.452 ms | 132.287 ms |
| Flare statistics | 218.996 ms | 12.613 ms |
| Highlight neutralization | 312.173 ms | 11.618 ms |
| Ordered despeckle | 62.872 ms | 8.519 ms |

The completed render improves by 4.33 times, or 76.9 percent less time. Final warm SD15 samples range from 162.335 to 188.882 ms. An earlier complete final candidate measured a 166.250 ms median. Report the variation rather than treating a best sample as a guaranteed latency. Final physical-buffer normalization is 35.931 ms per megapixel, compared with the earlier 155.468 ms baseline.

DP2 and DP1s final medians are 207.869 and 165.560 ms respectively. Their selected photographs have different highlight workloads, so the SD15 speedup is not a universal family claim. The Merrill control's working float digest remains unchanged. Its final median is 1153.330 ms versus the earlier 1046.985 ms, while separate diagnostic GPU totals remain similar at 932.821 versus 927.300 ms. The control variation limits cross-run wall-time precision and does not indicate a changed Merrill algorithm.

All twelve complete SD15 intermediate buffers remain byte-identical, covering 655,097,856 bytes. SD15, DP2 and DP1s final TIFFs remain byte-identical to the pre-optimization shared-finishing goldens. The reference implementation comparison retains the prior 661 one-code reconstruction differences and the same final RGB16 difference bounds. Synthetic flare checks exercise partial tiles and distinguish sequential Float32 rounding from reordered summation. Despeckle fixtures cross tile and batch boundaries. Highlight checks retain dense/sparse, bulb and final-row behavior. Source buffers remain immutable.

No-op input scaling reuses the original integral sensor buffer rather than creating an unchanged image. A dedicated GPU comparison verifies equality with a unit multiplier and unchanged source samples. This is a functional optimization and does not select camera families.

Remaining work is dominated by logarithmic reconstruction, whose forward/backward recurrences total about 51 ms in the final SD15 diagnostic. Additional input-packing passes or a more elaborate highlight component scheduler were not added. They would introduce buffers and scheduling complexity beyond the demonstrated benefit. Reconstruction scratch reuse saves five full scratch allocations, while bitflags and column summaries add only small working storage. Exact keys and destination encoding remain unchanged.

Redundant benchmark TIFFs and duplicate post-change full-stage dumps were deleted after hashing and comparison, reclaiming about 3.03 GB. One pre-change stage set, numerical reports, final photographic goldens and portable fixtures remain available for review and further measurements. Allocation totals are cumulative and must not be reported as peak memory.

Workspace tests, bounded and original-file GPU checks, source immutability checks, Clippy with warnings denied, formatting, generated indexes and diff checks pass. Changes remain uncommitted in the Classic experiment worktree. Main remains unchanged.
