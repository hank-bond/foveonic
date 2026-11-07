---
summary: "Move the sensor reduction boundary earlier while preserving original evidence, Full output and existing recipe ownership."
---

# Early sensor reduction

Requirements: [small previews](../docs/specs/small-previews.md) and [render preparation](../docs/specs/render-preparation.md).

Keep existing recipes, typed graphs, execution and cache owners. Metadata remains in original camera coordinates. Processing stages receive explicit working geometry and physical support instead of a fabricated smaller camera. No selected operation is disabled to meet a timing target.

## Sequence

- [x] Review recorder lifecycle, overflow metadata recovery and task-based work attribution before using traces.
- [x] Map the current pre-pool stage chains and preserve a Full float baseline before numerical changes.
- [x] Move Merrill row correction after pooling for Color and Monochrome. Preserve physical row-bank phase, scale polynomial support and verify Full bytes, working geometry and retained/fresh results.
- [ ] Move Merrill highlight recovery and speckle processing after basic calibration and mapped repairs. Preserve original clipping evidence independently of pooled sample values.
- [ ] Resolve native shield/row calibration separately from multiscale neighborhood processing and directional cleanup.
- [ ] Apply the same distinction to Quattro/Tiger mapped defects, phase calibration, neighborhood repairs and highlight recovery.
- [ ] Verify all supported families and selected controls through the same scale contract. Record remaining native work explicitly.
- [ ] Measure cached-source first preparation separately from retained edits and complete group delivery. Review photographic quality before release.

## Findings

The existing Merrill cut follows repair, highlight recovery, speckle filtering and row correction. Quattro similarly reduces after its repair/filter/recovery chains. These are pipeline boundaries, independently of scheduling. Several operation descriptions still carry native dimensions and support sizes, so relocating a graph edge alone would produce incorrect addressing or scale.

The first Merrill slice keeps the existing row-correction operation and selected polynomial degrees. Its description maps sensor geometry onto the working grid. Patch sizes follow physical support, and row banks retain their original sensor phase independently of padding. Color and Monochrome prepare the same description and execute it after their sensor reduction. Other pre-pool stages remain open work.

## First working-grid slice

Row correction passes its native GPU fixture and follows pooling in authored Color and Monochrome graphs at divisors one, two, four and eight. Source-free reset renders match fresh output at divisors two, four, eight, sixteen and sixty-four. A complete Full float image with reconstruction and row correction selected is bit-identical before and after this change.

Three alternating headless Wall trials with two repeated fresh renders per process measured a Thumb median of about 296 → 227 ms, excluding source opening and export. Full repeated fresh-render medians were about 667 and 659 ms. Runs used the same original, explicit reconstruction, WB/mode and 512×342 export size, with no competing app/compiler workload detected. These are engine measurements, not native application latency or retained-edit measurements. One cold sample is insufficient for a performance claim. The engine remains above the first-Thumb target and the remaining pre-pool filters/recovery are still open.

The compared Thumb samples have unchanged framing. On one hazy-coast photograph, tone-mapped RGB RMS change was about 0.00010, with a 0.00221 maximum on a nominal 0–1 range. This one-image comparison does not replace broader photographic review.
