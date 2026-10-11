---
summary: "Compressed DNG preserves editing latitude and embeds color-mode presentation, while TIFF bakes the selected rendering."
kind: reference
status: accepted
implementation: incomplete
---

# Output strategies

## DNG

- The application **must** produce compressed DNG output intended for further editing in Lightroom and Capture One.
- DNG processing **must** prioritize editing latitude, retaining a useful linear-space image with color and tone presentation embedded as profile or look data.
- DNG **may** bake calibration and image-content improvements such as detail recovery and highlight reconstruction into its samples.
- White balance **must** remain an editing choice in the DNG workflow.
- The DNG color-mode path **must** reformulate metadata-derived channel tone curves into a common curve with a compensating color matrix.

The objective is useful further editing and attractive results. The common-curve and matrix representation may approximate the source color mode. The fitting method remains an implementation choice to review with the owner.

## TIFF

- The application **must** produce TIFF output with the selected white balance, color, and tone processing baked into the image.
- TIFF **must** use its own rendering and packaging path within the shared graph engine.
- TIFF ColorDQ conditioning **must** be opt-in and support an explicit gain setting. Automatic noise-dependent gain selection is deferred.

## Conformance

A compressed DNG opens in Lightroom and Capture One for further exposure, white-balance, and tone editing. Review checks the embedded color-mode presentation and available editing latitude in each application. A TIFF reflects the selected baked rendering.
