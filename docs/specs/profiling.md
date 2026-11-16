---
summary: "Profiling separates ordinary completed work, synchronized GPU diagnostics, and native feedback with reproducible context."
kind: reference
status: accepted
implementation: incomplete
---

# Profiling

## Measurement modes

Profiling must support off, wall-timing and GPU-diagnostic modes. Off must avoid optional recording. Wall mode may record host wall durations, lifecycle observations and resource counts, but must not request GPU timestamps, add completion waits or change repaint scheduling. GPU mode must reuse the core timestamp collector and clearly identify its additional synchronization. Missing timestamp support must be explicit, not silently interpreted as zero GPU cost.

Host spans are elapsed wall durations, not CPU-active time. Nested host spans and GPU passes must not be added as if they were disjoint. Allocation totals are not process residency. Native callback encoding and image receipt must not be labeled as completed presentation or scanout.

## Reports and context

CLI and GUI reports must use structured JSON with a common envelope and mode. The core profiler owns processing host/GPU measurements and activity counts; GUI code adds lifecycle, frame and callback observations. Loading measurements must be reported separately when loading occurs, not copied into warm render profiles.

Reports must identify the source, settings, output and fresh versus session execution where applicable. A changed algorithm's settings and stage labels must distinguish it from existing methods. Repeated passes must remain distinguishable in raw records and aggregate by meaningful stage, with counts and per-render totals rather than unstable list positions.

## Reusable measurements

Reusable capture and analysis scripts must be maintained outside ignored evidence directories. Scripts must retain raw records, isolate app preferences, stop only their own process, and reject accidental profiling-mode inheritance. Workload checks indicate known concurrent processes, not proof that all system GPU activity is absent. Native pointer posting and pipe receipt do not establish precise input-to-scanout timing.

Completed full processing and warm interactive measurements must remain separate: a reused denoising boundary does not measure the denoiser's execution cost. Algorithm comparisons must record their recipe, run warm repeated measurements without timestamp synchronization for wall-time comparisons, and validate photographic results separately. Memory sampling must be outside timed interactions. Experiments and historical bundles remain evidence, not maintained tools.

Heuristic optimizations may change photographic output. Their evaluation must measure camera-code differences with an explicit conversion scale and bit depth, and visually inspect affected areas, primarily through crops. Whole-frame aggregate error alone is insufficient. Include concentrated errors and representative edges, texture, dark regions and highlights, and describe the visible effects. Keep quality comparisons outside timed intervals and distinguish approximate results from exact-output optimizations.

## Cached multi-photo editing

The optimization harness must retain a distinct render session for each original in a measured photo set. Concurrent native sessions must use separate executors. A serialized reduced working-set trial may share one executor, matching the app's bounded owner, and must identify that execution policy explicitly. Repeating one source must not be presented as editing different photographs. Measure completed warm exposure edits separately from edits that invalidate reconstruction. Record processing quality separately from requested and effective working resolution, real settings, output extent, warmup, per-photo completion and whole-group completion.

Compare sequential whole-photo rendering with bounded host concurrency on the same ordered device queue. Keep production scheduling unchanged during this measurement slice. Concurrent whole-photo work must not be described as batching the same graph stage across photographs. Record logical retained buffer totals and enforce an explicit measurement budget. These totals exclude originals, display textures, scratch and driver allocations.

Node diagnostics should distinguish operation work, cache publication, existing completion waits and buffer retirement. Preserve node/output cache observations and host timeline placement. Timeline exports may show host overlap, but must not place GPU passes on a host clock without a measured shared origin. Separate profiling Off acceptance runs from Wall attribution and synchronized GPU diagnostics. Collect verification readbacks outside timed edits.

## Checkpoint diagnostics

Optional graph diagnostics should connect stage timings with output dependencies, buffer identities and logical byte sizes. Retained-checkpoint snapshots should describe exact version ancestry and cache roles. Diagnostic records must own metadata only and must not extend processing-buffer or parameter-payload lifetimes. Memory scans and verification belong outside ordinary node timings. Boundary ownership counts must identify their scope and must not be described as transient GPU peaks or process footprint.

Checkpoint investigations should distinguish the first changed edit from later same-control edits. Use separate ordinary latency and diagnostic runs, and validate reused output against fresh processing. Graph ancestry timing estimates must remain distinguishable from measured cache-policy benefits.

## Release performance baselines

Each release must include a versioned performance baseline and a before/after comparison against the previous published release, covering latency and memory with maintained test photographs and fixed, reproducible recipes. Complete and review these comparisons before publication. Include enabled expensive features as well as camera defaults. Mutable local sidecars must not define the workload. Record processing policy and effective working resolution separately.

Baselines must retain repeated Off-mode timing observations, input and recipe identities, build identity, hardware, cache setup and relevant display geometry. Cover decoded-input preparation, retained edits, native detail and representative multi-photo workflows separately. Keep source loading, foreground delivery and background completion distinguishable. Report UI-update gaps independently of engine processing time.

Compare matched workloads per photograph and workflow so aggregate improvements cannot hide individual regressions. Hardware-dependent comparisons require the same host, or new matched runs of the released binary and candidate. Regression budgets require explicit approval. Recording slow behavior does not make it an acceptable target. Keep synchronized GPU attribution and numerical output verification separate from baseline timing, and preserve raw evidence with the release results.

Sample process memory separately from latency runs. Record the metric, sampling interval, measured stages and whether a value is settled usage or an observed peak. Include applicable GPU memory separately where available. Missing measurements and a previous release that cannot complete a workload are unavailable comparisons, not performance passes. Publication with missing comparisons or unresolved regressions requires an explicit release-specific owner decision.

The [finishing optimization plan](../../plans/archived/finishing-optimization.md) records the fresh-processing baseline and comparison-tool extension. Remaining workflow baseline linkage is tracked in the [performance-diagnostics plan](../../plans/performance-diagnostics.md#release-workflow-baseline-follow-up).
