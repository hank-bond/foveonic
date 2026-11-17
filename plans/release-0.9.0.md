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

Both prepared secondary hosts respond to read-only connection probes. Their source paths, inputs, toolchains and retained package identities still require verification before use. The 16 GiB Mac can run required Rust checks and non-slider native work. The 32 GiB Mac can run bounded sliders, then exact-package smoke and exclusive performance. Windows uses its interactive desktop token for packaged GUI validation. Ordinary local test apps and local test executables must not be relabeled as production packages.
