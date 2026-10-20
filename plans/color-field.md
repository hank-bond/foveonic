---
summary: "Implement GPU color-field highlight reconstruction, validate against the pinned reference, and compare quality/performance experiments using saved edits."
---

# GPU color-field highlight reconstruction

## Owner directive and scope

Work autonomously in the dedicated `experiment/color-field` worktree until the approved implementation and evaluation are exhaustively completed. Do not treat a partial shader prototype as completion. Implement a faithful, GPU-optimized N2 mode named **Color field**, then explore deeper algorithm-changing quality/performance variants. Keep reference defaults and preserve the current threshold-adjusted stock mode. Add useful, limited tuning controls focused on false color donation. Do not commit, install the app, publish results, or alter the owner's existing checkout. Reasonable supporting tool installation is approved. Stop only for genuine blockers or requirements that cannot safely be resolved within this scope.

Render comparisons using the owner's saved sidecars on the highlight corpus. Prioritize `_P2M0095` blotches, `_P2M0286` pink/red donation into white sweater stripes, and `_P2M0313` posterization. Preserve hard overexposure recovery even if it remains slower. Profile completed renders and stage costs, keep architecture clean where the selected method requires additional cross-stage evidence, and leave a toggleable visual report open in the browser for owner review.

## Inputs and boundaries

- Worktree starts at `ea4a44c`. The owner's main checkout has independent uncommitted export/UI work and remains untouched.
- Reference implementation: `hank-bond/x3fuse-core` `dev` at `2b0dd4432c370de8ac629a3576f26c85f2dc5391`. Do not modify its dirty checkout.
- Private corpus and a snapshot of 14 saved sidecars are cloned into `.local/highlight-color-field/`. Two supplemental regression inputs have no saved sidecar. Retain their explicitly identified default recipes separately.
- Current requirements: [highlight restoration](../docs/specs/highlight-restoration.md), [processing graphs](../docs/specs/processing-graphs.md), [editing sidecars](../docs/specs/editing-sidecars.md), [profiling](../docs/profiling.md).
- Numerical comparisons need identical normalized inputs and calibration. End-to-end comparisons with different calibration are photographic evidence, not numerical equivalence.

## Completion checklist

- [x] Create an isolated worktree and freeze the corpus/sidecar inputs.
- [x] Record accepted requirements and inspect source, calibration, repair provenance, GPU execution and GUI integration.
- [x] Extract a pinned, attributed CPU oracle and establish reference fixtures.
- [x] Implement reliability, anchors, native gradient solve, coarse color evidence, graph conditioning/distance, color solve, and application in shaders.
- [x] Integrate mode/settings, cache keys, inspection, CLI, GUI, TIFF and DNG technical recipes; leave stock arithmetic unchanged and disclose its unresolved cache regression.
- [x] Validate numerical accuracy, unchanged healthy samples, severe clipping, repaired donors, solver failures and retained ownership.
- [x] Attempt all saved-sidecar scenes with stock, faithful color-field and tuning/algorithm variants; preserve failed cases.
- [x] Profile completed first/warm renders and diagnostic stage costs. Record memory/iteration limits and numerical tradeoffs.
- [x] Inspect priority crops/full frames and evaluate deeper simplifications without silently substituting them for the faithful mode.
- [x] Correct CG checkpoint/restart behavior and validate compensated color iterates with repeated 0399 renders without relaxing bounds.
- [x] Offer Classic / Color Field: Local / Color Field: Global, with shared optional donor tuning and no experimental GUI rows.
- [ ] Recheck the separate strict stock-cache mismatch and complete owner photographic/control review.
- [x] Produce and open a toggleable browser report with full-frame/crop comparisons, recipes, explicit failures, limitations and timing tables.
- [x] Run workspace, GPU/fixture, strict Clippy, formatting, documentation/index and report checks. Leave changes uncommitted; do not imply all robustness checks passed.

## Discoveries and progress

Fourteen sidecars existed at the start and use Sunlight/Standard, per-image exposure and manually adjusted stock thresholds. The two supplemental frames are not part of the saved-edit comparisons. Inputs and sidecars were copied with copy-on-write filesystem cloning so later edits in the main checkout cannot change the experiment snapshot.

The GPU port uses compacted native equations, block-Jacobi-preconditioned conjugate gradients, device reductions, batched submissions, parallel graph labeling/peak conditioning, and distance propagation. Solving corrections to brightness anchors and the neutral color prior avoids cancellation in the equivalent absolute systems. Optional kernels compile lazily, so stock selection does not initialize them. Identical-input testing caught and corrected a boundary edge-index error.

The initial prototype aligned work to ActiveImageArea. Further inspection established that reference DNG recovery runs on the full sensor raster before cropping. The faithful default now preserves that full-raster geometry. Active-area-only work is an explicit independent experiment, including its grid-phase and boundary-neighbor changes. Earlier ROI photographs/timings remain preserved, separately labeled, with original records and frozen binaries.

Full-raster CPU comparisons added after crop validation caught a `0.00020399` maximum normalized error on 0313, just beyond the unchanged `2e-4` limit. Tightening the brightness bound from `1e-6` to `5e-7` resolved that accuracy miss while keeping the color bound `2e-6`. A separate `1e-6` color trial stalled on nine cases; it is preserved as a failed experiment, not silently used as a baseline. The failing log and binary are preserved; the acceptance threshold was not relaxed. Extending the oracle to all 16 rasters found one-bin reliability differences on a few sensor codes. Quantization now has an independent one-bin bound, positive half ties match the reference, and equation comparisons share the actual GPU quantized masks. The earlier 0170 `2.065e-4` end-to-end difference is recorded rather than misattributed to the solver. Matched-evidence equation errors passed the unchanged `2e-4` limit across the corpus. Seven synthetic cases, matched crops, cached/fresh results, retained-image byte identity, cancellation, invalid preconditioners, and actual DNG writing also pass.

Earlier ROI experiments keep their initial stopping bounds. The final round remeasures every current variant with the same solver revision and full-raster domain except for the explicitly named active-area trial. Earlier work stays preserved separately rather than being mislabeled as current defaults. The report discloses work domain, solver revision, source/sidecar hashes and binary hashes.

The broad opt-in processor fixture suite encountered the [known stock-cache mismatch class](./gpu-output-mismatch.md): 69 tests passed, one unchanged exact cache comparison failed. A single pristine-base isolation run passed, which does not establish a fix or cause. Strict stock checks remain untouched. All eight separate GUI GPU tests passed.

A later all-corpus oracle run exposed occasional 0399 color convergence failures even at `2e-6`. The expanded local rank-one ratio block contributed cancellation. Retaining its `q_rg * (u_r - u_g)` factorization, alongside already factored neighbor differences, avoids cancellation without changing equations, adding CPU recovery, increasing buffers, or relaxing convergence. All six dedicated GPU tests then passed, including all 16 full rasters. This was necessary numerical structure, not an alternate photographic algorithm. It did not establish a complete convergence fix; the repeated final sweep below failed again.

Broad final sweeps preserve current native-profile, full-resolution 8-bit previews for experimental methods, while retaining RGB16 TIFFs for saved-threshold Classic and faithful Color field. Tool-owned staging TIFFs are verified/consumed; all previously retained investigation artifacts remain untouched. This limits disk growth without changing the render recipe or timing intervals.

## Retained sweep and review evidence

The final frozen renderer attempted 208 scene/variant recipes with first + three warm + separate GPU diagnostic samples. It completed 1,027 render samples and recorded three errors, all on supplemental 0399: full Color field and Local gradient failed iteration 0; Coarse failed iteration 2. The latter has only one successful warm sample. No failing case was retried to obtain a clean report. No convergence bound was relaxed and no fallback method was introduced.

The dedicated six-test run passed beforehand, including all 16 full rasters with matched quantized evidence. Repeated final-sweep failures established a real numerical robustness blocker, despite that passing run. The approved solver follow-up below supersedes that diagnosis without discarding this failed sweep. Do not call the overall implementation complete or merge-ready while the separate stock-cache mismatch and owner review remain open.

Retained-sweep warm Classic/full/anchors/local-gradient medians in ms: 0095 `503/724/577/618`; 0286 `385/998/517/555`; 0313 `632/1360/781/899`. The 0010 sky full solve is about 2,776 ms; its 4px grid is 6,588 ms. Native gradient CG dominates much of the added processing cost. Uniform support saves comparatively little. The timed command reports roughly 2.92 GB peak memory footprint, not peak GPU residency.

Visual review finds substantial recovery of pink wrap shading and smoother skin transitions in 0313, but not all lost texture. In 0095, half/no donation creates conspicuous pale islands; lower donation is not monotonically better. No tested variant cleanly removes all 0286 stripe contamination; coarse/no-donation variants can worsen warm boundaries. These are owner-review observations, not objective quality goldens.

The private retained photographic report shows its frozen sweep revision, failed/partial cases without substitute pictures, recipes, metrics and findings. Earlier rounds, source snapshots, failed binaries/logs and original retained TIFFs remain preserved. Free space is roughly 1.5 GiB; only rebuildable local compilation caches were removed, after preserving the pristine-base test binary. Further large photographic sweeps need a storage decision. The approved solver follow-up uses no-export validation to avoid additional image storage. Retained evidence has not been deleted.

Ordinary workspace tests, strict Clippy, formatting, index generation/checks, and seven report-tool unit tests passed. Browser automation checked 16 scenes, 13 choices, 824 asset links, stable A/B bounds, keyboard/side-by-side behavior, native-profile links, explicit failed/partial cases, and zero JavaScript errors. The report was opened in Safari. The current GUI bundle was rebuilt but not installed or launched. Three copy-on-write playground originals/sidecars are available for later manual tuning without changing the frozen corpus.

## PR-region correction

The owner identified that the initial 640px detail crops did not reproduce the PR's test regions. Earlier detail-level photographic conclusions are provisional and must be rechecked against the corrected views. The owner approved locating the regions from the PR screenshots and rebuilding the report without another processing sweep.

Registered all 15 detail panels in the N2 example comment across 11 scenes, using multiscale normalized cross-correlation followed by visual checks. Scores were approximately 0.983–0.999. The 0313 face crop includes both adults and the baby's head, and its swaddle crop includes the full wrap/arm area. The 0286 crop includes the full white/red/yellow knit region rather than a smaller isolated stripe. Bounds remain estimates within a few pixels, not claims about unpublished original coordinates.

The checked-in region manifest records oriented native dimensions, bounds, labels and screenshot provenance. The report uses variable region counts, screenshot links and Fit / 100% viewing. Scenes without a PR detail shot stay full-frame-only. Reframing staged variants uses their existing lossless native-profile pixels and preserves their original metadata and earlier crop files. Processing, settings, raw measurements and that revision's solver behavior are unchanged.

Checks for this correction passed: 11 report-tool tests, ordinary workspace tests, strict Clippy, formatting and documentation/index checks. Browser validation covered all 15 registered regions and 195 per-variant native crop assets, variable view counts, full-frame-only scenes, screenshot links, 100% viewing, stable A/B bounds and failed-case behavior with no JavaScript errors. Comparison settings, timings, GPU profiles, error records and frozen-renderer context matched the prior report. Original crop files and staged metadata were preserved.

## Approved three-choice and solver follow-up

The owner selected Classic as the stock name, alongside Color Field: Local and Color Field: Global. The dropdown maps to the existing shared color-field operation and its local/global brightness configuration. Classic remains default. Local uses 64 damped sweeps. Experimental approach, area and strength controls are no longer ordinary GUI rows. Agreement and boundary restraint stay separate under optional tuning until their effects are isolated. Selecting either color-field preset resets investigation-only overrides, preserving Classic threshold, the enable state and shared donor tuning.

The GPU CG previously restarted every 32 iterations, unlike the reference implementation. Checkpoints now preserve the recursive residual, search direction and CG scalars unless recursive convergence needs true-residual replacement. A dedicated GPU test checks preserved conjugacy, false convergence, genuine convergence and retained breakdown flags. Restart removal passed the seven-test suite and all 16 identical-evidence rasters, but a predetermined repeated 0399 trial still failed Full at iteration 5, 16px-grid at iteration 4, and Local at iteration 0. Those failures remain preserved.

Compensated color iterates retain small updates in the two unused vector lanes without adding buffers. A GPU probe verifies those low components survive compilation. An ill-conditioned equation test demonstrated that ordinary additions in the ratio product still lose the correction. Explicit fused operations retain it. Native brightness arithmetic and all photographic equations remain unchanged. No stopping bound, iteration limit, oracle-error bound or fallback policy changed.

The resulting eight-test GPU suite passed, including all 16 full rasters, unchanged healthy samples, retained ownership, cache edits, cancellation, invalid preconditioners and DNG writing. Maximum full-raster normalized error was approximately `8.83e-5`, below the unchanged `2e-4` limit. A planned 63-render 0399 trial completed without failures: first + 19 warm + one synchronized diagnostic sample for each of Full, Local and 16px-grid. All earlier failed trials and their frozen measurement binaries remain separate. The original photographic report has not been re-rendered or relabeled as corrected-binary measurements. A separate all-corpus round completed all 192 planned Classic/Full/Local renders without errors, with first + two warm + separate GPU diagnostic samples per recipe. Both follow-up rounds used no-export output and retained their own frozen binaries and raw measurements.

The unchanged strict six-original stock-cache test passed once on the current candidate. That does not identify the cause or erase the earlier failure. All eight GUI GPU tests, ordinary workspace tests, strict Clippy, formatting, documentation/index checks and 11 report tests passed. Browser validation checked all 15 regions and 195 native crops with no JavaScript errors. Rebuilding the retained report changed only labels and findings. Every other report-data field and the raw measurement hash stayed unchanged. Before-update snapshots remain preserved. The GUI bundle was rebuilt without installation or launch.

The owner subsequently approved an autonomous [speed pass](./color-field-speed.md), with colon-separated public names and no internal algorithm code in method labels. That plan records measured retained changes, rejected candidates, controlled final comparisons and further checks.
