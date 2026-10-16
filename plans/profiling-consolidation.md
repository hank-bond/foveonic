---
summary: "Consolidate retained core, GUI and native profiling into explicit modes and reusable structured reports."
---

# Profiling consolidation

Implements [profiling requirements](../docs/specs/profiling.md), approved after the diagnostic design review and completed in the isolated editing branch. Existing core and GUI guards, shaders, arithmetic, operation waits and publication priority stay unchanged.

- [x] Replace boolean profiling with off/wall/GPU modes; separate loading from render profiles.
- [x] Use common JSON envelopes, current host/GPU profiles, activity counts and source/settings/execution context.
- [x] Centralize GUI configuration and remove profiling-only repaint requests.
- [x] Promote reusable native recorder, workload monitor and one analysis path, with tests.
- [x] Make the full-processing runner accept the normal photographic controls instead of requiring edits to a fixed recipe.
- [x] Document usage and limitations; migrate callers and indexes without legacy live paths.
- [x] Run CPU tests, strict checks and coordinated GPU/native validation.

No new denoiser, highlight method, dashboard, tracing framework, image cache or asynchronous timestamp collector is included.

The shared parser lives in `crates/foveonic/src/options.rs` and is compiled by the CLI and completed-processing example, not exposed as a photographic-library API. Settings/control types gained serialization only, including `tone.rs`; the earlier no-tone-cache decision remains unchanged. GUI request reports record requested controls; processing reports record camera-selected WB/mode and controls. Derived camera matrices/tables remain reproducible from source rather than being copied into every report.

Core API entry points reject unsupported explicitly requested GPU diagnostics; wall collection never enables their trace queries even on timestamp-capable devices. Counts remain engine-owned, not GUI-wide or residency estimates. Portable tool summaries keep modes, cold/warm full processing, and native receipt distinct. Historical experimental tools and bundles are not maintained compatibility paths.

Validation passed: 56 ordinary core tests, 68 ordinary GUI tests, strict all-target Clippy, examples, formatting, documentation checks/tests, bundle/artwork tests and 13 portable profiling-script tests. Serial GPU validation passed 53 core tests and five GUI tests, including the new wall-mode exact-pixel/no-resolve regression. Both native modes ran through the recorder/analyzer with current settlement on all six Detail drags, one loading report per source and no sampled overlapping workload. Wall native/device activity confirmed no timestamps; GPU mode confirmed enabled queries and labeled diagnostic results. The completed-processing runner accepted the shared photographic options, separated ordinary cold/warm samples from its extra GPU diagnostic, and matched its exact golden across independent runs. CLI wall output used the same JSON envelope. These captures validate capabilities, not a responsiveness speedup.

Final review also kept callback micro-timing summaries restricted to timed pointer windows, rather than diluting them with idle callbacks for the same displayed revision. Preserved upstream processing spans still join delivered revisions outside the pointer window. Raw records were replayed after this portable analyzer correction.

Final review retained the existing `scripts/` root so Python tools receive generated indexes without adding a parallel root to the docs generator. Reusable tools are in `scripts/profiling/`. The scripts summarize one supplied dataset; source/settings remain in raw records, and methods/recipes should be compared in separate datasets.

The completed profiling slice was committed as `088a8c2` on local `main`. The owner subsequently approved integrating the separately checkpointed guards, inspection differences, sidecars, and DNG design documents, then removing the clean editing worktree. Integration preserves the maintained modes and reports, adds comparison-root context to processing diagnostics, and counts comparison-view preparation in view activity. Final combined validation is recorded in the sidecar and editing handoff plans.
