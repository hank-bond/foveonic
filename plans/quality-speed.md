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
- [ ] Automatic resolution and two-choice GUI, policy-aware delivery/reuse, UI fixture migration. GUI subagent owns GUI files.
- [ ] Dense/compact brightness guides, gain geometry and complete Adaptive statistics. Node subagent owns finishing shader/node files.
- [ ] Consistent recipe guide preparation and dense/reduced Dehaze. Parent owns recipe and Dehaze work.
- [ ] Migrate profiling/behavioral scenarios and output schemas to independent policy and resolution.
- [ ] Independent review, regression/fidelity checks, targeted headed coverage and isolated latency/memory measurements.
- [ ] Final cleanup, commits, review build and owner checklist.

Only the parent runs compilers or GPU checks, serially through the shared release cache. Implementation agents avoid compiler/GPU work and share API decisions before changing callers. All photographs, approved baselines and unique inputs remain protected.

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
