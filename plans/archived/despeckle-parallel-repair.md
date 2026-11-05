---
summary: "Eight parallel Merrill speckle repair rounds preserve ordered results while reducing low-threshold stalls."
status: archived
---

# Parallelize speckle repair

The owner approved eight parallel repair rounds with an ordered remainder, reviewed the results, and authorized cleanup, commit and merge. The implementation slice is complete and targets the next optimization release after the feature-focused 0.7.0 release. Requirements: [dynamic despeckling](../../docs/specs/despeckle.md) and [Merrill speckles](../../docs/specs/merrill-speckles.md).

## Agreed scope

Preserve immutable detection, camera-derived thresholds, bank capacities, the first retained records in each bank, and all repair arithmetic. Preserve each pair of neighboring repairs in native order. Keep Compact and the existing threshold controls unchanged. Do all scheduling and repair on the GPU without pixel readback. Reuse the existing image-sized marks and bounded record buffers.

Each round classifies pending records against earlier pending neighbors in a separate dispatch, then repairs the ready records in parallel. No two ready records are neighbors. After eight rounds, compact remaining records in native bank/row order and replay them with the existing sequential correction. The sequential remainder is part of the selected algorithm, including for chains longer than eight rounds.

## Steps

- [x] Implement bounded rounds with existing GPU storage and unchanged arithmetic.
- [x] Check exact samples against serial replay, long chains, cross-bank neighbors, capacity truncation, empty work and immutable inputs.
- [x] Compare original-file outputs and completed processing at thresholds 1 and 0.2.
- [x] Record integrated timings and resource activity, update the specification and run repository checks.
- [x] Owner reviewed the results and authorized cleanup, commit and merge.

## Investigation findings

On the rocks-and-tree-shadows Merrill photograph, threshold 0.2 selects about 2.42 million sensor pixels. Serial repair took about 6 seconds on an Apple M1 Pro. The accepted records form 23 dependency levels. Eight rounds leave 13282 records for ordered replay. At threshold 1, about 62627 records form three levels. These observations are specific to Sunlight/Standard with the default native corrections and do not establish a general latency target.

An isolated scheduling prototype matched every float sample. Its repair timings excluded detection, initial resource preparation and schedule uploads. Integrated measurements must include all node work and GPU completion. Low thresholds also select ordinary grain. Threshold range changes are outside this optimization.

## Implementation and checks

Readiness uses one spare bit in each existing record. The existing marks buffer becomes the pending layer mask after initial compaction and is reused for remaining-record ranks after the eight rounds. Classification excludes detections dropped by each bank's capacity before considering dependencies. All eight banks share the same pending mask, so adjacent repairs across a bank boundary retain their dependency. Repair arithmetic lives in one shader function shared by ready records and ordered replay. The parallel dispatch covers each bank's bounded capacity and skips entries beyond its GPU count, requiring no host count readback or indirect-dispatch resources.

Production has one fixed eight-round path. A four-pass serial executor lives only in the adjacent tests as the scheduling oracle. Exact synthetic comparisons cover signed dense layers, all layer masks, varied donors in long horizontal/vertical/upper-right chains, cross-bank neighbors, capacity 1/7/full banks, empty work and unchanged inputs. Resource counters confirm equal allocation counts, storage bytes and uploads, and no processing readback. This measures engine-owned buffers, not driver residency or process peak memory.

All six Merrill photographs match serial replay byte-for-byte at thresholds 1 and 0.2. The existing photographed-defect, preset/WB, Compact and cached-versus-fresh rendering checks pass. The full workspace tests, strict all-target Clippy, formatting, documentation indexes/checks and five artwork checks pass.

## Integrated measurements

Apple M1 Pro, Full Sunlight/Standard, default native corrections, no optional reconstruction controls. Source opening and GPU initialization precede all intervals. Image readback for exact comparisons follows completed processing and is outside timing. These are processing measurements, not native input-to-screen latency.

The original-file test compares complete despeckle calls against serial replay, including allocation, detection, scheduling, remaining-record compaction, correction and GPU completion. On rocks-and-tree-shadows, threshold 0.2 fell from 5937 ms to 86 ms, and threshold 1 fell from 222 ms to 30 ms. At threshold 0.2 the other five fixtures fell from 1584–3299 ms to 40–47 ms. These are one comparison per file, so use them as magnitude evidence rather than a latency distribution.

After builds and correctness checks, the existing resource profiler measured three alternating fresh sessions per threshold. After the first pair, processing through `despeckle.layers` completed in 249–253 ms at threshold 0.2 and 200–205 ms at threshold 1. Earlier unoptimized runs on the same source and recipe took about 6167 ms and 454 ms respectively. Fresh full photographic processing completed in 275 ms at threshold 0.2 and 224–227 ms at threshold 1, excluding display texture preparation and export. The two thresholds allocate the same engine-owned storage and perform no pixel readbacks in these wall-mode runs. Filesystem cache state and unrelated system workload are uncontrolled.

A separate synchronized diagnostic at threshold 0.2 recorded about 65 ms across all despeckle GPU passes, including 37 ms in the ordered remainder. At threshold 1 the remainder was empty. The eight classify/repair rounds remain separate timestamp samples and require no profiling-only changes in ordinary processing. Increasing the round count or building a general GPU scheduler is outside this slice.

## Cleanup

Removed the variable round count and serial-only branch from production. Named the layer, readiness and rank bits in WGSL, kept the arithmetic shared between ready repairs and ordered replay, and retained the serial executor only in tests. Disposable investigation sources were removed while local measurement reports were retained. Threshold controls, camera preparation and Compact are unchanged.

Final cleanup checks pass against the 0.7.0 mainline. One cache readiness test hit its five-second deadline during parallel checks, then passed in isolation and with the complete workspace suite using one test thread. No cache code changed. The next-release entry lives under Unreleased, and extraction confirms the 0.7.0 release notes are unchanged.
