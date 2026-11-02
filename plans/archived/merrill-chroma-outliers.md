---
status: archived
summary: "Investigate green shadow pixels that intensify with Chroma cleanup before selecting a narrowly scoped correction."
---

# Merrill chroma outliers

The owner reported individual noisy green pixels becoming stronger as Chroma increases in a supplied DP1 Merrill portrait. The owner authorized investigation of near-black normalization and filtered-color reliability. No processing change is approved by this investigation.

A prior controlled comparison reproduces green-pixel amplification with Multiscale Rows + Columns and shadow neutralization off. Chroma selects a nonlinear blend toward fixed filtered color proportions, with weights 0.75, 0.9375 and one at choices two, three and four. The fixed base does not become more strongly filtered as this slider changes. At one observed pixel, linear sRGB green rises from approximately 18 to 34 while red and blue become more negative. This does not establish that the underlying sensor-layer normalization is singular.

- [x] Inspect native conditioned and filtered layers and their luminance denominators at the reported outliers.
- [x] Verify that the Chroma sweep retains identical upstream estimates.
- [x] Separate brightness restoration and ColorDQ from the color-proportion blend.
- [x] Record the actual cause, evidence limits and a minimal proposed next step before implementation.

Use the opt-in [chroma investigation](../../crates/foveonic/examples/chroma_investigation/AGENTS.md). It records small matched GPU sample tables and changes no photographic settings or defaults.


## Findings

The diagnostic samples 3,533 locations in the reported top-shadow region, including five identified outliers and their nearby pixels. Chroma two, three and four retain identical conditioned layers, filtered layers and luminance ratios within each Luma sweep. Fresh earlier RGB measurements reproduce the cached trace at the same outlier. Neither changing upstream filtering nor an unexpected cache transition explains the response.

Every sampled filtered layer triplet is positive. None reaches the 0.0001 luminance denominator floor. The smallest reference luminance is approximately 0.442. Normalizing both branches with the positive-signal helper would change none of these samples. The asymmetric normalization is therefore not the cause in this region. Negative reconstructed RGB does not establish negative sensor layers.

At oriented coordinate (539, 76), conditioned layers are approximately (34.354, 9.769, 23.309), while filtered layers are (0.698, 1.595, 0.895). Their weighted luminances are approximately 8.473 and 0.450. Luma two selects brightness 4.848, approximately 10.77 times the filtered reference luminance. Luma zero restores approximately 18.82 times that luminance. The reference floor is far below the denominator, so this is substantial scaling of a weak color estimate rather than division by an almost-zero numerical constant.

The filtered reference at its own brightness maps to approximately (-2.817, 3.839, -2.722) in linear sRGB. Restoring selected brightness at Chroma four produces approximately (-30.335, 41.343, -29.321) before ColorDQ. The less-filtered conditioned branch has a strong magenta error at this pixel. Chroma two partly cancels its error against the green reference. Increasing Chroma removes this cancellation. This also means a guard that only asks whether the filtered estimate has greater chroma than the input can miss the problem.

The sampled blend, brightness reconstruction and camera matrix predict all 30 targeted GPU RGB probes within approximately 0.000031 calibrated output code. ColorDQ reduces the example's green value from approximately 41.34 to 34.34 at Chroma four. Amplification is already present before that stage and with shadow neutralization disabled. Across the sampled region, upper-tail green variation decreases overall while some individual outliers intensify.

## Proposed next step

Assess filtered-color reliability before restoring brightness, considering weak reference signal, large restoration gain and inconsistent neighboring color. Preserve the selected brightness and well-supported colors. A stable local color estimate may be a safer fallback than simply retaining more of the less-filtered branch, which can itself contain opposite-hue noise. This proposal still needs a controlled photographic comparison and approval before implementation. Raising the denominator floor, clamping all negative RGB, globally increasing Luma cleanup or merely capping Chroma would not be a demonstrated solution.

The diagnostic uses recorded white balance, Standard mode, Multiscale Rows + Columns, default Directional Fine on both axes, neutral detail and shadow neutralization off. It does not reconstruct unshown screenshot edits. These results identify the mechanism in this crop, without proving general behavior for every shadow, camera or colored point source. Production processing and defaults remain unchanged.
