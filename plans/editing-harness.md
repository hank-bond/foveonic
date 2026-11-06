---
summary: "Measure warm multi-photo editing, host scheduling and graph boundary costs before changing production execution."
---

# Editing optimization harness

Implements the [profiling requirements](../docs/specs/profiling.md#cached-multi-photo-editing) alongside the [small-preview experiment](./rotation-and-small-preview.md). The owner requested one scale-aware recipe before quality bypasses and better evidence about scheduling and work between graph nodes. Keep production scheduling and GUI cache policy unchanged in this slice.

- [x] Attribute recipe/session preparation, checkpoint planning, operation work, cache publication, completion waits and retirement.
- [x] Add a maintained Rust harness with independent warm photo sessions, bounded host concurrency, explicit retained-buffer budget and raw shared-format records.
- [x] Separate warmup, exposure/reconstruction edits, requested/effective scale, group completion and first feedback. Keep verification outside measured intervals.
- [x] Summarize workloads by scale/edit/concurrency and export host timelines without assigning GPU passes an invented host origin.
- [x] Compare eight distinct Merrill originals with Off acceptance runs and separate Wall diagnostics. Verify exact final outputs across schedules.
- [x] Complete workspace tests, Clippy, formatting, documentation and artwork checks. The 14 harness tests, 22 profiling-tool tests, four core profiling GPU tests, two executor GPU tests and two reduced-preview framing/fallback GPU tests passed. Off/Wall activity counts and pixels match.
- [ ] Owner review before commit.

## Execution boundary

The experiment schedules complete existing render sessions through bounded host workers on the shared ordered queue. It does not batch the same operation across photos, remove cancellation fences or change immutable resource ownership. Device-wide waits may include other executors' work. Session buffer totals are logical retained storage, not total device residency. A shared production cache and stage-major execution remain later design decisions informed by these measurements.

## Local eight-photo evidence

An optimized harness run used eight distinct full-resolution Merrill originals and Thumb, with seven exposure edits per schedule. Each source retained its own warm session and executor. The first edit was separated from six settled edits. Settled whole-group medians were 136.19 ms with one worker, 76.74 ms with two, and 55.66 ms with four. First-photo medians were 18.10, 17.96 and 26.53 ms respectively. Greater concurrency improved whole-set throughput but delayed first feedback at four workers. First edits cost 259.91, 147.49 and 115.83 ms, showing why first-edit cache preparation needs its own category.

Reconstruction-changing chroma edits measured 191.94 ms serially and 137.62 ms with two workers. Final linear-buffer checksums matched across serial and concurrent schedules for each edit type. Independent follow-up runs additionally compared all eight final cached images byte for byte against fresh processing, passing for both exposure and reconstruction edits with two workers. Those runs measured settled medians of 77.99 and 160.02 ms respectively, showing capture-to-capture variation for reconstruction work. Exposure runs with Wall diagnostics also matched Off output exactly. Logical retained session buffers totalled 1,566,286,848 bytes (about 1.46 GiB), before originals, final images/textures, temporary resources and driver memory.

Serial Wall diagnostics attributed median per-photo costs of 7.10 ms to total recipe preparation (including 5.82 ms tone preparation), 7.22 ms to checkpoint/cache planning, 0.33 ms to Tone operation work, 1.30 ms to the existing node completion wait and 1.28 ms to completed display preparation. Publication was about 0.001 ms and retirement below 0.001 ms. Nested durations overlap. These results support investigating unchanged preparation/key reuse and bounded CPU preparation overlap before assuming stage-major GPU execution is necessary.

These are local headless measurements from one capture per schedule, with uncontrolled OS/GPU activity and the owned review app still open. They exclude GUI presentation and do not establish production multi-photo memory feasibility. They do not enable app concurrency or claim that each graph stage supports all smaller grids yet.
