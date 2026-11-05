---
summary: "Merrill Noise owns reference and Compact speckle detection with shared three-layer Compact repair."
kind: reference
status: accepted
implementation: conforming
---

# Merrill speckles

The Speckles control must move from Corrections into Noise for Merrill Color and Monochrome. Camera-record Bad pixels remains in Corrections. Noise-scoped recipes must copy the speckle enable state and settings. Corrections-scoped recipes must preserve them. Explicit Noise bypass must disable speckle processing while retaining the authored settings. Other camera families retain their existing correction scope.

Reference must remain the default and reproduce the existing detector at threshold 1. The threshold control multiplies camera-derived thresholds, with lower values detecting more outliers. The Compact threshold slider must have a lower limit of 0.2. The optional Compact method targets bright isolated points and small clusters using immutable native-layer ring neighborhoods. Sizes 3, 5 and 7 specify the largest native neighborhood diameter. Every smaller radius must also be checked.

Compact must make one repair decision per pixel. An outlier in any layer may trigger repair, including a defect confined to one layer. Repair must replace all three layers together using the same donor pixels and weights at one shared radius. Layer-specific thresholds must not leave a partially repaired white speckle. Protected highlight evidence in any layer must preserve the whole pixel.

Bright clipped highlights require conservative handling based on unchanged native sensor samples. A calibrated value above nominal white must not by itself bypass detection, since white balance can amplify an unclipped defect beyond that value. The Compact guard uses original layer codes at or above 3900, matching the native layer filter’s near-clipping boundary. Such a sample may enter normal outlier detection only when both other layers at that pixel and all layers in its immediate eight neighbors are below one eighth of that boundary. This narrow exception permits repair of isolated near-clipped spikes in dark surroundings. Any surrounding or cross-layer support retains highlight protection. White-balance-adjusted values must not determine this support. A real isolated colored light can remain ambiguous in a single exposure. Both methods share the existing calibrated native-layer boundary before color conversion and monochrome mixing.

Comparisons must include persistent sensor defects and unclassified speckles without assuming all bright points have the same cause. Review thresholds and sizes on the speaker, car, portrait and noisy leaf backgrounds. Real reflections, eyes, hair, fabric, edges and ordinary grain must be inspected alongside remaining outliers. A lower count of bright points alone does not establish improvement.

Threshold and size are independent of future per-camera defect calibration. Camera-specific sparse maps must follow [sensor defect calibration](./sensor-defect-calibration.md), including camera serial matching.
