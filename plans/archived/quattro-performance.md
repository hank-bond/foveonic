---
summary: "Completed output-equivalent repair and cache optimizations cut Quattro render latency roughly threefold."
status: archived
---

# Optimize Quattro rendering

## Scope

The owner reported roughly four-second Quattro rendering and requested a speed pass. Preserve default and adjusted photographic output, ordered repair dependencies, camera calibration and the functional recipe/node separation. Profile completed fresh processing separately from edit-session reuse, decoding and export. The owner also requested that an unedited file default to its recorded white balance; preserve explicit and saved choices.

## Steps

- [x] Freeze a baseline executable and capture exact float goldens for DP2, sd, DP3 ISO800 and DP2 luminance-noise step two.
- [x] Measure completed unprofiled cold/warm processing and separate synchronized GPU diagnostics.
- [x] Parallelize independent RGB repair records while retaining source-order dependencies.
- [x] Check blue repair and luma cluster workgroup scheduling against native fixtures and baseline pixels.
- [x] Improve the initial functional edit checkpoint and verify cached results against fresh rendering.
- [x] Resolve default WB from file metadata and verify explicit choices remain intact.
- [x] Compare the optimized full pipeline against the frozen goldens, measure final latency and run workspace checks.

## Baseline

Reused engines and decoded sources still execute fresh processing. Two warm renders follow the first render; a fourth render collects GPU timestamps separately. Export and float readback are outside processing intervals. Exact float goldens are retained until verification finishes.

| Case | Warm completed processing, ms | Directional RGB repair, ms | Masked RGB repair, ms | RGB clusters, ms |
| --- | ---: | ---: | ---: | ---: |
| DP2 | 3411 | 1376 | 407 | 181 |
| sd | 3109 | 1306 | 322 | 92 |
| DP3 ISO800 | 3584 | 1604 | 355 | — |
| DP2, luminance noise two | 3301 | 1363 | 405 | 181 |

The timings identify serial RGB defect catalogues as the first optimization target. Dependency batches may run concurrently only when their read neighborhoods and writes cannot interact. Arithmetic, strict comparisons and record ordering within dependent chains must remain unchanged.

## Implementation findings

RGB repairs now schedule independent record levels, preserving earlier writes for neighboring and duplicate records. Cluster matching dispatches independent search regions concurrently. Ordered blue repair distributes candidate scoring across a workgroup, with stable source-order ties. Mono clusters cache their target patch and use 256 lanes while retaining score accumulation order. Native full-plane and bounded fixtures pass, and the first optimized full pipeline matches every baseline float sample in all four cases.

A bounded dense dependency table reduces chroma metadata preparation from 47.36 to 5.30 ms per call. Larger planes retain sparse scheduling. A 20,000-record comparison verifies identical levels and ordering for both storage choices. Preparation remains stateless; the change adds no node-level memoization.

Initial interactive checkpoints recognize the functional blue-guided reconstruction boundary as well as the existing detail boundary. All outputs of the chosen producer are marked dirty, preventing unnecessary residual retention. Cached tone, detail, luminance and WB edits match fresh outputs exactly. The initial checkpoint retains about 368 MB. File WB defaults now wait for the camera catalogue and use the recorded preset; explicit choices remain intact.

## Final measurements

Completed full processing with a reused engine and decoded source, without reusing image results:

| Case | Before, ms | After, ms | Speedup |
| --- | ---: | ---: | ---: |
| DP2 | 3411 | 1020 | 3.3× |
| sd | 3109 | 827 | 3.8× |
| DP3 ISO800 | 3584 | 1133 | 3.2× |
| DP2, luminance noise two | 3301 | 978 | 3.4× |

All four full-resolution floating outputs are byte-identical to the frozen baseline at the same explicit settings. Recipe construction is now measured separately: roughly 90–115 ms in the final diagnostic runs. The largest remaining DP2 GPU stages are mono clusters and RGB neighborhood filtering. These measurements exclude source decoding and export.

All 412 ordinary workspace tests, strict Clippy, formatting and documentation checks pass. Additional GPU checks cover native full repair boundaries, parallel-versus-serial ordering, recorded-WB fallback, and cached high-ISO noise/detail edits. Benchmark TIFF duplicates were pruned after verification; float goldens, timing records and final review exports remain available locally.
