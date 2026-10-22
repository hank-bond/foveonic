---
summary: "Measure Quattro latency and retain simple optimizations that preserve demonstrated photographic output."
---

# Quattro performance

This pass follows the [Quattro integration](./quattro-integration.md) work. Measure complete processing separately from decoding and export. Preserve the demonstrated photographic output. Keep immutable preparation caches with the engine and numerical behavior with the operations.

## Method

The baseline covers DP0, DP1, DP2, DP3, sd Quattro and a separate DP3 ISO 800 file on an Apple M1 Pro. Explicit settings were Standard, Sunlight, chroma/luminance/banding step 2, Smooth/Crisp 0.5 and sharpness 0. Decode once, then measure three warm full renders. Record GPU timestamps in a separate synchronized run. Freeze the baseline executable and complete Float32 outputs before shader edits.

These explicit settings isolate performance changes from concurrent work on recorded file defaults. Final application-default measurements remain a separate check. Scheduling, thermal conditions and shared memory traffic affect wall times, so stage measurements and complete wall measurements serve different purposes.

## Accepted shader changes

| Change | GPU stage before → after | Benefit and cost |
| --- | --- | --- |
| Share crop coordinates and weights across RGB | 43–54 → 17–22 ms | Saves 26–32 ms. A small loop reordering retains each channel's 16-tap accumulation order. |
| Cache scalar histogram values and bins per workgroup | 94–119 → 74–86 ms | Saves 17–34 ms. Adds 2.6 KiB shared storage for the common 11-wide neighborhood. Larger accepted windows retain the storage-read path. |
| Share high-ISO guide coordinates and compare RGB vectors | 135 → 37 ms | Saves 98 ms on the ISO 800 case. Uses the existing sample order and per-channel arithmetic. |

All six complete Float32 outputs remained byte-identical after all three changes. Independent native fixtures passed for 320 crop/resize/rotation cases, eight scalar histogram cases and the guidance/recursive-means/residual-blend cases.

The final coordinated quiet run used the frozen executable containing the three accepted shader changes, without the rejected cache. Every complete Float32 output matched the original golden again.

| File | First render before → after (ms) | Warm median before → after (ms) | Warm reduction |
| --- | --- | --- | --- |
| DP0 | 1306 → 949 | 1009 → 934 | 7.4% |
| DP1 | 936 → 865 | 920 → 847 | 8.0% |
| DP2 | 994 → 920 | 991 → 909 | 8.3% |
| DP3 | 993 → 918 | 976 → 938 | 3.9% |
| sd Quattro | 904 → 843 | 893 → 822 | 7.9% |
| DP3 ISO 800 | 1187 → 940 | 1138 → 927 | 18.5% |

The first render uses a fresh engine after decoding. Warm results reuse that engine and source but recompute the complete image. These first-render figures are not disk-cold application startup measurements. Separate synchronized GPU totals improved by approximately 58–74 ms for ordinary files and 160 ms for the ISO 800 file. The ISO 800 guide stage fell from 135 to 37 ms.

Crop-only DP0 wall measurements were inconsistent despite a repeatable GPU stage improvement. The final combined measurements above use a coordinated quiet window and show the benefit across all six files. Each before/after comparison still uses separate runs, so small wall-time differences remain subject to scheduling and thermal variation.

Each change stays inside its existing shader. Reverting one shader restores that operation without changing recipes, settings or image contracts.

## Rejected experiments

- Caching only Float32 scalar values with a maximum 31-wide halo measured approximately 103 ms on a full blue plane, slower than the original approximately 91 ms microbenchmark. Retaining the quantized bin beside each value and limiting the cached halo produced approximately 75.5 ms instead.
- Padding histogram rows to a multiple of four measured 75.9 ms versus 75.5 ms without padding. Keep the simpler tightly packed layout.

## Rejected response cache

A four-entry engine-owned cache keyed exact numerical profile, exposure, effective tone controls and monochrome behavior. It preserved all six complete Float32 outputs and passed reuse, eviction and key-invalidation checks.

Direct release measurements showed response preparation costs approximately 7.45 ms, while a cache hit costs 0.0003 ms. The broader 75–90 ms recipe-preparation time includes other work. A cache therefore saves less than one percent of complete processing while adding a mutex, eviction logic, key maintenance and API plumbing. The owner rejected this tradeoff for this pass. Cache code and tests were removed. Retain the measurement to avoid attributing all recipe preparation to response generation again.

## Monochrome Stage1

Measure the demonstrated 2944×1888 Stage1 boundary separately while later monochrome reconstruction is still being completed. Five synchronized GPU diagnostic iterations give the following medians after discarding the first:

| Operation | Before (ms) | Interior replay optimization (ms) |
| --- | --- | --- |
| Blue extraction | 0.59 | 0.59 |
| Blue speckles | 1.63 | 1.63 |
| Channel replacement | 0.97 | 0.98 |
| Chromatic-ratio suppression | 7.28 | 7.28 |
| Neighborhood statistics | 9.82 | 9.81 |
| Ordered neighborhood replay | 31.61 | 13.59 |
| Bare 15×15 RGB bilateral | 75.92 | 75.91 |

The replay kernel previously considered twelve centers and scanned nine neighbor positions for each interior destination. Exactly nine centers contribute one update each. Visiting those nine directly retains their update order and loads the immutable destination sample once. The original boundary mapping remains separate. This saves 18 ms in the measured stage. All eighteen bounded native fixtures pass and the complete Stage1 Float32 output is byte-identical to its frozen pre-change output.

The chromatic-ratio stage already shares neighborhood coordinates and is too small to prioritize further. The bare bilateral visits approximately 1.25 billion RGB neighbors per image. A shared tile might reduce repeated storage loads, but adds workgroup storage and risks slowing the ordinary color path that shares this shader. No tiling change has been implemented. Full monochrome photographic latency remains to be measured after downstream correctness work settles.

The ignored `original_channel_filter_matches_native_stage1` test accepts `QUATTRO_MONO_STAGE1_PROFILE` as a JSON output path for these five diagnostic iterations. Its optional `QUATTRO_MONO_STAGE1_OUTPUT` path captures the complete GPU float result for exact comparisons. These diagnostic timings are separate from normal complete-render wall measurements.

## Complete default monochrome processing

After the recorded monochrome mixer initialization and weighted native quality checks passed, freeze a fresh release profiler and measure all six files again. Select only monochrome rendering. Follow each file's recorded mixing and remaining automatic controls. Each receipt includes executable and source-manifest hashes, input-file hashes, requested settings and separate processing/export traces.

| File | First render (ms) | Warm median (ms) | Separate synchronized GPU total (ms) |
| --- | --- | --- | --- |
| DP0 | 1081 | 906 | 679 |
| DP1 | 848 | 822 | 603 |
| DP2 | 889 | 879 | 656 |
| DP3 | 891 | 865 | 622 |
| sd Quattro | 788 | 767 | 537 |
| DP3 ISO 800 | 861 | 815 | 633 |

Each warm median uses three complete fresh renders with the engine and decoded source reused. Decode and TIFF export are outside these processing intervals. These are current monochrome latency measurements, without comparisons to earlier incomplete monochrome output.

All six files record mixer weights `[10813,10813,11142]/32768`, represented exactly as `[0.329986572265625,0.329986572265625,0.34002685546875]` in the logged requests. They use monochrome luminance and banding step 2, zero exposure and neutral contrast/shadows/highlights. Smooth/Crisp and sharpening remain automatic so the recipe can follow recorded monochrome values, with fallbacks of 0.5 and zero respectively. Inactive WB and color-mode requests remain empty.

The separate Stage1 diagnostic and complete-render traces agree: chromatic-ratio suppression takes approximately 7 ms, statistics 9–10 ms, ordered neighborhood replay 13–14 ms and the bare bilateral approximately 74–83 ms. The high-resolution scalar histogram takes approximately 74–83 ms. Factory ordered defect clusters vary substantially by file, reaching approximately 164 ms for DP0.

### Superseded neutral mixing

An earlier run used equal one-third mixer weights before source initialization read the recorded mixer fields. Its warm medians were 982/846/878/895/767/814 ms in the same file order. Those are historical neutral-mix measurements, not file-default latency claims. Retain their timing/settings receipts separately. The six superseded TIFFs were deleted, reclaiming approximately 706 MB. The current file-mixed TIFFs remain available for review. The independent NDCR before/after Float32 proof remains valid and retained.

## Remaining opportunities

Ordered defect-cluster scoring uses one workgroup per ordered cluster. Splitting candidate scoring across workgroups would need another reduction while preserving cluster order. This is more complex than the accepted shader edits and has not been implemented.

The small highlight-audit readback waits for earlier GPU work. Its host wait must not be reported as the CPU cost of highlight repair. Deferring the audit could improve overlap, but removing that wait does not remove the queued GPU work.

- [x] Repeat complete quiet-system measurements with the explicit frozen baseline controls.
- [ ] Measure final recorded-file defaults after correctness work settles.
- [ ] Keep final controls, model coverage and numerical claims aligned with the integration plan.

## File opening and first exposure edit

The follow-up owner report separated slow initial display from exposure feedback. A release sd Quattro worker sequence with explicit Sunlight/Standard controls and sharpening 0.75 measured 1987 ms for initial loading/display, 563 ms for the first exposure edit and 175–181 ms for later exposure edits. These are worker timings including display preparation, rather than GPU-only measurements or recorded-default claims.

The initial session retained reconstruction inputs but not the photographic-response input. Prime both boundaries on the initial render using the functional Tone, SignedPhotographic and CodeResponse operations. This adds no camera-family switches and keeps the existing 1 GiB cache limit. The measured initial caches are 585 MiB for sd Quattro and 968 MiB for the Merrill fixture. First and subsequent exposure edits now dispatch only their response operation and match fresh renders exactly. The displayed original remains unchanged. Later switching between unrelated controls still replaces the active boundary and can require a warm-up.

A subsequent quiet worker sequence measured initial loading/display at 1688 ms, first exposure at 216 ms and later exposure edits at 201–208 ms. Runs were separate, so thermal and scheduling differences affect small comparisons. The eliminated reconstruction dispatches establish the cache benefit independently of wall-time variation. Finishing preparation and display work remain in the exposure latency. A separate synchronized diagnostic attributes approximately 149–150 ms to recipe preparation and 16–21 ms to the response GPU operation. It rebuilds numerical preparation even when intermediate images are reused. Removing that preparation cost requires a separate change to preparation lifetime.

Quattro's three compressed Huffman planes were decoded sequentially. Decode the three validated plane slices in bounded scoped workers, preserving plane order and all predictors. The initial sd Quattro release comparison reduced packed decode median from 617 to 463 ms and complete source opening from 683 to 529 ms. Broader runs encountered concurrent build/system activity, so these figures are directional. Every decoded sample matches an independent serial oracle across seven ordinary and inspection-only fixtures. The high-resolution blue plane contains four times as many samples as either other plane, limiting the gain from plane concurrency. Decoding still occurs only when opening a different source, independently of exposure edits.

Scope presentation now retains its last completed snapshot separately from the arriving image frame. Histogram, waveform and parade stay visible through file selection, rendering and scope replacement. Replies still require matching frame revision and region. The native viewport-scope sequence passes after moving the snapshot, including whole-image, viewport, pan, resize and all three plot views.

The worker slider benchmark accepts an optional `FOVEONIC_SLIDER_X3F` path for repeating this measurement on any supported source. Duplicate timing TIFFs were removed after retaining timing receipts, reclaiming approximately 449 MiB.

Follow-up validation passes: 439 ordinary workspace tests, strict all-target Clippy, formatting and generated-document checks. Independent cache GPU tests cover Quattro and Merrill, decoder comparison covers seven fixtures, and the native viewport-scope sequence passes.
