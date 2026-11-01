---
summary: "Merrill Noise owns reference and optional compact speckle detection with adjustable threshold and native support."
kind: reference
status: accepted
implementation: conforming
---

# Merrill speckles

The Speckles control must move from Corrections into Noise for Merrill Color and Monochrome. Camera-record Bad pixels remains in Corrections. Noise-scoped recipes must copy the speckle enable state and settings. Corrections-scoped recipes must preserve them. Explicit Noise bypass must disable speckle processing while retaining the authored settings. Other camera families retain their existing correction scope.

Reference must remain the default and reproduce the existing detector at threshold 1. The threshold control multiplies camera-derived thresholds, with lower values detecting more outliers. The optional Compact method targets bright isolated points and small clusters using immutable native-layer ring neighborhoods. Sizes 3, 5 and 7 specify the largest native neighborhood diameter. Every smaller radius must also be checked. Bright clipped highlights require conservative handling based on unchanged native sensor samples. A calibrated value above nominal white must not by itself bypass detection, since white balance can amplify an unclipped defect beyond that value. The Compact guard protects original layer codes at or above 3900, matching the native layer filter’s conservative near-clipping boundary. Both methods share the existing calibrated native-layer boundary before color conversion and monochrome mixing.

Comparisons must include persistent sensor defects and unclassified speckles without assuming all bright points have the same cause. Review thresholds and sizes on the speaker, car, portrait and noisy leaf backgrounds. Real reflections, eyes, hair, fabric, edges and ordinary grain must be inspected alongside remaining outliers. A lower count of bright points alone does not establish improvement.

Threshold and size are independent of future per-camera defect calibration. Camera-specific sparse maps must follow [sensor defect calibration](./sensor-defect-calibration.md), including camera serial matching.
