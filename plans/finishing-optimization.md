---
summary: "Restore practical brightness-guide grids, establish repeatable comparisons and optimize measured finishing bottlenecks in reviewed checkpoints."
---

# Finishing latency optimization

Requirements: [processing quality](../docs/specs/processing-quality.md), [profiling](../docs/specs/profiling.md) and [reproducible inputs](../docs/specs/performance-diagnostics.md#reproducible-test-inputs). The owner authorizes the following sequence, with independent largest-model review and commits between safe slices. Keep all GPU measurements and compiler work serial on this host. Preserve source photographs and authored edits.

## Approved sequence

- [x] Restore the previous half-width, half-height brightness guides and gain anchors in both processing policies. Keep native photographic resolution, dense Quality scene estimation and policy-consistent section/Whole guidance. Remove unused dense-guide paths. Verify Speed parity, source-independent arithmetic, photographic section/Whole agreement and latency. Review with Astra, clean up and commit.
- [x] Reprofile the resulting graph and pair with Astra to rank optimization opportunities by measured latency. Include fresh preparation and retained edits, since a reused node is not a measurement of its execution cost.
- [x] Add a fresh-processing comparison foundation using fixed workloads, Off timing distributions and separate exact-output checks. Keep source loading and warmup explicit.
- [ ] Extend the release matrix and link retained-edit, native/multi-photo and separate GPU attribution evidence. Keep release performance baselines compact, versioned and identifiable by hardware, source hashes, recipes, processing policy and effective resolution. A recorded slow result is not an approved latency target.
- [ ] Delegate independent, narrowly scoped optimization experiments to Sol agents. Each owns separate source changes or an isolated worktree with the shared build cache. The parent grants compiler/GPU windows and collects measurements. Review and merge useful changes separately. Discard unsuccessful prototypes without changing production defaults.
- [ ] Run targeted application regressions, summarize accepted improvements and remaining opportunities, and provide a review build.

## Comparison gates

Performance observations use profiling Off with warmup and repeated samples. Keep GPU diagnostics and verification readback outside those timings. Match actual image grids and edited recipes between candidates, preserving per-photo results so aggregate improvements cannot conceal a regression. Include the flower recipe that exposed the Quality slowdown, plus representative supported camera families and expensive controls.

Output-equivalent optimizations must preserve samples and geometry against an immutable checkpoint. Use the existing exact matrix and fresh replay machinery. Independently check policy, cache reuse and section/Whole overlap where relevant. Approximate algorithm candidates require explicit measured error and photographic review, and must not silently replace the approved implementation. Retain reproducible test declarations in the repository rather than depending on mutable local sidecars.

Record observed UI-update gaps separately from render latency. The flower investigation found main-thread display-surface acquisition blocked during long compute stages even though application UI-update scopes remained short. Faster numerical execution may improve both, but do not infer that from engine-only timings.

## Starting evidence

The [Quality/Speed plan](./quality-speed.md#owner-review-flower-latency-and-ui-stalls) records the dense-guide checkpoint. The flower's single Off render took 2.47 seconds at divisor two and 14.78 seconds at native Quality. The same divisor-two recipe took 0.41 seconds in Speed. Instrumented native Quality attributed about 10.74 seconds to two brightness guides, 1.76 seconds to Dehaze estimation and 1.39 seconds to Adaptive fitting. These figures identify initial candidates, not the ranking after restoring half-grid guidance.

The broader data-structure and resource-lifetime memory pass stays deferred. Continue using explicit sampled physical-footprint guards during test runs.

## Half-grid checkpoint evidence

The restored shared guides remove the dense-guide branch and its redundant node-level policy parameters. Request/result policy remains intact. Guide arithmetic uses the existing scalar accumulator and optional broad-scale coefficient grid. The source cleanup received independent review.

Ordinary engine checks passed 565 tests plus 17 CLI tests. The 12 guide GPU cases and seven synthetic Adaptive cases passed with nonzero matched counts. Sixteen active-feature Speed cases retained their frozen float hashes and geometry, and all 80 unaffected camera/scale/policy comparisons remained exact. The latter cover three Classic, three Merrill, three Quattro and one Tiger source.

Single Off observations of the flower's unchanged recipe improved from 2.47 to 1.06 seconds at divisor two and from 14.78 to 4.19 seconds at native Quality. Speed at divisor two remained approximately 0.42 seconds. These observations precede the dedicated repeated benchmark slice.

The cold region/Whole gate exposed a photo-edge distinction hidden by dense gains: an interpolated half-grid gain may need a padding anchor immediately outside the active photo bounds. Whole indexing already accesses it, while mapped regional sampling clamps to active bounds. Same-grid mapped HDR guides now expose the same stored padding anchors. Smaller-grid Speed guidance preserves its existing crop clamp. Validate this change without loosening the overlap oracle. Adaptive structure probes separately require active-bound nearest-cell semantics.

The corresponding Adaptive correction keeps one active-guide domain and derives available storage bounds for base anchors. Its explicit same-grid sampling flag comes from the relationship between the application and main working grids. Structure probes affine-map raw pixel coordinates, clamp to original Whole active bounds and index their containing cell. Reduced-guide interpolation is unchanged. An independent synthetic oracle covers nonconstant padding, odd photo bounds and translated regions through right/bottom edges. Both photographic cold-region suites passed after this correction on Merrill and Quattro, including Color/Monochrome, Natural/Camera, both policies, panning, exact Whole continuation and retained earlier images. The existing overlap tolerance was preserved.

Independent review approved the final source and test cleanup. Ordinary core tests, strict workspace Clippy and formatting passed. Keep these as the rollback checkpoint before numerical optimization experiments.

A final exact Speed comparison found sparse floating-point drift when the same-grid lookup was introduced as a runtime shader branch. Both equivalent runtime source forms changed 1,691 of 48,254,976 components in the native coast fixture, with a maximum normalized difference of 0.00000234. The frozen earlier binary reproduced the expected hash. One compile-time specialized gain pipeline restores the original reduced-guide path's exact hash while retaining same-grid padded sampling. It selects a single shader override from the already-keyed geometry flag, with unchanged bindings and uniform layout. Independent review approved this small specialization rather than a second shader or a generic framework. Compare actual GPU output even when a supposedly inactive source branch preserves the mathematical expression.

The final specialized implementation passed the 565 ordinary engine tests, seven synthetic Adaptive GPU tests, both photographic cold-region suites and all 16 frozen active-feature Speed comparisons. Failed runtime-branch captures remain failed evidence, not new goldens. The 80 unaffected comparisons and 12 guide GPU arithmetic checks remain applicable to the unchanged operations they exercise.

## Accepted appearance and comparison foundation

The owner explicitly accepts the current `8c0da85` rendering as the quality baseline. Adaptive and Dehaze are photographic heuristics rather than operations with one physically correct output. Alternative heuristics may be explored in isolated candidates and judged by fidelity to this baseline, visual review, latency and memory. Preserve the baseline instead of regenerating it for a candidate. Unchanged and arithmetic-preserving paths retain exact gates. Intentional approximations need measured errors and representative crops, with no automatic default promotion.

The thin Python fresh-processing runner reuses the existing case expansion, renderer, owned-process guard, workload monitor and distributions. It records Off warmup and measured iterations separately, hashes the first image outside timing, and compares numerical identity independently of timing budgets. No second Rust runner or source-attestation framework was added. The full Python suite passed 180 tests with one skip, and independent review approved the source.

A real four-case baseline/candidate pair exercised coast, dog and Quattro H with fixed Natural/HDR/Auto Dehaze recipes, plus unchanged native Speed on coast. All output SHA-256 values and geometry remained exact. The first experiment, changing Adaptive summary workgroups from 64 to 16 without changing math, showed approximately 1.6–2.0 percent lower medians on coast/dog, no H improvement and a 3.4 percent higher Speed median. With only three post-warmup samples and small changes, this is not a demonstrated useful win. The experiment remains unmerged.

The post-rollback diagnostic sweep covered ten maintained camera fixtures plus the flower at native and half resolution. Dense Adaptive summaries dominate H (about 5.61 seconds) and cost about 1.34–1.52 seconds on flower/coast. Dehaze partition plus merge cost about 2.91 seconds on H and 1.35–1.37 seconds on flower/coast. Its minima cost another 0.30–0.70 seconds. These synchronized diagnostic observations choose experiment scope, not release latency targets.

The first heuristic Adaptive proposal bounds each Whole-normalized region at 128 by 128 midpoint strata, with 64 by 64 as a separate speed candidate. It retains complete histogram preparation and native application, scales support by represented area per scheduled sample, and preserves weighted populations and the summation tree. Thin highlights and weak shadows require explicit phase-sensitive tests and crop review. Dehaze's next approximate proposal, if the exact partition trial is ineffective, replaces heap boundary selection with a dark-score histogram tail average. Neither proposal is approved as a production default.

## Cache admission correctness

A rejected large checkpoint previously could discard lower-priority entries before discovering that protected buffers prevented admission. A pressure-only preflight now counts unique incoming and protected allocations before mutating the cache. Successful admissions keep the existing priority and recency policy. Four small GPU regression cases passed, including shared allocations and immutable existing versions. Independent review approved the focused change.

The native flower Exposure comparison showed no useful latency improvement: three Off edit observations were approximately 3.61/3.82/3.81 seconds before and 3.61/3.80/3.80 seconds after. Dehaze still executed on the next edit and final retained byte counts matched. This is a correctness fix, not evidence that the flower's repeated estimation has been solved. The same artifact collector requests Auto Dehaze analysis in the app and profiler, but this headless measurement does not establish application latency. Replaying graph allocation identities against the existing eviction order explains the loss: admitting a 193,019,904-byte RGB checkpoint evicts both a prior RGB checkpoint of that size and the 32-byte Dehaze estimate. The net 32-byte reduction therefore represents a successful replacement. This separate retention-policy tradeoff remains open.

## Bounded Adaptive estimate experiment

The 128-by-128 region lattice passed source review as an isolated heuristic candidate. A matched five-case Off comparison, with one declared warmup and three measured fresh renders, reduced native median time from 4.94 to 1.95 seconds on coast, 6.79 to 3.56 seconds on the Quattro dog, and 10.75 to 5.16 seconds on Quattro H. Source loading is separate. Full geometry matches throughout. Speed retained its exact float SHA, as did the owner-owned reduced Merrill Thumb fixture whose normalized windows never exceed the cap. The three large Quality outputs changed and remain unapproved. The fixed edited-flower recipe improved from 4.15 to 2.95 seconds natively and 1.08 to 0.75 seconds at divisor two. Its visible mean/maximum RGB errors were about 0.00000025/0.0000132 natively and 0.00000021/0.00000732 at divisor two. Both Speed flower outputs remained exact. These observations establish a promising candidate, not a replacement baseline or production default.

Visible-crop comparisons in normalized linear working RGB found mean/RMS/maximum errors of 0.000127/0.000178/0.001636 on coast, 0.00000306/0.0000222/0.000244 on dog, and 0.000106/0.000152/0.000763 on H. Signed mean D50-Y brightness shifts were approximately -0.0143, -0.00162 and +0.0222 percent. These are numerical observations in linear units, not perceptual acceptance. Unseen padding is excluded.

The new adversarial synthetic test initially failed its fixed absolute tolerance on a mixed thin highlight and weak shadow population. Its original first-sample origin can leave large moments whose subtraction is ill-conditioned. Independent review approved an explicit moment-scale floating-point variance bound while retaining tight mean/support checks and the separate tight weak-population oracle. The shader is unchanged. All eight synthetic GPU cases passed after that test-only correction, including the separate tight weak-population case. Both existing photographic cold-region suites also passed on Merrill and Quattro. A separate flower diagnostic reduced Adaptive regional-summary GPU time from about 1.34 seconds to 37 milliseconds, with the complete Adaptive operation about 74 milliseconds. Defer the 64-sample experiment while larger Dehaze costs remain. Preserve the initial failure as evidence.

## Exact Dehaze partition trial

The partition-count plumbing and 16/32/64 variants preserved exact float hashes and geometry in the four initial photographic cases. Thirteen GPU cases passed, including independent serial selection, equal-score/intensity index ties and wide integer sums. Compared with the original 16-partition checkpoint, 32 partitions reduced complete native medians by roughly 0.36 seconds on coast, 0.46 seconds on dog and 0.26 seconds on H. The 64-partition variant did not consistently improve on 32 and requires almost four times the original heap allocation. Small Speed variations occurred even in the no-Dehaze control, so these low-count timing observations do not establish small per-case regressions or statistical significance. The variants remain unmerged while a bounded selection heuristic is evaluated.

The next isolated Quality Auto Dehaze candidate retains dense minima, eligibility, selected population size, brightness calibration and correction. It groups dark-score candidates into 256 bins, uses exact wide integer counts and RGB sums, and fractionally includes the boundary bin. It intentionally removes fine score/intensity/index ordering within that bin. Speed, manual color and patch picking retain their existing paths. This is an approximate estimator requiring separate photographic comparison, not an approved default.

## Completed harness and cache checks

The snapshot workload extension and portable export received independent review. A real four-case edited-flower capture exercised both policies and native/half grids with frozen checkpoint processing, and its portable export compared exactly with the complete report. The maintained manifest now declares 18 recipes over 13 distinct originals, including owner-owned Medium/Low Merrill and fixed edited flower settings; it expands to 144 grid/policy cases. The complete 144-case fresh-processing baseline then passed, with one excluded warmup and five measured iterations per case, using the immutable `8c0da85` executable. The portable report under `scripts/profiling/baselines/` compares exactly with the complete capture and contains no local input or artifact paths. Retained-edit, native-detail and UI-delivery baseline linkage remains separate work.

The Python profiling suite passed 183 tests with one skip. The cache source change passed 565 ordinary engine tests and 17 CLI tests, in addition to its four guarded GPU admission cases. A guarded native flower retained edit matched independent fresh processing exactly. Strict workspace release Clippy, formatting and regenerated documentation indexes passed. The cache budget and successful-admission eviction policy remain unchanged.

## Exact expanding Dehaze minima

The staged integer filter reuses the existing two scalar scratch planes. A first centered three-by-three RGB minimum establishes radius one. Each scalar pass expands covered radius by `d <= 2r+1`, preserving a continuous square support and clamped boundaries. Larger windows need a logarithmic number of passes. Radii at most 16 retain the cheaper existing path; manual color skips spatial minima as before. No new image buffer or photographic option was added.

Independent review approved the interval proof, bindings, buffer swaps and profiling capacity. Fourteen Dehaze GPU cases and 565 ordinary engine tests passed. A ten-source Native Quality Auto Dehaze matrix preserved every float SHA and geometry: three Classic, three Merrill, three Quattro and one Tiger. Separate three-sample Off medians improved from about 4.99 to 4.60 seconds on coast, 6.93 to 6.58 on dog and 10.93 to 10.32 on H. These are observations, not latency budgets. On the flower diagnostic, spatial minima fell from about 307 to 28 milliseconds with unchanged selection math and scratch footprint.

An apparent inactive-Speed slowdown did not persist in interleaved controls with two excluded warmups and seven samples each: baseline medians were 317/299 milliseconds, candidate medians 305/318. Both paths remained exact. Keep this variability visible rather than inferring a regression or improvement from one three-sample control.

The first 256-bin estimate candidate passed 16 Dehaze GPU checks and a separate both-policy manual-calibration test. Actual manual Quality/Speed photographs and both edited-flower Speed grids retained exact hashes. Visible-crop linear RGB mean/maximum errors were approximately 0.000144/0.00729 on coast, 0.000267/0.00243 on dog and 0.000460/0.00206 on H. The native flower changed by about 0.00000076/0.0000142. These remain unapproved photographic differences. Its partition pass still costs about 640 milliseconds on the flower, motivating a bounded parallel-partition experiment before choosing the estimator.

## Combined owner review

The owner authorizes combining the successful optimizations for one final review rather than requiring isolated approval of each candidate. Preserve `8c0da85` and its complete release observations for numerical and photographic comparison. The combined implementation includes bounded Adaptive summaries, exact expanding Dehaze minima, score-bin tail averaging and sample-count-based bin partitioning. Quality appearance remains pending the owner's combined review; this checkpoint does not publish a release or replace the accepted baseline.

The ignored native/DNG/cache integration check failed at the same amount-reuse assertion when run from clean `8c0da85`. Independent review identified a stale grid transition: it changed Native to Half along with Amount, then required estimate reuse and equality with Native bytes. The corrected check preserves native Auto/picking/DNG validation, releases those resources, seeds a separate Half session and keeps that grid fixed for Amount 45→35. Both amount-only edits must reuse estimation; returning to 35 must match the fresh Half pixels exactly. No numerical tolerance is relaxed.

The combined build passed 568 ordinary core tests, 17 CLI tests, strict workspace release Clippy and packaging. Its GPU checks passed 19 Dehaze cases, eight Adaptive cases and four cache-admission cases. Both Merrill and Quattro passed all three Dehaze swatch/DNG/cache integration checks, including the corrected same-grid reuse assertion, and the existing Adaptive cold-region suites. Combined repeated Off medians improved coast from 4.94 to 0.67 seconds, dog from 6.79 to 1.37, H from 10.75 to 1.76, and edited flower from 4.15 to 1.35 natively / 1.08 to 0.46 at Half. Targeted Speed float hashes remained exact. Photographic appearance awaits owner review.

The owner requested launch while the broad combined numerical sweep was still running. Its completed cases remain evidence, but the 144-case candidate sweep was stopped deliberately before completion to avoid competing GPU load. The accepted 144-case baseline is complete and unchanged. Resume the candidate sweep in a new capture after owner testing; do not report the partial sweep as complete release coverage.

## Preview enlargement margins

The owner selected a 10% linear display-enlargement margin for Quality and 20% for Speed. The main-preview resolver therefore retains Half through 55% native-pixel zoom in Quality and 60% in Speed, including display density. Native loupe detail, export and fixed diagnostic grids are unchanged. Independent Rust and Python behavioral expectations follow the new limits. Boundary checks cover the reported 52% view and both policies immediately above their Half-to-Native thresholds.

Independent review caught a boundary mismatch in the Python expectation: decimal limits are compared against Float32 geometry by the app, while Python had used Float64 arithmetic. The independent expectation now rounds geometry operations, density products and policy limits at Float32 boundaries. Regressions include captured non-integer image widths at both Half thresholds, alongside the 52% and just-above-threshold checks. This changes test precision, not runtime geometry.

The enlargement slice passed four targeted GUI policy tests, all 185 Python profiling tests (one skip), strict GUI Clippy, formatting and documentation checks. Largest-model review approved the Float32 expectation correction and boundary coverage. The review app was rebuilt for owner testing.
