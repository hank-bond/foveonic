---
summary: "Profiling separates ordinary completed work, synchronized GPU diagnostics, and native feedback with reproducible context."
kind: reference
status: accepted
implementation: conforming
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
