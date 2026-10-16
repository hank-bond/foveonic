---
status: archived
summary: "Completed full-pipeline profiling and output-equivalent optimization reduced six-photo render latency roughly five to seven times."
---

# Profile and optimize the pipeline

## Agreed scope

The owner reported unacceptable processing latency and approved a pivot: finish only the minimal in-flight wiring needed for a buildable baseline; measure cold/warm wall time, GPU stages, synchronization and memory with reconstruction off/on; optimize the largest measured bottleneck and check output equivalence after each change. Algorithm expansion in [the completion plan](../complete-reference-algorithms.md) is paused. The owner authorized branch `perf/pipeline-latency` and commits for successfully checked changes, with final difference review afterward. Keep cleanup, profiling-only work and optimization in separate commits; review diffs and validate before committing. Work autonomously through measured, output-equivalent rounds until returns diminish or complexity outweighs the incremental gain. Profile the full pipeline with all processing enabled. UI responsiveness and caching are follow-up work, not part of this pass. Record output-changing alternatives for later owner review. Installs, reference edits and evidence deletion remain unauthorized; preserve concurrent GUI work.

## Steps

- [x] Stabilize partial shared noise/ColorDQ wiring; check shared GUI/CLI callers and record baseline semantics.
- [x] Commit the validated cleanup checkpoint, excluding unrelated GUI work.
- [x] Capture completed, unprofiled cold/warm renders, a separate timestamped stage breakdown, and process peak memory across six originals with all processing enabled.
- [x] Capture/review/commit profiling-only work before implementing optimization.
- [x] Optimize the largest measured stage with equivalent output; preserve photographic selection/order, correction controls, and retained ownership.
- [x] Verify component/native and photographic output equivalence; rerun the identical benchmark and quantify improvement.
- [x] Review the next bottleneck, latency/synchronization/memory tradeoffs, and remaining performance limits.
- [x] Run workspace/GPU/GUI/CLI/strict/documentation checks and commit successful rounds on the branch.

## Baseline constraints

Timestamp profiles currently resolve/read back per operation and therefore synchronize frequently. Use completed unprofiled wall time as the primary latency measurement; report profiled times separately. Wait for the queue before declaring render completion. Engine creation/decoding, rendering, display texture preparation and readback/export are separate costs. Warm means a reused engine and decoded source; it does not imply image-result caching.

Earlier measured expensive stages are ordered camera records (~0.79 s), masked cluster matching (~1.90 s), and scene-dependent highlight donor accumulation (up to ~2.10 s). These are hypotheses until the current build is measured. Do not replace dependent repairs with simultaneous updates or unordered donor statistics merely to improve time. Parallel candidate evaluation/reduction can retain sequential member writes and strict first-best tie ordering.

## Progress

Minimal wiring retains native automatic ColorDQ only with opt-in reconstruction, manual `Some(gain)` with 0 Off, and shared metadata-derived despeckle dose using the actual prepared calibration WB factors. GUI/CLI/catalog callers are migrated. This is stabilization of existing partial work, not authorization to continue adding resampling or other algorithm stages. Current scale tables give dose 1 for the six originals. Ordinary workspace tests, strict all-target Clippy and release build pass. All 40 ignored core GPU/fixture tests and the GUI worker test pass. Thirty native recipe cases now check despeckle dose/ColorDQ limit as well as reduction/window. The new fixture assertion caught the in-flight ColorDQ column mixup: native column 7 is the cap, not column 12 (sharpening base). Corrected before the baseline checkpoint; preliminary reconstruction hashes before this fix are not equivalence goldens. Automatic ColorDQ is bit-identical to its explicit calculated-gain route, including retained-image checks. Non-unit despeckle dose, invalid dose and geometry-independent coupling preparation are checked. These preliminary diagnostics precede optimization. A submitted render is not a completed render: preliminary default blue-sign submission is ~80 ms while completion is ~3.25 s, mainly cluster (~1.9 s) and ordered record (~0.8 s) GPU work; flower defaults complete in ~5.33 s with an additional ~2.10 s GPU donor-table loop. These are initial findings, not the finished profiling checkpoint. Image data stays device-resident through processing; timestamp resolution reads small timing buffers and introduces synchronization only with profiling enabled.

Cleanup is reviewed independently from the concurrent GUI work: selectively staged controls/smoke API migrations compile and pass workspace tests and strict Clippy. Forty core ignored GPU/fixture tests and the GUI worker test pass in the actual tree; staged core sources are identical. Documentation generation/check, formatting, diff checks and eight Python tests pass. Owner viewport/help/scopes/inspection work stays uncommitted.

## All-enabled baseline

Cleanup checkpoint: `43fcd2e`. The reproducible example enables camera repair, highlights, despeckle, row correction, reconstruction (including column/pattern), automatic ColorDQ, native sharpening 0.75, gain 1.3, Exposure 0.5, nonneutral tone/saturation, and oriented 800×1200/1200×800 encoding/resizing/TIFF. Three completed unprofiled renders are followed by a separate timestamped render. Warm uses the same decoded source and engine, not result caching. New-process setup may reuse the driver's persistent shader cache; the first engine took 1022 ms, subsequent engines 44–54 ms.

| Original | Decode ms | First render ms | Warm median ms | Timestamped ms | Warm export ms | Process peak MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| blue-parking-sign | 268 | 4156 | 3912 | 3980 | 13 | 889 |
| glossy-leaves-and-buds | 243 | 4034 | 4114 | 4143 | 15 | 1040 |
| rocks-and-tree-shadows | 242 | 3695 | 3697 | 3825 | 21 | 922 |
| sunlit-red-flower | 240 | 6222 | 6244 | 6288 | 13 | 1054 |
| turquoise-still-life | 243 | 3923 | 4125 | 4089 | 13 | 1053 |
| yellow-flowers-and-stroller | 243 | 5666 | 5729 | 5727 | 16 | 895 |

Every unprofiled render has **zero CPU readbacks**, 62–64 compute passes, 64–66 engine submissions, ~264 MB upload (source and prepared tables), and ~275 MB device-local copies. Export adds one 7.68 MB readback; timestamp profiling adds 18–21 reads totaling just 992–1024 bytes, plus resolve submissions. Diagnostic golden readback is outside timing and activity. The latency is dominated by serial GPU work, not intermediate image round-trips. Total wrapper storage allocation is 4.89–5.67 GB per render; this is cumulative allocation, **not peak residency**. RSS also does not fully represent Metal device storage.

Largest common stages: masked clusters ~1.89–1.90 s, ordered camera lists ~0.79 s. Flower/yellow donor tables add 2.10/1.47 s. Next costs include edge weights (55–365 ms), row correlation/fits (combined across banks), directional local means, bilateral, and scene-dependent despeckle. GPU sums account for 3.61–6.09 s of timestamped runs; remaining preparation/submission/waits are materially smaller.

Use the [profiling workflow](../../docs/profiling.md) for current benchmark commands. Initial goldens are created only when absent. Further rounds fail on any changed full-buffer byte. Small exports also permit direct encoded-TIFF comparison. Counters describe engine-owned work, not hidden driver staging or other shared-device users. Wrapper counter tests, workspace tests, strict Clippy and documentation/format checks pass before this instrumentation-only checkpoint.

## Round 1: cooperative cluster candidate search

Independent candidate searches run in one 64-lane workgroup; clusters/member updates remain sequential with storage barriers. Each lane retains native float accumulation order. Reduction chooses lowest cost then original row-major/mapping rank, preserving strict first-best ties. Host metadata supplies a uniform maximum member loop bound; unused slots do no repairs. No new submissions or readbacks.

GPU matching fell from ~1890–1900 ms to 29.5–29.8 ms. Completed warm all-enabled renders now measure 1994/2196/1850/4377/2174/3752 ms (blue/leaves/rocks/flower/turquoise/yellow), versus 3912/4114/3697/6244/4125/5729 ms. Six full-sensor float buffers and all six resized TIFFs are **byte-identical** to baseline. Native/order/capacity/dark-defect component regressions, explicit cross-lane first-tie/no-candidate/uniform-padding tests, workspace and strict Clippy checks pass. Work remains GPU-only with immutable earlier artifacts.

## Round 2: stable GPU donor bin grouping

A direct per-bin private recurrence preserved exact output but only lowered flower/yellow table work from 2104/1468 to ~1379/966 ms. Splitting workgroups gave little further benefit. Materializing floating donor ratios in compaction failed the strict flower image comparison and was rejected (likely changed contraction/rounding; not fully isolated). Caching bin labels alone was exact but slower.

The selected path uses integer block histograms/prefixes and stable scatter, retaining each bin's original donor sequence. It reuses old rank scratch for bin labels after compaction completes. Ratio arithmetic remains inline in ordered summation. Source pixels/maps are unchanged; no photographic host arithmetic or readback is added. Two integer buffers add ~128.7 MB cumulative storage, and four passes/submissions replace the long serialized table loop. Flower/yellow table work is ~421/211 ms, plus ~14/11 ms grouping; all-enabled warm renders are 2059/2123/1859/2648/2073/2470 ms (blue/leaves/rocks/flower/turquoise/yellow). Common scenes add ~1–2 ms grouping overhead. Further per-bin serial latency is visible and can be investigated after the camera-record bottleneck.

All six full float buffers and TIFFs remain byte-identical to the original all-enabled baseline. Stable grouping across block boundaries/partial tails/empty bins, original saturation/connectivity/density/gain/selectivity, all **43 core ignored GPU/fixture tests**, GUI worker, ordinary workspace and strict Clippy checks pass.

## Round 3: independent original-signal bad-red detection

The old ~790 ms camera kernel combined immutable-input detection over 143827 records with ordered correction. Split independent per-record detection, capped integer prefixes and stable scatter/mask overrides from the dependent correction loop. Duplicate detections, capacity-minus-one, neighbor order, JPEG mask behavior, exhausted-mask sentinel and million-record override limit are unchanged. Shared selection scheduling is used by processing and record-inspection tests; no compatibility path or host readback is added.

Detection is ~0.4–4.4 ms, capped prefixes ~19–21 ms, scatter ~0.1 ms, and ordered correction ~200–203 ms. Two record-sized uint workspaces (~1.15 MB total) and three passes trade for ~560 ms saved across every original. Full all-enabled warm renders are now 1555/1572/1278/2082/1501/1894 ms (blue/leaves/rocks/flower/turquoise/yellow), versus baseline 3912/4114/3697/6244/4125/5729 ms. Full float buffers and TIFFs remain byte-identical. All five targeted camera GPU regressions (native/order/capacity/tie/dark defects), workspace and strict Clippy pass.

## Round 4: reuse cross medians instead of repeating neighborhood sorts

The edge kernel recomputed five cross medians for each of five neighboring gradients at every pixel. A single GPU pass computes identical median RGB once, with the same zero outer columns and initialized tile boundaries. The edge kernel reads those medians but keeps its original gradient/averaging/LUT expressions. The median buffer is subsequently fully overwritten as the existing horizontal-mean workspace; all-enabled storage allocation is unchanged. One pass/submission is added, with no readback or result cache.

Median plus edge work now takes ~13–24 ms on local/full photos, versus ~206–363 ms before (rocks global ~2.7 ms versus ~55 ms). All-enabled warm renders are 1280/1378/1218/1733/1246/1553 ms. Six float buffers and TIFFs remain byte-identical; full/reduced/global base, signed-detail, border, restored-pattern-seed and overlap component checks pass, as do workspace/strict Clippy. Edge-only inspection uses the needed median workspace but still skips directional means/color/detail.

## Round 5: simpler row-fitting reuse and quality-aligned residual layout

Parallelizing coefficients into a second dispatch dimension was output-exact but gave no reliable speedup (~100–117 ms), so it was rejected. Inverting the independent coefficient/pixel loops instead reuses one RGB normalization per pixel while preserving each coefficient's x/y FMA sequence. Fit time falls from ~100 to ~55 ms without more buffers/passes. Store the private residual workspace column-major, matching native quality traversal; update quality and row-mean indices consistently. Correlation work falls from ~213 to ~62 ms, while residual writes rise from ~27 to ~34 ms. Net row savings are ~190 ms, with the same compensated sums/thresholds/counts.

Warm all-enabled renders are 1118/1108/1036/1556/1041/1373 ms. Six full float buffers and TIFFs remain byte-identical. Native bank/row-count/gain/bounds regression, workspace and strict Clippy pass. This is a private storage/index change, not a new photographic algorithm or compatibility path.

## Round 6: cooperative donor RGB prefetch, still serial float sums

One 64-lane workgroup per bin loads the next RGB chunk into shared memory. Lane zero consumes that chunk in original donor order, retaining inline ratio arithmetic and the original recurrence. Uniform range loading makes barriers valid, including empty bins and partial chunks. This adds neither passes nor full-image buffers. Unlike rejected floating-ratio precomputation, RGB prefetch is byte-exact on all six originals.

Flower/yellow table work falls from ~421/211 to ~126/58 ms. Warm all-enabled renders are 1120/1108/1038/1252/1035/1215 ms; every full float buffer and TIFF still matches baseline byte-for-byte. All highlight connectivity/gain/ownership/selectivity/stable-bin tests, workspace and strict Clippy pass.

## Round 7: metadata-only dependency phases for camera pixel lists

Camera corrections read their own pixel and radius-one masked neighbors, then write only their own pixel. A metadata-only schedule assigns each eligible original main/JPEG record a level strictly after every earlier record at a neighboring center, including duplicates and read-before-write anti-dependencies. Centers within one level are independent; all runtime-detected records still execute serially afterward, then packed clusters. Detector mask overrides cannot expand the read footprint. The original record array/masks, correction arithmetic and immutable provenance remain unchanged.

The six originals select 41131 records in eight levels: 40625/470/15/8/5/6/1/1. Listed corrections fall from ~200 ms to ~0.26 ms, plus ~0.01–0.23 ms detected suffix. Eight extra passes/submissions and ~164.5 KB integer order storage trade for this gain; CPU preparation uses only metadata coordinates/classes. Warm completed all-enabled renders are 825/920/841/1042/838/1021 ms, with byte-identical float buffers and TIFFs. Structural schedule checks cover duplicates, neighbors, anti-dependencies, invalid borders/empty masks and all six originals at Camera/Manual 0/Manual 4. The full ignored suite (44 core plus GUI worker), ordinary workspace and strict Clippy pass.

## Round 8: stop copying local-mean pixels that are overwritten

The line kernel copied an entire input line before writing all its interior pixels again. Preserve copies only for untouched rows/columns and border margins; the backward recurrence still fully writes the interior, including endpoints. Reuse its RGB sample within each step. This keeps all tile/margin values and recurrence expressions unchanged, with no new allocation/pass.

Full-resolution two-direction mean work falls from ~194 to ~144 ms; local cases save ~25–35 ms; global rocks is already small. Warm renders are 852/888/848/995/804/969 ms (blue fluctuates despite improved kernel time). Six full floats/TIFFs remain byte-identical. Native full/reduced/global/overlap/border tests, workspace and strict Clippy pass.

## Round 9: share projection geometry without vectorizing interpolation

Repeating the 16-tile geometry/overlap traversal per RGB component cost ~40.5 ms across projections. Invert the tile/component loops with a three-element scalar result array: keep every channel's primitive scalar interpolation, reverse tile order and overwrite decisions. Geometry is evaluated once, scalar/RGB roots retain their layouts, and no storage/pass is added. Projection falls to ~14.8 ms. The attempted vector-float interpolation failed byte equality on blue despite passing native-tolerance components; it was rejected, not accepted as equivalent.

Warm completed renders are 866/856/816/981/777/940 ms; blue still fluctuates. Every full float and TIFF matches baseline. Full/reduced/global/signed/overlap fixtures, ordinary workspace and strict Clippy pass.

## Round 10: bilateral workgroup layout, unchanged per-pixel FMA

Tested 8×8, 16×8 and 32×8 groups on the M1 Pro, with matching host/test grids. 32×8 puts neighboring row pixels in the same SIMD group, with 256 threads within standard limits. Bilateral work falls from 83/92/32/41/49/82 to 64/72/25/34/39/66 ms. No arithmetic, buffer or pass changes; all six full floats/TIFFs remain exact.

Warm renders are 779/852/800/970/770/925 ms. Full ignored suite (44 core plus GUI worker), workspace and strict Clippy pass. This layout remains to be benchmarked on other GPU backends with the rest of cross-platform runtime validation.

## Round 11: exact integer prefixes, no photographic arithmetic changes

Camera detection prefixes now use a 256-record workgroup integer scan, then a capped block scan and exact local+block offsets in scatter. Counter capacity, accepted-bit order, duplicates and mask-exhaustion behavior are unchanged. ~562 uint offsets (~2.25 KB) and one extra pass replace the ~19.6 ms serial scan with ~0.2–0.35 ms. Despeckle's eight bank prefixes are independent; one workgroup per bank lowers ~8.2 to ~1.0 ms without extra buffers/passes.

Fifteen explicit prefix fixtures cover 1/255/256/257/777 records and caps 0/7/999, including cross-block saturation/partial tails. All 45 core ignored tests plus GUI worker, workspace and strict Clippy pass. Six goldens/TIFFs stay byte-identical. Five warm repetitions measure 723/787/780/947/747/900 ms; peak process RSS is ~1.10–1.22 GiB, including golden comparisons/export. Original decoder and concurrent GUI/help changes remain untouched.

## Round 12: stop at diminishing donor-chunk returns

Increasing cooperative donor chunks from 64 to 128 retains identical accumulation order and saves only ~7 ms on flower/~3 ms on yellow (~0.7%/~0.3% completed latency). Shared workgroup RGB storage increases from 1 to 2 KB; buffers/passes are unchanged. Full six-photo float/TIFF equality and all 45 core/GUI worker tests pass. This is the final small layout adjustment; further hardware micro-tuning is deferred.

## Completed performance pass and limits

Matched five-warm-render medians on Apple M1 Pro, all processing enabled, original baseline executable versus final engine. Queue completion is included; decoding/setup/export are separate. Neither route caches processing results.

| Original | Baseline ms | Final ms | Speedup |
| --- | ---: | ---: | ---: |
| Blue parking sign | 3844 | 724 | 5.31× |
| Glossy leaves and buds | 4025 | 785 | 5.13× |
| Rocks and tree shadows | 3703 | 779 | 4.75× |
| Sunlit red flower | 6225 | 938 | 6.64× |
| Turquoise still life | 3937 | 741 | 5.31× |
| Yellow flowers and stroller | 5616 | 896 | 6.26× |

Final first-process renders complete in ~732–943 ms. Decode is ~247–262 ms, cached-driver engine setup ~52–55 ms, and completed resized TIFF export ~11–17 ms. The original baseline repeat's first engine recompilation took ~956 ms; subsequent setup was ~43–54 ms. This is not a controlled empty-driver-cache benchmark. Submission alone remains ~55–84 ms and is not processing latency. Timestamped renders take ~849–1048 ms, with ~624–851 ms summed measured kernels; per-operation timing synchronization is additional overhead.

Every retained full float and resized TIFF matches the baseline byte-for-byte. Render activity reports zero CPU image readbacks; export reports one 7.68 MB readback. All-enabled counters: 109 allocations, 79 dispatches/81 submissions, ~264.5 MB uploads, ~275.2 MB device copies and ~5.57 GB **cumulative** storage allocations (not simultaneous residency). Compared process peak RSS ranges are similar: baseline ~1.08–1.23 GiB, final ~1.09–1.22 GiB. This pass primarily improves latency, not measured peak memory; process RSS is not a dedicated GPU residency measurement.

Every stage was reviewed. Remaining average/max GPU costs: local means ~92/145 ms; row fits/quality/residuals ~55/62/34 ms; bilateral ~50/72 ms; ordered despeckle ~56/211 ms; pattern mask ~43 ms and swaps up to ~71 ms; column bilateral ~38/40 ms; donor table ~30/119 ms; ordered masked clusters ~30 ms. Context/resampling/projections, calibration/black/gain, color/ColorDQ, detail/restoration, sharpening, tone/saturation and encoding are smaller. No stage is disabled or weakened to obtain these times.

Stop here: the last measured gain is below 1%; larger remaining reductions require more elaborate image/scratch layouts, cooperative recurrences or dynamic dependency scheduling, or changes to floating accumulation. Existing ordered recurrences remain intentional. Image readback elimination is not the remaining bottleneck. UI responsiveness, result caching, broader-camera/high-ISO/extreme-scene validation, other backends and owner photographic acceptance remain follow-ups.

Final validation passed: release build, ordinary workspace tests, 45 ignored core tests plus GUI worker, strict Clippy, documentation/format/diff and Python checks. Source/GUI help work concurrent with this pass is excluded from its commits.

## Output-changing alternatives for later review

No output-changing alternative is installed in the equivalent-output pass. Tested/rejected experiments:

- Floating donor-ratio caching alters contraction and fails flower equality. Accepted RGB-only prefetch avoids this.
- Cooperative neighbor RGB prefetch for serial despeckle reduces rocks correction ~203 to ~163 ms, but changes 20460 bytes of its float output. Native-tolerance tests pass, yet exact comparison fails. The implementation is reverted.
- Vector-float tile projection passes component tolerances but fails blue float equality. The accepted scalar loop inversion gets the geometry saving without this change.

Possible later choices include block-partial donor sums (different float32 association), simultaneous dependent defect updates (different adjacent repairs), or a simpler modern edge/noise model instead of the native recipe. These require photographic review and explicit acceptance; disabling corrections or lowering noise quality is not an optimization in this pass.
