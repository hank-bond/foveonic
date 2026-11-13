---
summary: "Complete native visible-region rendering for adaptive tone, monochrome and authored rotation, with whole-image comparisons and measured app latency."
---

# Regional preview completion

The owner approved evaluating incomplete visible-region support, adding scenarios, completing that support and measuring zoomed multi-edit performance. Follow [viewport workflows](../docs/specs/viewport-workflows.md), [Smart preview](../docs/specs/smart-preview.md) and the [diagnostic measurement contract](../docs/specs/performance-diagnostics.md). Wide execution remains a separate experiment.

## Scope and findings

Smart currently requests a native region followed by an unconditional Whole continuation. Some recipes instead fall back to Whole before starting. Existing traces distinguish this extra work from checkpoint eviction and the worker's single native-session capacity. The owner clarified that Whole continuation remains useful background preparation for panning. It must run after all user-requested visible renders and yield to new foreground work without being falsely settled or lost.

Adaptive Tone already estimates from a whole-photo reduced branch (Thumb for Natural, the next finer grid for Strong, capped by available input). Regional application needs its own aligned spatial support, including brightness-guide construction, structure probes and gain interpolation. Whole analysis must remain independent of the visible rectangle and reusable across panning.

Monochrome uses separate camera algorithms. Quattro/Tiger need regional spatial controls and sharpening extents. Merrill's column correction and recurrence-based histogram require whole-photo context for exact output before later scalar stages can run regionally. Classic currently has no regional adapter in either color or monochrome and remains an explicitly measured gap rather than being advertised as supported.

Authored rotation requires distinct composed-demand and camera-processing rectangles. Map through the existing whole-image sampling transform, include interpolation and filter support, and preserve the requested composed tile for display and scopes. Whole-image arithmetic and coordinate conventions remain authoritative.

## Slices

- [x] Capture the current unsupported/fallback scenarios and baseline timings before production changes.
- [x] Prioritize all requested visible renders over speculative Whole continuations, including operation-boundary yield and resumption.
- [x] Enable Adaptive Tone regional application with reusable whole-photo analysis and sufficient aligned support.
- [x] Add monochrome regional camera adapters while preserving global prerequisites.
- [x] Add authored composition mapping for cropped and rotated regions, including quarter turns and fine angles.
- [x] Review remaining camera, recipe and output restrictions and report unsupported cases explicitly.
- [x] Run native mixed-edit scenarios, normal and low budgets, and compare warm latency and work counts.
- [x] Run frozen Whole rendering comparisons and obtain independent code/design reviews for each slice.

## Scenarios and evidence

For supported Merrill, Quattro and Tiger inputs, cover color and monochrome, Reference and Universal tone, Natural and Strong, center and edge regions, asymmetric saved crop, camera quarter turns, authored quarter turns and positive/negative fine angles. Compare actual regional coverage and geometry as well as pixels against the corresponding Whole output. Fine rotation comparisons must sample the composed output rather than compare raw storage alone. Include tuned monochrome processing, spatial edits, region movement, repeated exposure changes, source reuse and retained analysis. Classic baseline cases must clearly identify fallback until an adapter is implemented.

App scenarios cover four displayed editing targets with one zoomed, two independently zoomed, and the independent loupe. Fit siblings keep their own reduced demand. Assert requested-region completion before background Whole work, current final edits, bounded settlement and unchanged selection. A new foreground request must interrupt future continuation operations, while valid continuation remains pending for later completion. Preserve explicit Full/Fast/Draft/Thumb resolution semantics.

Measure natural Off runs separately from traced attribution. Distinguish cold source preparation, warm native checkpoints, visible-result delivery and complete-group settlement. Record region dimensions, recipes, cache budget, source reads, evictions and actual processing work. Check competing app/compiler activity before timing. Retain reproducible fixture identities and report variance rather than promoting a single diagnostic sample to a benchmark.

## Engine slice checks and measurements

The existing regional support boundary now accepts Adaptive Tone and monochrome for native Merrill, ordinary Quattro and Tiger color geometry. Authored quarter turns and fine rotation map composed demand to camera processing bounds, then use the same composed sampling path as Whole. Quarter-turn-only delivery retains integer sampling. Classic and reduced-RAW Quattro still lack the existing camera regional adapter. Intermediate inspection other than `linear.base`, reduced working-scale regions, and incompatible output sizing continue to report Whole coverage.

All 40 frozen Whole Full/Fast/Draft/Thumb outputs matched exactly across three Classic, three Merrill, three Quattro and one Tiger input. Regional comparisons independently assert tile coverage and storage geometry. Adaptive center/edge and repeated-exposure checks cover the sign, hazy coast, speaker, DP0 Quattro, sd Quattro and Quattro H. Monochrome checks cover native orientation, tuned texture/noise and complete continuation. Fine-angle output comparisons use actual sampled GPU textures, with a 0.0005 normalized-RGB tolerance for translated Float32 coordinates. Synthetic high-frequency odd-sized quarter-turn tiles require exact samples and matching viewport histograms at all camera/authored orientations. Whole arithmetic is unchanged.

Matched warm headless exposure checks alternate the same two values, discard two edits, and retain prior display artifacts. Each cell below summarizes three trials of five edits, using Off recording and completed GPU texture preparation. Source access and initial preparation are excluded. The regional tile is 992 by 1002 pixels. These are engine timings, not app or physical screen latency.

| Source and processing | Whole median per trial, ms | Regional median per trial, ms |
| --- | --- | --- |
| Hazy coast, Natural | 28.16–28.41 | 10.60–10.77 |
| Hazy coast, monochrome | 26.69–28.79 | 3.33–3.50 |
| sd Quattro, Natural | 53.64–57.46 | 32.22–34.64 |
| sd Quattro, monochrome | 45.73–49.42 | 14.24–14.54 |

Repeated settings can reuse prior fitted responses. A separate seven-edit incremental regional sequence measured Natural at 30.5–33.2 ms on hazy coast and 46.5–51.4 ms on sd Quattro. Do not apply the two-value warm figures to arbitrary new slider values. The fitting overview remains small; fitting and application still have costs. No per-node algorithm optimization was attempted.

Rebuild fixture declarations with `python3 scripts/profiling/prepare_inputs.py --offline`. Run GPU cases serially. `FOVEONIC_FINISH_SOURCE` selects an original from the declared inputs for `gpu_adaptive_regions_use_whole_analysis_and_match_whole_pixels` and `gpu_rotated_regions_match_sampled_whole_view`. Use `FOVEONIC_FIXTURE_DIR` for `monochrome_regional_views_match_whole_frame`, `quattro_monochrome_regional_matches_full` and `tiger_monochrome_regional_matches_full`. `regional_rotation_samples_match_the_whole_composed_view` uses generated data. All are opt-in release library tests with `--ignored --test-threads=1`.

`profile_smart_preview ORIGINAL REPORT --warm-edits --region-only --repeats 5 -- RENDER_OPTIONS` measures useful region-only work without pretending that the application currently omits continuation. The report records actual regional coverage and rejects Whole fallback. Use `--full-edit exposure --benchmark-off --repeats 5` for matched Whole edits. `--settings-snapshot FILE` reuses the shared diagnostic snapshot format for authored composition and transient controls. Keep the ordinary existing warm region-plus-Whole route for measuring the complete handoff.

The separate [bounds inventory](./regional-bounds.md) recommends a later consolidation of storage extent, active bounds and output view. This slice fixes their concrete regional uses without adopting that broader refactor.

## Continuation ordering and native acceptance

Automatic Whole tasks now carry speculative intent. The existing cursor serves foreground images before continuations, including while foreground sources are loading. Scopes and color picks also precede speculative work. A continuation can yield at a processing boundary, retain its valid checkpoints and retry without sending a false completion or cancellation reply. Explicit Whole demand promotes matching continuation work. Completed speculative source handoffs cannot occupy the only loader slot ahead of a different foreground source, and queued speculative acquisition cannot overtake foreground acquisition. Already active source reads and submitted GPU work finish at their next existing boundary.

Independent review accepted the priority, loader and yield changes. Deterministic tests cover ordering, source waits, promotion, handoff release and retry. The real-worker test injects foreground demand after a Whole has submitted GPU work, verifies foreground delivery first, and then verifies the still-pending Whole completes. Native journeys with the declared four-photo regional recipes passed three trials at the existing 2 GB budget, three at 4 GB and one at 1 GB. All 28 final app images matched fresh processing. Captures were complete and loss-free. The owner visually reviewed the engine-support build and accepted it before the scheduling slice.

The original loupe check used Adaptive Tone to require Whole fallback. That expectation became obsolete. It now explicitly checks Adaptive regional detail and movement to a new region. The existing unknown-native-geometry transient-photo case continues checking Whole Detail. Output resizing is not a loupe fallback fixture because native Detail intentionally ignores export dimensions.

The following table summarizes traced foreground completion over the three edits per phase and each available trial. Foreground completion means the last current visible photo result was observed by the app. Full background settlement is measured separately. These are diagnostic Wall runs, not baseline performance percentiles, and the single native editing target's first edit includes a working-set transition.

| Cache budget | Four Fit photos, foreground ms | One native view, foreground ms | Two native views, foreground ms | One native editing target, foreground ms |
| --- | --- | --- | --- | --- |
| 1 GB | 137–160 | 1314–1359 | 1492–1761 | 109–592 |
| 2 GB | 128–147 | 621–669 | 782–1715 | 106–568 |
| 4 GB diagnostic control | 136–152 | 137–149 | 523–932 | 40–538 |

At 2 GB the one-native-view edits caused 21–30 checkpoint evictions; the 4 GB control reduced this to 4–5. Two native views still took 523–932 ms at 4 GB with no checkpoint evictions. The single native session and local graph cache cap remain independent constraints. These findings support the separate [cache workflow investigation](./cache-workflow-measurements.md), rather than raising the default or claiming regional support alone resolves zoomed latency. No memory policy or slot count changed in this slice.
