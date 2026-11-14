---
summary: "Replace five preview modes with consistent Quality and Speed processing, dense intermediates and policy-aware reuse."
---

# Quality and Speed migration

Requirements: [processing quality](../docs/specs/processing-quality.md). Recovery checkpoint: `d77e6ac`. The owner authorizes all slices, tests and commits autonomously and will visually review at the end. Largest-model independent review remains required per slice. Preserve recovery commits rather than accumulating one unreviewable change.

## Agreed design

One engine `ProcessingQuality` enum accompanies the existing numeric working `Scale`. Settings keep it transient. Diagnostics preserve it. The GUI offers Quality and Speed and selects scale automatically for both. Requests, results and presentation compatibility carry the same policy. Bounds choose coverage only. Exports force Quality. No new cache owner, worker type or scheduling architecture is needed.

Quality covers all working pixels for optional guides and estimators. Speed retains the existing bounded approximations. The same algorithms consume explicit sampling parameters where possible. Dense brightness guidance preserves physical neighborhoods and uses full-grid gains. Adaptive's normalized regional scoring windows remain the same statistical model while dense Quality covers all contributing pixels. Dehaze must preserve photographic neighborhood size when removing its estimation-grid reduction.

## Inventory

| Operation | Current reduction | Quality | Speed / disposition |
| --- | --- | --- | --- |
| Adaptive input statistics | Thumb or half-Thumb RGB branch | Working-grid RGB | Existing profile target |
| Adaptive histogram and region statistics | 256 by 256 point samples | Complete working-grid coverage | Bounded samples |
| Brightness base | 2 by 2 cells and additional coarse coefficient grid | Dense cells and coefficients | Existing compact/coarse grids |
| Adaptive and Dynamic Range gains | Half-grid gain anchors | Full-grid anchors | Half-grid anchors |
| Regional application guides | Fast branch selected by region | Whole guide at the working scale | Whole guide at a Fast ceiling |
| Dehaze input | Family/mode-specific half-size branch | Working-grid branch in all families/modes | Shared Fast ceiling |
| Dehaze estimator | Pooling to a 512-pixel long edge | Dense grid with preserved physical radius | Bounded pooled grid |
| Color Editor guide and map | Current working grid | Existing full-working path | Existing path initially |
| Advanced Color, shadow neutralization, Local Contrast, sharpening | Working grid and algorithmic support | Existing algorithms | Existing algorithms |
| Early sensor pooling | Requested working scale | Retained | Retained |
| Camera noise/reconstruction, Color Field grid, texture/wavelet pyramids | Intrinsic sensor/model geometry | Retained | Retained |
| Camera statistics/calibration | Source-derived | Resolution independent | Resolution independent |

Color Editor's independent cleaned guide is a possible future Speed target, but it needs measured benefit and separate visual validation. Shared upstream RGB preparation may offer reuse without an approximation. Arbitrary denoise/detail bypasses, looser solver convergence and calibration shortcuts are not part of this migration.

## Slices and ownership

- [x] Commit current regional-guide and loupe work with the known refinement mismatch documented.
- [x] Engine processing policy, diagnostic snapshots and export invariants. Parent owns core request settings and recipe integration.
- [x] Automatic resolution and two-choice GUI, policy-aware delivery/reuse, UI fixture migration. GUI subagent owns GUI files.
- [x] Dense/compact brightness guides, gain geometry and complete Adaptive statistics. Node subagent owns finishing shader/node files.
- [x] Consistent recipe guide preparation and dense/reduced Dehaze. Parent owns recipe and Dehaze work.
- [x] Migrate profiling/behavioral scenarios and output schemas to independent policy and resolution.
- [x] Independent review, regression/fidelity checks, targeted headed coverage and isolated latency/memory measurements.
- [x] Final cleanup, commits, review build and owner checklist. Owner visual acceptance remains pending below.

Compiler and GPU checks run serially through the shared release cache. The parent delegates an exclusive measurement window when a node author needs to investigate a numerical failure. Implementation agents avoid compiler/GPU work and share API decisions before changing callers. All photographs, approved baselines and unique inputs remain protected.

## Validation and corrected assumptions

Today's Full output is not a universal Quality golden: hidden sampling reductions exist in that path. Preserve those captures as historical comparisons. Unaffected recipes must remain bit-exact. Changed dense operations need independent numerical checks and explicitly attributed photographic differences. No baseline is silently regenerated to bless candidate output.

Keep section/Whole and cached/fresh checks separate from Quality/Speed appearance comparisons. Test all supported camera families with native detail, Color/Monochrome, odd bounds, rotations, fractional mappings and padding. Verify guide construction once followed by reuse, and verify different quality cannot satisfy current native demand. Native detail and Whole continuation must keep policy through rapid controls and background completion.

## Owner review checklist

- [ ] Quality/Speed are the only preview choices and both automatically fit single and multi-photo layouts.
- [ ] Whites scrubbing at 200 percent on hazy coast no longer changes local contrast when a section is replaced by Whole completion, with Adaptive Natural and Disabled.
- [ ] Mode switching may change tone intentionally. Assess Adaptive Natural/Strong, Dynamic Range controls and Dehaze in both modes.
- [ ] Quality native detail preserves fine contrast. Inspect coast metal/label, clouds, flowers and deep shadows for artifacts.
- [ ] Speed loupe and viewport detail are responsive and remain aligned while panning. XL and 400 percent loupe remain available.
- [ ] Quality and Speed both work in Color and Monochrome across Merrill, Quattro, Classic and Tiger fixtures where supported.
- [ ] Before/After, inspection, multi-edit selection and controls, cache pressure and recipe persistence keep their existing behavior.
- [ ] Export from either preview mode produces the same Quality result.

Record measured visual tradeoffs, latency and memory below as implementation evidence arrives. Owner acceptance is pending until that final review.

## Evidence and lessons

Recovery commits: `d77e6ac` (prior regional/loupe work), `ac9f2ec` (policy foundation), `5a173da` (two-choice automatic UI), `f8419c9` (profiling axes).

The ordinary workspace suite passed after the initial integration: 564 engine, 530 GUI, 17 CLI, 16 xtask and documentation tests. The shared brightness-guide GPU suite passed all 12 cases, Dehaze passed all 12, and Adaptive passed all seven synthetic cases. Dense Adaptive statistics exposed GPU fast-math reassociation defeating compensated sums. Centered moments with a balanced binary sum tree now preserve the analytical large-window fixtures without changing Speed arithmetic. Three photographic Adaptive tests require an explicit source environment and are validated separately.

An inspection alias ahead of Adaptive analysis prevented reuse of an otherwise identical guide. The analysis image remains inspectable, while the guide now uses the direct RGB producer. Whole-to-section and moving-section tests must verify that reuse for both policies.

Profiling uses independent resolution and processing-policy axes. Nonfinite paired image samples fail the comparison rather than reporting misleading zero error. Changed dense photographic outputs remain unreviewed, never automatically promoted into goldens.

Native resource probes completed on the M1 Pro with 16 GB memory, with source decode excluded from render timing. Merrill Natural plus Whites completed in about 13.6 seconds with dense native guides and peaked at 3.33 GB process footprint. Quattro H completed in about 1.72 seconds in Speed and 24.11 seconds in Quality, with peaks of 5.85 GB and 6.85 GB respectively. These are fresh native graph runs, not cached interactive slider timings or automatic overview sizes. The large native Quality cost is an explicit fidelity tradeoff to review. The Speed run already peaked at 5.85 GB; the Quality run peaked at 6.85 GB. These aggregate measurements do not attribute that difference to individual operations. Both completed without OOM. Automatic overviews use smaller working grids.

The first Quattro Speed probe caught a family normalizer reconstructing settings without the new transient policy. It now preserves processing quality beside working scale and region, with an exact-preservation regression test for both policies. This demonstrates why arithmetic unit checks alone are insufficient and the camera/render workflow matrix remains required.

The complete unchanged-operation matrix passed 80 comparisons against pre-migration float hashes and geometry: three Classic, three Merrill, three Quattro and one Tiger fixture, four resolutions, both policies. This does not cover intentionally changed active guide output. Active-guide paired comparisons remain descriptive and unreviewed.

Photographic section/Whole tests passed on Merrill and Quattro for Color and Monochrome, Natural and Disabled, and both policies, including Whites, cold sections, panning, Whole continuation and retained earlier images. Whole-first and moving-section Dynamic Range guide reuse passed both policies. Dehaze whole/region/cache checks passed Merrill and Quattro, and Auto/manual swatch equivalence passed Classic in both modes and renderings. Adaptive inspection replay passed both policies across all exercised working grids. TIFF and DNG exports from either policy matched independent Quality files on Merrill and Quattro.

Final source review found idle preparation still restoring default Quality settings. Its existing configuration now carries workspace processing quality and uses the existing config replacement/invalidation mechanism. Shared sensor checkpoints keep their existing admission. The scenario driver now uses the same engine policy enum directly rather than a duplicate two-value adapter.

Headed validation exposed fixed-resolution assumptions in the old Smart/Tile tests. Their independent oracle now checks observed pixel density, policy, authored edits, native geometry and sufficient detail coverage. Explicit native zoom, unchanged pan and reopened-photo identity checks remain required. Tests must also reject recomputation when a completed finer image satisfies relaxed demand.

A real trace identified that last reuse defect: changing requested scale from 4 to 8 allocated a new request identity and dropped the delivered snapshot before the existing presentation matcher could reuse it. The completed fix preserves exact in-flight work matching while allowing presentation to evaluate the retained immutable snapshot. A real GPU regression verifies texture reuse without photographic dispatch and rejects changed policy, edits and source identity. This is an existing-owner handoff correction, not a cache or scheduling redesign.

The same trace recorded seven initial decoded-cache reads: four acquisitions and three later 8-to-4 promotions after photographic layout resolved. All were foreground; warm edits had zero reads. Initial acquisition counts are now explicitly diagnostic across automatic-resolution setup, with primed-cache original reads still forbidden. Settled warm-edit and reuse checks remain strict. The original failed capture remains failed under its original declaration. Improving initial request sizing is a separate future latency opportunity.

## Memory correction before further app review

The owner stopped the four-photo headed transition run after observing about 11 GB application memory and macOS pressure on a 16 GB machine. That run is interrupted, not passed. All app tests were stopped. The retained-cache setting is not a process-memory ceiling: required images, active inputs, node temporaries and driver storage can exceed it.

The existing coast graph records 772 MB of unused scratch before its first guide and 965 MB before ToneMask. Interactive execution already completes each graph operation, so cross-photo overlap of complete guide workspaces is not established. Each dense guide allocates five 16-byte moment/base buffers plus its 4-byte output, 84 bytes per working pixel, rather than allocating that set per dispatch.

The corrective slice preserves pixel arithmetic: trim completed scratch spares to 128 MiB and one per exact size after existing waits and consumer retirement, and use one scalar in-place guide accumulator/output instead of two vec4 base buffers. Guide workspace becomes 52 bytes per working pixel. Queue reuse before completion remains unchanged, and no waits are added. These reductions do not themselves prove an overall memory ceiling.

Before another app run, tests must use an explicit physical-footprint guard and progress from small cases. The optional macOS guard samples OS-accounted physical footprint rather than RSS, records observations and kills only its owned child above the requested threshold. It is a sampled stop threshold, not a guaranteed peak cap or complete accounting of all driver/graphics allocations. The same guard must cover fresh replay and render-matrix children when selected. The frozen 32 active-case hashes and geometry remain the exact parity gate for the allocation changes.

## Deferred follow-up

After closing the Quality/Speed migration, review data structures, element types and resource lifetimes for further memory reductions, especially peak memory. Keep that broader pass separate from this migration. The owner explicitly prioritizes finishing the quality switch now; do not expand this slice into a general memory redesign.

Memory correction checks passed: all 32 active-guide case hashes and geometry remain exact, the 12 shared-guide GPU cases and two small-buffer scratch GPU cases passed, and workspace unit/strict Clippy checks passed. The guarded native Quattro H Natural-plus-Whites render completed under the 4 GiB sampled threshold and retained its previous output digest. Its largest sampled physical footprint was about 3.57 GB, compared with the earlier 6.85 GB process-peak observation; sampling and process-peak reporting are different measurements.

The corrected four-photo H transition run reached the explicit 4 GiB threshold during setup and was stopped by the guard (about 4.48 GB sampled), not by macOS pressure. It remains an aborted run. Per the owner's direction, further general memory/data-type changes are deferred. Final functional validation used a four-photo mixed Classic/Merrill/Quattro fixture subset under a 5 GiB guard. Native H output remains separately covered.

The guarded single-photo zoom/pan/crop, retained-texture and neighbor-readiness smoke suite passed. Its largest sampled physical footprint was about 4.76 GiB under the 5 GiB threshold. This is behavioral evidence rather than a latency or general peak-memory guarantee.

## Closeout

The final mixed-camera native journey passed Quality/Speed switches, relative group edits, native zoom, tile selection and pan, Crop/Rotate, inspection, Before/After, loupe, cached motion and restoration of all four photographs. All four final retained images matched independent fresh replays bit-for-bit, including geometry. The largest sampled app footprint was about 4.65 GiB under the 5 GiB guard. These bounded results do not establish an overall memory ceiling or solve the larger four-photo H case.

Automatic resolution exposed another obsolete harness assumption: a native Whole image already owned by the main photograph can satisfy the loupe without creating a separate detail slot. The test now checks the image actually selected for Whole coverage, preserving independent source, calibration, policy, settings, footprint and reuse assertions. Regional routes still require their detail slot and bounds. The independent review approved this correction and the other closeout changes.

The final GUI suite passed 533 ordinary tests with 52 opt-in tests ignored. Python profiling tools passed 170 tests with one skip, and artwork tools passed five. The earlier full-workspace checks, GPU arithmetic suites, 80 unaffected-output comparisons, 32 exact allocation-change comparisons and policy-independent exports remain separate evidence. Formatting, strict Clippy and generated documentation checks complete the review build. The app is left closed after testing.

The implementation is ready for the owner's visual checklist. Keep the specification incomplete until that acceptance is recorded. Further data-structure, element-type and resource-lifetime memory work belongs to the deferred follow-up rather than this migration.
