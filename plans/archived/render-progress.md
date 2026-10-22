---
summary: "Replace loading text with discrete source/render progress and request-to-image milliseconds."
status: archived
---

# Render progress

The owner approved a progress bar and total milliseconds in the loading area. Progress must reflect actual pipeline stages, including source-file reading, with equal sections and no interpolation. The [interactive-rendering requirements](../../docs/specs/interactive-rendering.md) define the accepted timing boundary and source/render sections.

- [x] Observe completed source reading and decoding without changing samples.
- [x] Report prepared recipes and completed required graph operations after existing interactive GPU waits.
- [x] Share only the latest revision's progress outside the bounded reply channel.
- [x] Replace the normal spinner/status text with a bar and elapsed milliseconds.
- [x] Verify actual source/render hooks, cached work, supersession and frozen totals.
- [x] Check native rendering, failure tracking, documentation and the local app build.

Progress has five equal sections. Reading completes the first, decoding/metadata the second, recipe preparation the third, graph operations share the fourth, and display preparation/delivery completes the fifth. Cache reuse completes skipped work immediately. DNG exports use a combined preparation/render completion boundary and do not yet expose appearance-fitting substages.

The UI polls one revision-tagged snapshot. Rendering callbacks neither block on the reply queue nor request UI-context locks. The elapsed label requests a repaint every 50 ms while active. The bar itself has no animation or time interpolation. Completion freezes on current-revision image delivery after GPU view preparation completes. This includes worker queueing and excludes deferred scopes, navigator work and physical scanout.

Validation passed all 453 ordinary workspace tests and strict workspace Clippy. A real DP0 Quattro original reported source read/decode boundaries and 34 Full graph stages, 21 Fast stages and nine cached Fast exposure-edit stages. Every observed Float32 output exactly matched its unobserved rendering. Tracker tests cover stale revisions, monotone updates, fully cached execution, failure state and frozen elapsed totals.

The native app completed Full/Fast switching, zoom and exposure-edit captures. Each completed image had revision-matched 100% progress and a frozen total at least as large as worker processing time. The visible bar and millisecond label were checked in the real workspace. The local app bundle was rebuilt without installation.
