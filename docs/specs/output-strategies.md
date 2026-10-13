---
summary: "Compressed DNG preserves editing latitude and embeds color-mode presentation, while TIFF bakes the selected rendering."
kind: reference
status: accepted
implementation: incomplete
---

# Output strategies

## DNG

- The application **must** produce compressed DNG output intended for further editing in Lightroom and Capture One. DNG **must** be an output-only format for this application; regeneration uses the original X3F and its [editing sidecar](./editing-sidecars.md).
- DNG processing **must** prioritize editing latitude, retaining a useful linear-space image with color and tone presentation embedded as profile or look data.
- DNG **may** bake calibration and image-content improvements such as detail recovery and highlight reconstruction into its samples.
- White balance **must** remain an editing choice in the DNG workflow.
- The DNG color-mode path **must** reformulate metadata-derived channel tone curves into a common curve with a compensating color matrix.
- Embedded presentation **must** incorporate the selected camera mode and editor tone changes without baking that tone rendering into the linear samples. Receiving editors need not reproduce the application's slider values; their tone controls starting at neutral is expected.
- Embedded recipe metadata **must** document the selected edits and processing already baked into the samples. It **must not** be treated as recoverable editing state for this application.

The objective is useful further editing and attractive results. The common-curve and matrix representation may approximate the source color mode. The fitting method remains an implementation choice to review with the owner.

## TIFF

- The application **must** produce TIFF output with the selected white balance, color, and tone processing baked into the image.
- TIFF **must** use its own rendering and packaging path within the shared graph engine.
- TIFF ColorDQ conditioning **must** support an explicit gain override (0 Off). The opt-in reconstruction recipe **must** select native automatic processing-ISO gain capped by its noise-derived limit when no override is set. Without reconstruction, automatic selection **must** remain inactive.

## Conformance

A compressed DNG opens in Lightroom and Capture One for further exposure, white-balance, and tone editing. Review checks the embedded color-mode presentation and available editing latitude in each application. A TIFF reflects the selected baked rendering.
