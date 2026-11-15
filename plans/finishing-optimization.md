---
summary: "Restore practical brightness-guide grids, establish repeatable comparisons and optimize measured finishing bottlenecks in reviewed checkpoints."
---

# Finishing latency optimization

Requirements: [processing quality](../docs/specs/processing-quality.md), [profiling](../docs/specs/profiling.md) and [reproducible inputs](../docs/specs/performance-diagnostics.md#reproducible-test-inputs). The owner authorizes the following sequence, with independent largest-model review and commits between safe slices. Keep all GPU measurements and compiler work serial on this host. Preserve source photographs and authored edits.

## Approved sequence

- [x] Restore the previous half-width, half-height brightness guides and gain anchors in both processing policies. Keep native photographic resolution, dense Quality scene estimation and policy-consistent section/Whole guidance. Remove unused dense-guide paths. Verify Speed parity, source-independent arithmetic, photographic section/Whole agreement and latency. Review with Astra, clean up and commit.
- [ ] Reprofile the resulting graph and pair with Astra to rank optimization opportunities by measured latency. Include fresh preparation and retained edits, since a reused node is not a measurement of its execution cost.
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
