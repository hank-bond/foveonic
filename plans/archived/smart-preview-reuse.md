---
summary: "Preserve completed Smart detail during scrubbing and reuse warm regional and whole-frame inputs across refinement."
status: archived
---

# Smart preview reuse

This slice implements the accepted [Smart preview](../../docs/specs/smart-preview.md) and [interactive rendering](../../docs/specs/interactive-rendering.md) handoff requirements with photographic arithmetic unchanged. The owner accepted modest variation above the approximately 90 ms warm slice plus Full target and approved commit and merge. Further CPU preparation caching is outside this slice.

## Work

- [x] Reproduce the display rewind and repeated reconstruction with native Smart at 200%.
- [x] Retain compatible completed detail during settings changes and submission, with newer whole-frame results taking precedence.
- [x] Separate native whole Full and current preview version/frontier metadata within one bounded shared buffer cache.
- [x] Extend the Smart profiler with persistent-session warm exposure handoffs and combined completion timing.
- [x] Verify exact cached/fresh pixels, cancelled upstream edits, retained-image ownership and one aggregate budget on Merrill and ordinary Quattro originals.
- [x] Finish native regressions, workspace checks and reopen the app for owner review.
- [x] Owner review and approval to commit and merge.

## Findings and evidence

The interface filtered regional detail against the newest requested edits and discarded it on submission. Both steps exposed an older whole-frame base while the next tile rendered. Presentation now checks compatible geometry and revision ordering independently of scheduling's exact-edit match.

The old cache used one output-name version map and one selected frontier. Regional and whole graphs overwrote matching logical names with different geometry recipes. Repeated exposure edits consequently reran unchanged reconstruction. Two metadata working sets now retain their own restart boundaries while sharing exact upstream generations and immutable storage under the existing 1 GiB admission limit. Changed controls and source/engine identity still invalidate through processing-owned recipes.

On the blue-sign Merrill original, the baseline after whole-frame continuation repeatedly took about 130 ms for regional exposure feedback and 440 ms for whole Full. The revised warm exposure handoff took 94–112 ms total in eight completed unprofiled pairs: approximately 32–42 ms for the slice and 61–75 ms for whole Full. This includes rendering, texture preparation and GPU completion, with prior display artifacts retained. Source decode and initial grid preparation are excluded. Retained GPU cache was approximately 564 MiB. These are local Apple M1 Pro measurements, not physical input-to-screen timing or a universal latency bound.

The GPU regression primes Fast, native detail and whole Full, then alternates exposure edits between detail and Full. Both grids dispatch only the photographic response, match fresh float output exactly, and preserve a retained original image. A cancelled noise change resumes with exact fresh output in both grids. It passes for the Merrill blue sign and ordinary DP0 Quattro.

The native Smart regression now changes exposure twice after a completed tile becomes visible and asserts that the same texture remains visible before and after each submission. It also checks that a newer Full result suppresses old detail during the next edit. Merrill and ordinary Quattro pass the complete sequence, including crop/zoom transitions, Adaptive fallback, scopes and cache reopening.

Workspace tests pass, including 417 ordinary core tests and 318 GUI tests. The two existing cache GPU checks, Merrill/Quattro cross-grid regressions, strict workspace all-target Clippy, formatting, documentation indexes and five artwork checks pass. The native app was rebuilt and reopened on the local example collection before owner approval.

Reproduce completed warm handoffs with a new report path:

```sh
cargo run --offline --release -p foveonic --example profile_smart_preview -- ORIGINAL.x3f REPORT.json --warm-edits --repeats 8 -- --reconstruction
```
