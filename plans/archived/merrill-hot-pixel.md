---
summary: "Repair isolated Merrill near-clipped defects while preserving Compact highlight protection with surrounding support."
status: archived
---

# Merrill hot pixel

Follows [Merrill speckles](../../docs/specs/merrill-speckles.md). The owner approved admitting extreme isolated near-clipped samples to Compact detection when dark neighbors and the other layers provide no highlight support.

- [x] Inspect the owner-provided original and locate the spot shown in the viewport.
- [x] Trace immutable original, native noise, calibration, ordered repair, highlight restoration, Compact speckles and final RGB.
- [x] Agree on a conservative distinction between isolated saturation in dark surroundings and genuine highlights before changing the guard.
- [x] Restrict the exception to original 3×3 samples below one eighth of the native near-clipping boundary, excluding the candidate layer itself. Keep ordinary calibrated threshold and size checks.
- [x] Verify the revision on the original, synthetic highlight cases and the seven repository photographs.
- [x] Complete workspace checks and package the review build.

## Findings

The initial trace found that a DP1 Merrill ISO100 exposure contains an isolated top-layer sample of 4045 at cropped image coordinate (3838, 1230), full-sensor coordinate (3902, 1266). Neighboring samples have a median near 79. The other layers have ordinary local noise and no comparable positive outlier. Native multiscale filtering retains that original near-clipped sample. Calibration raises it to approximately 6362 while neighboring calibrated values remain near 69. Ordered camera repair does not identify it. Before the correction, highlight restoration and Compact speckle removal left the sample unchanged. The resulting RGB pixel is bright magenta.

The former Compact guard bypassed its threshold and neighborhood tests when the unchanged original layer code reached 3900. Changing Compact threshold or size could not correct this sample. This is distinct from a prior issue where white balance alone raised an originally unclipped defect above calibrated white.

The evidence establishes the source of the visible spot in this exposure. Persistence across exposures has not been measured. A single clipped colored point can resemble a defect, so context-based repair must preserve legitimate lights and reflections conservatively.

## Validation

The revised Compact stage replaces the reported sample’s calibrated value of approximately 6362 with 70, consistent with its neighbors. Its final RGB crop no longer shows the magenta point or its immediate color-conversion halo. At the native boundary, five isolated near-clipped spikes change across the supplied exposure and seven repository photographs. The remaining 665,138 near-clipped samples remain unchanged. Three repairs occur in the supplied exposure, one in the parking-sign scene and one in the stroller scene. No near-clipped samples change in the leaf, paper, rocks, red-flower or still-life scenes. This establishes selectivity on these photographs, rather than a guarantee that every possible point light is distinguishable from a defect.

GPU tests cover isolated clipped spikes in each layer at 3900, 4045 and 4095, including calibrated values below nominal white. They preserve highlight support in another layer, an adjacent sample and a diagonal sample in another layer. Existing tests continue to cover unclipped defects amplified by calibration, lines, compact clusters, all three support sizes and immutable input buffers. The reference detector and node/recipe ordering are unchanged. Temporary measurement helpers stay outside tracked source.

Workspace tests, the Compact and reference GPU tests, strict all-target Clippy, formatting, generated-index checks and artwork tests pass. The optimized native app is packaged for owner review.
