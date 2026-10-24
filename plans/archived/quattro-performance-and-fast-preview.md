---
summary: "Measure Quattro latency and retain simple optimizations that preserve demonstrated photographic output."
status: archived
---

# Quattro performance

This completed pass follows the [Quattro integration](./quattro-integration.md) work. The entries below retain the sequence of measurements, rejected experiments, LOW investigation and accepted Fast integration. Earlier statements about prototype-only work describe those historical slices. Complete processing was measured separately from decoding and export, with immutable preparation owned by the engine and numerical behavior owned by the operations.

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
- [x] Keep final controls, model coverage and numerical claims aligned with the integration plan.

Broader benchmarking of every recorded-file default remains outside this completed slice. Timing claims below retain their explicit recipes and measurement boundaries. They do not establish performance for every capture setting.

## File opening and first exposure edit

The follow-up owner report separated slow initial display from exposure feedback. A release sd Quattro worker sequence with explicit Sunlight/Standard controls and sharpening 0.75 measured 1987 ms for initial loading/display, 563 ms for the first exposure edit and 175–181 ms for later exposure edits. These are worker timings including display preparation, rather than GPU-only measurements or recorded-default claims.

The initial session retained reconstruction inputs but not the photographic-response input. Prime both boundaries on the initial render using the functional Tone, SignedPhotographic and CodeResponse operations. This adds no camera-family switches and keeps the existing 1 GiB cache limit. The measured initial caches are 585 MiB for sd Quattro and 968 MiB for the Merrill fixture. First and subsequent exposure edits now dispatch only their response operation and match fresh renders exactly. The displayed original remains unchanged. Later switching between unrelated controls still replaces the active boundary and can require a warm-up.

A subsequent quiet worker sequence measured initial loading/display at 1688 ms, first exposure at 216 ms and later exposure edits at 201–208 ms. Runs were separate, so thermal and scheduling differences affect small comparisons. The eliminated reconstruction dispatches establish the cache benefit independently of wall-time variation. Finishing preparation and display work remain in the exposure latency. A separate synchronized diagnostic attributes approximately 149–150 ms to recipe preparation and 16–21 ms to the response GPU operation. It rebuilds numerical preparation even when intermediate images are reused. Removing that preparation cost requires a separate change to preparation lifetime.

Quattro's three compressed Huffman planes were decoded sequentially. Decode the three validated plane slices in bounded scoped workers, preserving plane order and all predictors. The initial sd Quattro release comparison reduced packed decode median from 617 to 463 ms and complete source opening from 683 to 529 ms. Broader runs encountered concurrent build/system activity, so these figures are directional. Every decoded sample matches an independent serial oracle across seven ordinary and inspection-only fixtures. The high-resolution blue plane contains four times as many samples as either other plane, limiting the gain from plane concurrency. Decoding still occurs only when opening a different source, independently of exposure edits.

Scope presentation now retains its last completed snapshot separately from the arriving image frame. Histogram, waveform and parade stay visible through file selection, rendering and scope replacement. Replies still require matching frame revision and region. The native viewport-scope sequence passes after moving the snapshot, including whole-image, viewport, pan, resize and all three plot views.

The worker slider benchmark accepts an optional `FOVEONIC_SLIDER_X3F` path for repeating this measurement on any supported source. Duplicate timing TIFFs were removed after retaining timing receipts, reclaiming approximately 449 MiB.

Follow-up validation passes: 439 ordinary workspace tests, strict all-target Clippy, formatting and generated-document checks. Independent cache GPU tests cover Quattro and Merrill, decoder comparison covers seven fixtures, and the native viewport-scope sequence passes.

## Second optimization round

The owner approved another measured optimization pass, including small photographic differences if they buy substantial latency reductions. This round first targets output-equivalent changes. The owner approved committing the validated output-equivalent optimizations before starting approximate algorithm experiments. Preserve original files and sidecars. Do not install.

The supplied DP0 original compresses 5,405,184 red samples, the same number of green samples, and 23,030,784 blue samples including special columns. Its total is 33,841,152 samples. Ordinary Merrill geometry has 16,084,992 samples per plane, or 48,254,976 total. With one decoder worker per plane, DP0's largest serial stream contains 43% more samples despite the smaller total. The sd Quattro original has 22,233,088 blue samples, 38% more than one Merrill plane. These counts explain the concurrency imbalance. They do not establish a measured Merrill decode-time ratio, because bit lengths and decoder work also matter.

Temporary preparation diagnostics isolated approximately 100 ms of the sd Quattro recipe in directional blue-repair preparation and another 29 ms in dark-offset preparation. Directional preparation selected 161,995 jobs. Most of its time scheduled dependencies through nine spatial hash lookups per job. Replace that hash table with a dense table of linked-list heads, retaining every prior record and the same maximum dependency level. Stable job ordering within each level remains unchanged. The table is temporary CPU preparation storage, requires no cache or invalidation rules, and is omitted when no jobs are selected. Dark-offset preparation has the largest head table, approximately 21 MiB for these sensor dimensions.

The luma cluster shader now prepares all six oriented target terms once per repaired point rather than repeating transforms and mask arithmetic for every candidate. It retains the original Float32 score accumulation, candidate order, tie rules and sequential point writes. Shared target storage increases from 100 to 1,200 bytes per workgroup.

Loading each candidate's 25 samples into a local array before its six orientations was rejected. It preserved complete outputs but increased the DP0 cluster diagnostic from approximately 170 to 227 ms and DP2 from 134 to 182 ms. Remove that array and retain direct candidate reads. No approximate processing change is selected.

- [x] Compare dense-grid job batches against an independent all-prior-record dependency oracle across all three repair stages, cell boundaries, duplicates and reversed input order.
- [x] Run ordinary workspace tests and strict all-target Clippy.
- [x] Complete quiet paired full-render measurements across five camera models, the ISO800 route and dedicated monochrome, with complete Float32 baseline comparisons.
- [x] Finish documentation checks and record final measurements.

### Quiet comparison

Freeze both release executables and alternate their execution order by case. Each runs one initial render, five warm complete renders and one separate synchronized GPU diagnostic, using Sunlight, Standard and otherwise recorded controls. Monochrome selects its dedicated recipe. Decode and TIFF export are outside the processing interval. First-render complete Float32 comparisons match in all eight cases, covering six color sources and two monochrome sources. The profiler performs this readback outside timing on the initial render, rather than validating every warm iteration. These are local Apple M1 Pro measurements, not application-startup or screen-feedback measurements.

| Source | Warm full render before → after (ms) | Reduction | Recipe preparation before → after (ms) |
| --- | --- | --- | --- |
| DP0 | 998 → 928 | 7.0% | 116 → 51 |
| DP1 | 891 → 853 | 4.3% | 102 → 49 |
| DP2 | 958 → 904 | 5.6% | 115 → 52 |
| DP3 | 963 → 881 | 8.4% | 137 → 61 |
| sd Quattro | 887 → 812 | 8.4% | 151 → 69 |
| DP3 ISO800 | 989 → 920 | 6.9% | 116 → 57 |
| DP0 monochrome | 966 → 901 | 6.8% | 116 → 52 |
| sd Quattro monochrome | 840 → 759 | 9.6% | 151 → 69 |

Separate synchronized cluster GPU totals fall from 165 to 152 ms for DP0 and 133 to 123 ms for DP2. The larger wall-time benefit comes from repair-job preparation. Cached edits rebuild this preparation too, but this round does not measure pointer-to-display latency. Dense indexing reduces preparation work without retaining additional per-engine resources. Remaining expensive stages include RGB range neighborhoods, full-resolution blue filtering and ordered candidate matching. Approximate denoising and splitting candidate scoring across workgroups remain separate experiments.

Final validation also passes the original-file cached photographic-edit regression on sd Quattro, the scheduling oracle, formatting, generated indexes and diff checks. No recipe controls, output defaults or decoder algorithms changed.

## Approximate algorithm experiments

After committing the output-equivalent changes, the owner approved testing reduced defect searches and lighter blue filtering. Keep processing sources at the committed behavior after freezing each experimental executable. Preserve originals and sidecars. Do not promote an experimental algorithm, commit or install it without review.

### Candidates

- Search every second or fourth physical row and column of the eligible defect-candidate catalogue, always including its first candidate as a fallback. Refine the best coarse location over its eligible 3×3 or 5×5 neighborhood. Retain all six orientations, patch-score arithmetic, original-candidate tie ranks and ordered defect writes. This can choose a different repair because the global minimum may lie outside the refined neighborhood.
- Pack coarse candidate indices during numerical preparation so active GPU lanes score consecutive eligible candidates. Find local-refinement eligibility through binary searches in the original coordinate-sorted catalogue. Full Float32 comparisons on DP0, DP2, DP3 ISO800 and DP0 monochrome match the slower coarse-search prototypes exactly for both search spacings. Packing changes execution cost, while the coarse search itself changes the algorithm.
- Reduce the blue filter from 11×11 to 9×9 or 7×7. Retain range weights, strength, adaptive blending and the rest of reconstruction. This reduces spatial support and can increase local noise or change fine texture. Color and dedicated monochrome both consume this shared reconstruction preparation.
- As an output-equivalent control, stop an orientation's score once its nonnegative partial sum cannot beat the lane's current best. Complete Float32 comparisons match the committed renderer on the four screening cases. Its measured benefit is smaller than the approximate search and it remains isolated with the other trials.

The first coarse-search shaders scanned the full candidate catalogue and skipped non-grid positions. The gentler search did not consistently improve the cluster stage. Packing candidates and checking the local neighborhood directly reduced DP0 cluster diagnostics from approximately 164 ms to 61 ms for the gentler search and 34 ms for the coarser search.

### Method and review

Freeze the committed renderer and every variant with executable hashes, input hashes and source snapshots. Use explicit Sunlight, Standard and sRGB TIFF output, with remaining controls following recorded defaults. Decode once per process and measure five warm complete renders separately from a synchronized GPU diagnostic. Alternate executable order by source. Compare complete RGB16 exports outside timing. Report component errors in equivalent 8-bit sRGB code units, counting a pixel above a threshold if any component exceeds it. These numerical thresholds are not perceptual acceptance criteria.

Lossless review images contain matched whole-image previews and three native 256×256 crops selected from the tiles with greatest squared error. Each crop row shows the committed result, candidate and absolute difference amplified eight times. Defect changes require this local review because a tiny whole-image mean can hide a large change at an isolated repair.

- [x] Screen both coarse searches and both smaller blue windows on DP0, DP2, DP3 ISO800 and DP0 monochrome.
- [x] Verify packed-search output against the corresponding coarse-search prototype.
- [x] Complete final matched timings and full-image comparisons across five cameras, ISO800 and two monochrome sources.
- [x] Summarize speed, numerical differences and visual limitations for owner review.
- [x] Restore tracked processing sources and finish documentation checks.

### Final comparison

The final matrix covers all five ordinary camera models, a separate DP3 ISO800 original and DP0/sd Quattro monochrome. Complete RGB16 comparisons exist for all 32 candidate/source pairs. On the four screening sources, packed searches match the corresponding coarse-search prototypes in complete Float32 comparisons. No photographic acceptance or new default is implied.

| Source | Committed (ms) | Gentler search (ms) | 9×9 blue (ms) | Gentler search + 9×9 (ms) | Coarser search + 7×7 (ms) |
| --- | --- | --- | --- | --- | --- |
| DP0 | 889 | 809 | 874 | 784 | 738 |
| DP1 | 816 | 766 | 789 | 744 | 709 |
| DP2 | 870 | 794 | 848 | 762 | 732 |
| DP3 | 856 | 806 | 842 | 787 | 744 |
| sd Quattro | 771 | 764 | 749 | 732 | 715 |
| DP3 ISO800 | 898 | 859 | 874 | 838 | 810 |
| DP0 monochrome | 856 | 769 | 834 | 745 | 701 |
| sd Quattro monochrome | 737 | 711 | 700 | 684 | 675 |

The gentler combined candidate reduces medians by 5.1–12.9%; the aggressive combination by 7.3–18.0%. Stage diagnostics independently support the speedup: DP0 defect matching falls from 152 to 61 ms with the gentler search and 34 ms with the coarser search. DP0 blue filtering falls from 82 to 59 ms at 9×9 and 41 ms at 7×7. For sd Quattro, defect matching falls only from 20 to 8 ms with the gentler search, leaving much less opportunity. These gains apply to full reconstruction, not cached exposure edits that skip these stages.

### Numerical and visual tradeoffs

| Candidate | Pixels above one 8-bit code across cases | Mean absolute component error (8-bit codes) | Largest component difference (8-bit codes) |
| --- | --- | --- | --- |
| Gentler search | 0.00051–0.00337% | 0.00004–0.00021 | 109.78 |
| 9×9 blue | 0.05043–2.17252% | 0.10228–0.21052 | 9.86 |
| Gentler search + 9×9 | 0.05114–2.17413% | 0.10230–0.21068 | 109.93 |
| Coarser search + 7×7 | 0.68908–15.40489% | 0.17788–0.38946 | 54.49 |

The gentler search changes very few pixels, but its largest observed sRGB difference is approximately 110 codes on sd Quattro. DP0 reaches approximately 69 codes. Native crops locate these in isolated repair patches. Low whole-image mean errors must not be described as uniformly tiny changes. Differences are not monotonic with search spacing, so the smaller largest error of a coarser search on these originals does not establish better quality. This is a poor unconditional trade on sources where defect matching is already inexpensive.

The 9×9 filter changes fine texture more broadly, with much smaller largest differences in the tested images. The 7×7 filter changes more noise and texture: in the ISO800 color case approximately 15.4% of pixels exceed one code in some component. Native crop review shows texture/noise changes without a broad color or geometric shift in the inspected crops, but the limited originals and recorded controls do not establish general perceptual equivalence.

The gentler combined candidate is useful for photographic review when greater speed is wanted; 9×9 filtering alone is the simpler conservative candidate. Reduced defect searches need explicit acceptance of occasional larger local differences, especially given sd Quattro’s small stage saving. No candidate has been selected for production. The exhaustive early-stop control remains another small output-equivalent opportunity, with cluster diagnostics of approximately 146/117/80/146 ms on DP0/DP2/DP3 ISO800/DP0 monochrome, compared with approximately 152/123/81/153 ms in the final committed baseline diagnostics. Runs differ, so this establishes only a modest opportunity, not a new full-pipeline speed claim.

Tracked processing files are byte-identical to the committed baseline after all trials. Documentation indexes, formatting and diff checks pass. Experimental executables, source snapshots, raw receipts and lossless review artifacts are retained separately. No algorithm or application default changed.

## Full embedded JPEG timing

A separate release harness calls the existing embedded-preview reader without thumbnail reduction, converts its complete RGB8 output to RGBA8, uploads an sRGB GPU texture and waits for upload completion. GPU initialization is outside the interval. Nine iterations per source provide medians over the last eight; local-drive filesystem caching is uncontrolled. This measures texture readiness, not native GUI request queuing, frame presentation or screen scanout.

| Source | Embedded JPEG dimensions | Read/decode (ms) | RGBA preparation (ms) | Completed upload (ms) | Total texture-ready (ms) |
| --- | --- | --- | --- | --- | --- |
| DP0 | 4816×3616 | 142 | 13 | 14 | 169 |
| DP1 | 5424×3616 | 166 | 15 | 15 | 195 |
| DP2 | 5424×3616 | 185 | 15 | 15 | 214 |
| DP3 | 5424×3616 | 189 | 15 | 15 | 219 |
| sd Quattro | 5424×3616 | 158 | 15 | 16 | 189 |
| DP3 ISO800 | 5424×3616 | 177 | 15 | 16 | 208 |

All six embedded JPEGs use sequential SOF0 encoding, not progressive quality scans. The current browser reads and decodes the full supported JPEG before reducing it to a 256-pixel thumbnail. Its preview API returns a completed image, so incremental rows or quality updates would require another decoding interface. Showing a complete camera JPEG before an editable low-resolution preview and final photographic image remains a possible responsiveness design, not an implemented or accepted mode. Camera JPEG appearance does not follow current application edits. The DP0 embedded JPEG is also narrower than the current 5424×3616 photographic output, so any progressive replacement must explicitly account for crop and geometry.

## Native LOW investigation

The reference implementation's ordinary Quattro LOW route is a separate input and processing contract, rather than a HIGH render with a smaller blue plane. Investigation used recovered ordinary callers and previously validated helper probes. All available photographic originals in this round are HIGH. No complete genuine LOW capture was decoded or rendered, and no Fast implementation or latency claim follows from this investigation.

### Contracts beyond blue dimensions

| Area | Native LOW behavior | Consequence for a reduced HIGH preview |
| --- | --- | --- |
| Input | Manager preprocessing (`0x58af8`) supplies interleaved float RGB, 12 bytes per cell, rather than six packed HIGH values, 24 bytes per cell. RGB and mono have equal dimensions. | Build an explicit same-resolution working representation. Changing a size flag cannot reinterpret the existing packed source. |
| Calibration | Preprocessing (`0x88fd8`) selects DP LOW response tables and sd LOW sensor-adjustment gains. | Preserve HIGH calibration for HIGH-origin samples until a measured reason justifies another transform. Native LOW calibration is not established as appropriate for averaged HIGH blue. |
| Repairs | The high-resolution luma repair sequence is skipped. Chroma repair, including bad pixels, despeckling and clusters, remains. LOW also changes some chroma repair gates and skips blue chroma FPPD correction. | Skipping all repairs would be an additional preview approximation. Native LOW does not justify dropping every repair. |
| Highlight preparation | LOW still performs highlight restoration and then copies restored RGB blue into mono. Some calibration/highlight helpers receive resolution-specific controls. | Derive the later blue guide from the intended processed RGB boundary. Do not retain an unrelated pre-restoration blue plane. |
| Noise parameters | Setup selects HI/MED/LOW tables and sensor/WB-dependent controls. Ordinary Stage1 also overwrites some supplied noise coefficients. | Audit controls actually consumed by the chosen Fast stages. Blindly copying a LOW descriptor or assuming all its entries affect output is insufficient. |
| Blue filtering | Stage2B (`0xe3910`) skips HIGH blue outlier removal but retains blue filtering and adaptive blending. Its ordinary LOW path uses a direct bilateral filter where exact HI selects the histogram filter. | A cheap same-size route can still denoise blue. Native LOW is not simply an unfiltered RGB copy. |
| Reconstruction | LOW forces the same-resolution branch: `CreateLumaResidual` plus float RGB-to-RGB16 packing. It bypasses `CreateBFLumaResidual`, the two-resolution color reconstruction. | This is the strongest structural precedent for averaging blue to the existing R/G grid and avoiding full-resolution fusion. |
| Detail controls | LOW has different Stage2B strengths and detail formulas. Finishing multiplies sharpening by about 0.7071, changes spatial local-contrast radii from 7/3 to 3/2, and changes the wavelet local-contrast base from 0.15 to about 0.12. | Filter and sharpening scales must follow working resolution. Running unchanged HIGH finishing on a smaller image changes apparent spatial extent. |
| Geometry | Manager preprocessing halves HIGH active-area coordinates but uses LOW coordinates directly. Crop bounds, calibration maps, repair coordinates, masks and final output sizing remain part of the contract. | Track physical source geometry and working geometry separately. Replacing only width and height in metadata is unsafe. |

Existing HIGH-file metadata already demonstrates that calibration selection matters. DP0 contains distinct `LUT_HI_RR_x1` and `LUT_LOW_RR_x1` tables with dimensions 3×13759 and 3×13757. The sd Quattro HIGH sensor-gain triple is approximately `(0.59767, 0.56410, 0.58696)`, while its LOW triple is `(0.59774, 0.56374, 0.57590)`. Selecting LOW gains for averaged HIGH samples could therefore shift blue by roughly 1.9% before later processing. This is not a measured final-image color error.

Native LOW retains a meaningful detail residual. In the ordinary nongamma helper, the initial ratio is `(max(filteredBlue,0)+5)/(stage1Blue+5)` when Stage1 blue is positive, otherwise one. A highlight transition and clamping follow. Copying Stage2 RGB alone would omit this detail/noise relationship. Low and high Stage2B setters also use different logarithm bases in a detail formula, natural log versus log2. These differences are supported by isolated native setter and residual probes, not a full LOW photographic comparison.

### Recommended first experiment

Keep the existing decoded HIGH source and its calibration identity. Form a same-resolution RGB working image on the native R/G grid using an averaged, appropriately calibrated blue plane. Reuse applicable color processing, replace two-resolution reconstruction with an explicit same-resolution path, and adjust finishing to working resolution. Preserve Full mode as the comparison. Any omitted defect correction should be named as a separate Fast approximation and checked on the known repair patches. The owner subsequently approved the isolated rendering experiment below. Application integration remains separate.

That grid has half the full blue width and height, or one quarter of its pixels. It preserves all native R/G samples. Further reductions for Fit can follow a separate resampling step. A half-pixel-count mode requires a different output grid and resampling policy, rather than the native LOW dimensions alone.

The reference implementation also has a separate QuickDis preview route. Its Stage1 avoids the ordinary bilateral filter, and its Stage2B packs RGB directly. Its own parameter preparation and incomplete whole-request provenance make it another possible experimental recipe, rather than proof that ordinary LOW can omit the same stages.

### Evidence limits and follow-up

Recovered callers establish the ordinary branch structure. Existing independent probes cover LOW noise descriptors, Stage2B setters, residual arithmetic, repair flags and selected finishing controls. They do not establish complete metadata-to-output parity for a genuine LOW capture. The LOW float column-calibration path, complete LOW highlight calibration, hardware sampling/normalization, exact active crop and composed output remain follow-up checks. A reduced HIGH preview can be evaluated against our Full output without claiming native LOW emulation. The Stage2 highlight-neutralization flag forwarded from request offset `0x568` has distinct thresholds, but its relationship to LOW was not established here and must not be inferred from its name.

No production processing files changed during this investigation. Background decoding, disk caching, sidebar preview persistence and viewport refinement remain separate responsiveness work.

## Same-resolution Fast experiment

The owner approved an isolated prototype and requested an interactive Full/Fast report to judge whether the different appearance is attractive. Preserve that review as an aesthetic decision rather than treating numerical disagreement as rejection. Production processing sources were frozen before editing and restored after the experiment. The measured executable, exact source patch, input identity, timings and photographic outputs are retained separately. No application default or export behavior changed.

The prototype keeps HIGH decoding, calibration tables, sparse corrections, RGB repairs, highlight restoration and ordinary Stage1/Stage2 color processing. It averages the calibrated high blue plane 2×2, filters the resulting mono plane, packs Stage2 RGB directly and derives a same-resolution residual against Stage1 blue. The existing residual-finishing operation applies that gain and generates signed detail. Finishing runs on the native R/G grid, with reduced sharpening, adjusted local-contrast controls and a physical-coordinate crop mapping that preserves fractional cell centers and odd active-area endpoints. Portrait orientation remains intact.

This is a LOW-inspired Fast recipe, not native LOW emulation. Its blue filter remains the existing histogram operation, with 7×7 support and variance coefficients divided by four as an experimental averaging model. It retains HIGH adaptive blend controls and does not establish independent sensor-noise behavior. It also derives blue from the calibrated HIGH history rather than claiming the native LOW restored-RGB copy contract.

Two repair variants isolate the tradeoff. The retained-repair variant keeps every existing HIGH blue repair. The faster variant omits the ordered high-resolution luma repairs, blue despeckling, luma clusters, sd luma FPPD/pattern correction and late high-blue mismatch. RGB repairs remain. The report compares Full against the faster variant. Full produces 5424×3616 and Fast 2712×1808, with dimensions swapped for portrait images.

### Native-output timing

Use frozen release executables sequentially, alternate order by source, decode once per process and take five warm fresh completed-render samples. Explicit controls are Sunlight WB, Standard mode and sRGB output, with other controls from the file. GPU synchronization is included in completed rendering. Source decoding, export and display presentation are excluded. A separate diagnostic iteration records GPU stages. These local Apple M1 Pro measurements do not represent GUI interaction latency or a cached edit session.

| Source | Full (ms) | Fast (ms) | Render speed |
| --- | --- | --- | --- |
| DP0 Quattro | 910 | 427 | 2.13× |
| DP1 Quattro | 812 | 427 | 1.90× |
| DP2 Quattro | 884 | 430 | 2.06× |
| DP3 Quattro | 857 | 435 | 1.97× |
| sd Quattro | 783 | 349 | 2.25× |
| DP3 Quattro · ISO800 | 901 | 495 | 1.82× |

An earlier matched 2712×1808 output screening measured the retained-repair variant at about 596–680 ms for DP cameras, compared with 792–883 ms for Full. The corresponding faster variant took about 426–492 ms. The sd screening had scheduling variation, so the subsequent quiet native-output measurements above are preferable. Keeping blue repairs therefore removes some, but not all, of the smaller-grid speed benefit.

### Photographic review and checks

The interactive report contains all five supported models and the ISO800 source. Full/Fast toggling preserves region and zoom. Fit, Full 100%, Fast 1:1, double-click zoom, panning and a hold-to-compare control support composition and detail review. Full images retain native output resolution. Fast images enlarge to the same photographic area. Both are lossless sRGB PNGs quantized from RGB16 exports to 8-bit display values. Browser resampling influences scaled detail. Optional local notes can be downloaded for owner feedback.

Matched-output screening confirms visible texture, edge and sharpening differences, with scene-dependent magnitudes. Those measurements do not establish preference or photographic acceptance. An independent bounded GPU check passes for four-phase averaging, negative/zero blue guides, highlight-transition residuals and RGB truncation/clipping. A local WebKit browser check passes for image loading, Full/Fast toggling without moving the viewport, zoom and portrait aspect ratio. All six fresh Full TIFFs are byte-identical to the previously frozen committed baseline exports. Documentation indexes, formatting and diff checks pass after restoring processing sources.

- [x] Implement and freeze an isolated same-resolution prototype.
- [x] Separate the reconstruction benefit from omitted high-blue repairs.
- [x] Measure native-output rendering across six color originals.
- [x] Prepare interactive Full/Fast photographic review.
- [x] Obtain owner preference on appearance and usefulness.
- [x] Select application integration and supported controls in the specification below.

There is no GUI Fast toggle, background decode cache, disk LRU, progressive viewport refinement or viewport-only full-quality render in this slice. The trial explicitly supports color final output only. Monochrome, inspection and DNG remain on their existing paths.

## Application Fast preview integration

The owner accepted both photographic appearances and approved the Full/Fast application toggle. The [Fast preview specification](../../docs/specs/quattro-fast-preview.md) records this slice. Full remains the initial workspace default. The preference persists separately from editing sidecars. Fast supports the demonstrated ordinary Quattro color final recipe, while inspection, comparisons, monochrome, other families and exports select Full. Background decoding, disk caching and progressive viewport refinement remain separate.

- [x] Replace experimental environment switches with explicit per-request preview quality and recipe-owned working geometry.
- [x] Integrate the reviewed averaged-blue same-resolution recipe and retain the original Full route.
- [x] Add persistent Full/Fast viewport controls and displayed-quality provenance.
- [x] Preserve native zoom/pan coordinates and map viewport scopes onto actual rendered pixels.
- [x] Validate original-file Full parity, fresh/cached Fast editing and quality switches on all six sources.
- [x] Verify Full TIFF export under a Fast preference and return to Fast preview.
- [x] Complete native application smoke checks and record remaining limitations.

The reduced numerical preparation is selected explicitly and encoded into operation keys. Full calibration metadata remains immutable. Half-grid cropping preserves physical endpoints and pixel-center offsets, including rotated origins. Correcting the interpolation-kernel axes for portrait orientation is the only intended numerical refinement from the isolated prototype.

Original-file regression checks cover all six color sources, including both portrait files and ISO800. Complete Full Float32 images match the committed golden buffers exactly. Fresh and cached images match exactly through Full/Fast switches, exposure changes, noise/detail changes and return to baseline. An earlier Fast image remains immutable through subsequent execution. The bounded averaging/residual GPU fixture also passes.

The GUI worker keeps preview policy out of returned photo settings and derives effective quality from output, comparison, rendering mode and source support. TIFF export is byte-identical to a separate Full render with the requested exposure edit, despite a Fast preference and an inspection root. The next preview is reduced again. Unit checks cover preference persistence, sidecar exclusion, toolbar selection, unsupported-mode policy, physical crop mapping and native-coordinate viewport scopes.

Native application checks pass on DP0 landscape and DP3 portrait sources. Each isolated run captures Full, Fast, zoomed Fast, an exposure edit, Full again and Fast again. Quality switches preserve the exact visible native rectangle. Viewport scopes use the corresponding half-resolution rectangle in Fast and native rectangle in Full. Navigator preparation completes for each displayed revision. The local app bundle is built without installation.

The six integrated Fast RGB16 exports match the reviewed prototype exactly, including both portrait sources. Full Float32 baseline checks and Fast fresh/cached equality remain separate checks. Ordinary workspace tests and strict all-target Clippy pass, along with documentation/index, formatting and diff checks. The bounded GPU and original-file checks are opt-in tests.

In one DP0 native run, switching from completed Full to Fast took about 351 ms of worker processing, a cached Fast exposure edit 117 ms, and returning to Fast after Full 333 ms. The first Full frame took about 1577 ms including source loading and setup. These are individual worker samples, not pointer-to-screen latency medians or matched fresh-render speed ratios. Zoom and pan use the completed GPU image and do not start another render.

The first application slice is complete. Fast remains a reduced preview at every zoom until the owner selects Full. Automatic refinement, viewport-only Full rendering, background decoding, disk caching and sidebar-preview replacement are not implemented here. They remain possible subsequent responsiveness slices.

The owner moved Preview into the viewport header to the left of Inspect. Each set uses its own padded button-group background. Both rows reserve the full control height before placing labels, keeping Preview vertically centered with Full/Fast rather than top aligned. The Open/Export toolbar retains its existing groups.

The viewport-control placement passes all 154 ordinary GUI tests and strict GUI Clippy. Native capture confirms Preview and Inspect sit in separate aligned groups, with Preview vertically centered beside Full/Fast. The native quality-switch, zoom, edit and scope checks pass after the layout change. The local app bundle has been rebuilt.

Owner review found excessive top padding in the viewport groups. Remove the general control-band leading gap from these inline rows and reserve only the padded control height plus each group's four-pixel margins. Preview and Inspect retain vertical centering without the extra band space.

## Closeout and deferred work

The final review keeps physical geometry distinct from the working grid, gives reduced-preview constants numerical names and provenance, and keeps same-size reconstruction and crop mapping in camera-independent nodes. The [architecture review](./quattro-review.md) records the cleanup and final validation.

The loading area now uses discrete stage completion and total request-to-image milliseconds. Weighted progress, versioned per-node profiling receipts and an interactive timing DAG were discussed and explicitly deferred. The current equal-section bar remains in place. Background decoding, bounded temporary disk caches, edited sidebar previews and automatic viewport refinement remain possible future work. They are not part of the completed preview implementation.
