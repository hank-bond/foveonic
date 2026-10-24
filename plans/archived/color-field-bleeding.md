---
summary: "Profile color-field parameters that may reduce yellow bleeding into the 0170 striped skirt without losing genuine highlight color."
status: archived
---

# Color-field bleeding on 0170

## Scope

The owner approved profiling parameters that could add useful processing flexibility, with priority on the strongly clipped yellow/white skirt in `_P2M0170`. The reference implementation screenshot shows the same yellow wash, so this is an opportunity to improve on the reference rather than a port defect. The owner also reports that the current agreement and boundary sliders have little visible effect. Preserve defaults and existing acceptance checks. Do not add a GUI control until a candidate demonstrates a clear photographic improvement and an understandable tradeoff. Do not commit or install.

Follow the [color-field requirements](../../docs/specs/color-field.md) and the [control sensitivity investigation](../color-field-controls.md). Source and sidecars remain immutable. Recovery remains in WGSL. Preview images are eight-bit review aids, not numerical goldens.

## Observations

- The skirt stripe period is roughly 8–9 native pixels. The default 8-pixel color grid therefore spans nearly one yellow/white stripe pair. A cell can average both stripe colors and donate that mixture into clipped white stripes.
- The reference implementation screenshot shows a similar yellow wash. Classic is whiter because its clipped-color neutralization does not reconstruct the same color evidence.
- Agreement and boundary slider extrema look essentially identical on the 0170 skirt crop. Their measured changes are concentrated in highlight speckles, not the broad stripe tint.
- Earlier full-frame comparisons understated the donation effect. Native highlight crops show that removing borrowed color reduces the strongest yellow wash, but changes surviving ratio evidence and genuine highlight color differently from explicit neutralization. Neighboring donor spill is not an established sole cause.
- Fine grid steps 2, 3, 4, 5 and 6 retain the tint. Smaller grids can reduce speckle, but have not demonstrated a clear photographic solution and cost more.

## Hypotheses

1. **Grid resolution.** Four-pixel cells separate the stripes better. Two or three pixels may help further. Six pixels may preserve more performance than four. Grid phase may also matter because the stripe period is near eight pixels.
2. **Donor purity.** Cells whose intact donors span both stripe colors produce a mixed donor. Rejecting high-variance donor cells should reduce the wash but may remove useful donors and create pale holes.
3. **Affected-to-affected connectivity.** The graph currently gates intact-to-damaged boundaries but lets two damaged cells connect freely. Adding a tone/ratio gate there may stop cross-stripe propagation.
4. **Neutral restraint.** Increasing the neutral prior for low-reliability cells may whiten ambiguous clipped fabric, but it also risks desaturating genuine yellow highlights.
5. **Donor-distance falloff.** Shorter or longer weighting changes how much color comes from near versus far cells. Because the nearest stripe is often the wrong color, this may not be monotonic.
6. **Source evidence.** The no-restoration source may already show yellow in the white stripes from optics or sensor crosstalk. If so, no color-field parameter can fully recover neutral white without an explicit neutralization choice.

## Experiments

- [x] Establish source evidence: 0170 with highlight restoration off, Classic, Global default and Local default using the saved recipe.
- [x] Compare fine grid steps 2, 3, 4, 5 and 6 at the native skirt crop.
- [x] Compare donor purity limits and affected-to-affected gates independently.
- [x] Compare neutral pull and distance falloff independently.
- [x] Isolate color data terms in the affected grid cells and final native fit while holding brightness reconstruction fixed.
- [x] Recheck restrained candidates on flowers, white knitwear, skin, clouds and the severe-clipping scene.
- [x] Summarize photographic tradeoffs. The owner clarified that per-image tuning need not improve every scene at one setting, then selected final-fit neutral restraint for a slider.
- [x] Agree cleanup scope and remove rejected shader investigation hooks. The [restraint plan](../color-field-restraint.md) tracks integration and acceptance. Defaults remain unchanged.

## Findings

### Parameter screening

Donor purity limits 0.01/0.05/0.20, affected-cell tone gates 0.25/0.50 EV, distance falloffs 64/1024 pixels and grid neutral multipliers 2/5/50/500 did not clearly remove the stripe wash at ordinary viewing size. Grid spacing 2/3/4/5/6 pixels changes speckle and donor availability rather than cleanly separating reconstructed yellow and white. The two-pixel grid also has fewer than the baseline minimum of eight donor samples per cell, so it cannot be interpreted as a pure resolution change.

The no-restoration render already has strong yellow/green clipping distortion. This does not establish optical crosstalk or prove that the distortion is trustworthy color evidence. It only rules out the color field as the sole origin of every visible color error.

### Final isolation round

The final round started from the frozen optimized implementation. All trial host and solver snapshots match that implementation. Brightness reconstruction, Local's 64 sweeps and convergence limits were held fixed. Changes were restricted to color evidence and application. Tests included:

- Removing the native color-fit data terms and retention step while preserving field sampling and its original affinity selection.
- Removing color data terms from affected grid equations while retaining graph geometry and boundary selection.
- Combining those two removals, and comparing with donation zero to distinguish borrowed color from neutral reconstruction.
- Weighting native color-fit evidence by 0.25, 4 and 16.
- Discarding ratio evidence at affected pixels, or discarding partially reliable layers instead of using continuous reliability.
- Increasing neutral restraint in the final native fit. Earlier independent trials covered 0.1/0.5/2/8 and a stronger reliability exponent.

Using only the sampled field at native application makes the yellow wash worse. The native ratio fit normally restrains this field color. Removing affected-grid data terms or borrowed color can whiten the skirt highlights, but adds pale yellow/peach patches to the magenta petals, breaks genuine yellow knit highlights into pale speckles and shifts clipped skin or clouds toward neutral. Increasing native evidence by 4 or 16 preserves more of those colors, but does not clearly solve the skirt wash. Stronger final neutral restraint has the same highlight-desaturation tradeoff.

These interventions do not prove a single physical cause. Field color itself is derived from layer-ratio evidence, including affected cells. The observed wash is therefore not adequately explained as only spatial donation from neighboring yellow stripes. Discarding potentially distorted ratios without replacing their lost color evidence also does not provide a reliable recovery rule.

### Checks and disposition

The final round recorded 93 completed renders and 92 preview exports with no logged render errors. Two tool timeouts remain explicit incomplete measurements, not passing samples. One strong-neutral trial produced no completed render. A cross-scene diagnostic batch stopped after five completed renders and four exports. Neither was retried. Other cross-scene candidates completed both Local and Global across five independent scenes.

Working source was restored byte-for-byte to its pre-round state. Preview images are eight-bit photographic review aids, not numerical goldens. No new default was selected. The initial screening incorrectly treated a cross-scene improvement as necessary for a slider. The owner clarified that losing genuine highlight color on one image is an acceptable tradeoff for reducing bleeding on another, then approved a shared final-fit color-restraint slider. The [restraint plan](../color-field-restraint.md) records that implementation and cleanup. Other experimental shader hooks are removed. Optimized Local/Global brightness and grid logic remain unchanged.

## Closure

This completed implementation or experiment is archived during release 0.3.0 preparation at the owner's request. Earlier scope, review and working-tree statements describe that historical slice. Recorded limitations and separately tracked follow-ups remain explicit.
