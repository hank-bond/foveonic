---
summary: "Integrate wide rendering and relative group viewing, then prepare and validate the 0.9.0 minor release with the maintained release lanes."
---

# Release 0.9.0

The owner authorizes merging the completed wide-rendering work into main, preparing the next minor release and running the new release test process. The subsequent Shift viewing reports belong to this candidate: each photograph must preserve its own point at the same relative position within its displayed region, independently of aspect ratio or pan. Shift-drag must move every edited photograph by the same on-screen displacement. Publication remains a separate owner review.

Requirements: [release specification](../docs/specs/releases.md), [release guide](../docs/releases.md), [test inventory](../docs/testing-coverage.md) and [multi-edit viewing](../docs/specs/multi-edit.md#per-photo-viewing). The [cleanup history](archived/wide-rendering-cleanup.md) records earlier experimental validation. That evidence does not replace testing the merged release candidate.

## Scope

Ship memory-aware wide rendering through the shared execution lifecycle, priority for viewport and auxiliary work over browser previews and idle continuation, compatible preview reuse, optional idle full-image completion, group Before/After and linked Shift gestures. The release build enables the previously explicit wide feature. Preserve subsequent main changes through 0.8.4, including capture metadata, encoder compatibility, Windows startup and the new test process.

The Shift-zoom fix passes all 639 ordinary GUI checks after reproducing both the visible mixed-aspect bug and a known-hidden Fit regression. The Shift-pan case independently reproduces unequal on-screen displacement before its fix. The final combined GUI suite passes all 642 ordinary tests, including mixed aspect ratios, both source orders, different zooms, display densities, Space/tool isolation and hidden/carry behavior. Independent reviews have no outstanding findings. These intermediate checks do not replace the final candidate run.

The release process uses one frozen source/configuration, independent Mac and Windows lanes, maintained required-test classification, bounded concurrent slider sessions with independent numerical verification, exact packaged startup/rendering checks and separate exclusive performance. Published 0.8.4 is the performance baseline. A conditional parallel-host schedule is not a release duration guarantee.

## Work

- [x] Resolve and independently review the merge against current main. Preserve unrelated 0.8.4 release records in the main checkout.
- [x] Reproduce and fix Shift-zoom anchoring with behavioral mixed-aspect, pan, wheel/pinch, selection and tool checks.
- [x] Enable the tested wide release configuration, set 0.9.0 and prepare reviewed changelog/draft notes.
- [ ] Freeze matching app, renderer, tests and inputs. Reconcile every ignored test with the maintained capability inventory.
- [ ] Run ordinary workspace/renderer, lint, Python, documentation and all required GPU/original-file checks on both backends.
- [ ] Run the complete native catalog, all slider cases and approved numerical comparisons using independent host lanes and exact-union report checks.
- [ ] Build the signed macOS and Windows draft packages and retain matching developer symbols. Verify the actual archives and normal-stack desktop smoke checkpoints.
- [ ] Compare exact packaged candidate latency and separately sampled memory against published 0.8.4 on matched, exclusive workloads. Investigate reproducible changes.
- [ ] Summarize unavailable platforms, coverage gaps, failed or deferred checks and performance observations for owner review before publication.

## Initial evidence

The prior wide slice passes its default/feature workspace suites, 46 selected GPU/public-flow controls, 144 frozen numerical cases and a 78-transition native journey. Current main supplies the subsequent fixes and bounded release tooling. Merging them invalidates affected candidate evidence and requires the new release checks.

The merged ignored-test inventory selects 499 required checks in 25 serial commands and records 158 explicit exclusions. This adds 37 wide correctness checks and four timing/attribution exclusions. Feature-enabled discovery is mandatory so gated checks cannot disappear from the inventory. Standard fixture sets already contain the pinned wide originals. Classification is not test execution or a waiver.

The prepared secondary hosts verified their source paths, input identities, toolchains and retained packages before execution. The 16 GiB Mac ran required Rust checks and non-slider native work. The 32 GiB Mac ran bounded sliders, then exact-package smoke and exclusive performance. Windows used its interactive desktop token for packaged GUI validation. Ordinary local test apps and local test executables must not be relabeled as production packages.

## First frozen candidate

Candidate `f80e306` passed ordinary workspace checks, the nineteen renderer checks, strict Clippy and all 499 required ignored checks on Metal and DirectX 12. The maintained inventory records 158 research exclusions. Python, artwork and documentation checks passed on the primary Mac. The optional checkpoint-capture integration also passed after a fresh native capture and trace analysis. All 1,440 slider cases in 28 groups passed independent image verification. All 144 numerical cases matched the approved samples and geometry. The exact signed macOS and Windows packages passed their ten desktop smoke checkpoints at normal stack sizes. All four release CI jobs passed, including Sequoia binary loading. The draft remains unpublished.

The complete native catalog ran all 48 selected jobs. Thirty-seven passed and eleven failed. Preserve that first result while correcting the findings. Several checks still assumed a native whole image was required before useful regional completion, or compared scope geometry against the reduced base instead of the covering native section. Strict warm-reuse checks also began before the demanded working grid was prepared when layout had reused a finer image. Traced correctness now prepares that grid explicitly, preserving subsequent zero-read assertions and the natural Off latency sequence.

Navigator restoration exposed a product bug. A real-worker component test reproduced blocked cached Current images after Camera, inspection and another photograph because their source revision was older than the displayed tile. Exact requested-key replies now restore those images while nonmatching older replies retain their ordering check. The same behavioral test passes and preserves the displayed texture against late obsolete Current, Camera, inspection and other-photo replies. Native readiness remains strict, with additional collapsed/open restoration steps. This new required GPU check brings the inventory to 500 checks in 25 commands. Independent reviews found no outstanding static issues. The rebuilt native cases and frozen-candidate checks remain required.

Targeted native runs now pass Smart completion, Fast, warm reuse, cached switching, rapid supersession, the regular transition journey, workflow restoration, scope resizing, multi-image comparison and source loading. The low-memory journey completes its authored edits and native-region coverage, but its trace was correctly rejected for one capture-transition drop. A CPU behavioral test reproduces admission remaining open while stop waits for an accepted emitter. Recording now closes admission before draining those emitters and opens only after startup releases its gate. Genuine contention during an active capture still counts as loss. The new test and existing generation controls pass. The rebuilt low-memory case passes trace integrity and independent image verification. A separate eight-image traced warm-edit case also passes its zero-read expectations after explicit demand-grid preparation. The replacement frozen candidate still requires its complete release lanes.

The matched packaged performance comparison retains initial and reversed trials, actual grids and separate sampled footprint. Native foreground zoom and edits were approximately unchanged in the reversed observations, while optional Whole completion was later. Whole completion now follows an independent auxiliary preview. Mixed-family group-settlement deltas also include a changed progress-readiness boundary, so they do not establish rendering-only regressions. Two comparisons were unavailable because delivered grids differed. Larger-budget native memory growth remains an observation for owner review. Sustained idle settlement, physical presentation and native Sequoia GUI coverage remain unestablished. These first-candidate observations do not automatically validate a rebuilt candidate.

The replacement candidate exposed an unsupported deadline in the new capture test on debug CI. Saving completed in 8.83 seconds after the test had rejected its five-second wait. Earlier successful diagnostic captures took roughly eight to eleven seconds on that runner. The test now awaits save completion like the existing capture tests, while preserving its bounded admission check and every event, generation and loss assertion. No product behavior changes for this correction. Preserve the rejected candidate records under their original identity and rerun the replacement source.
