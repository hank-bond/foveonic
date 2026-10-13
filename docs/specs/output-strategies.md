---
summary: "Compressed DNG preserves editing latitude and embeds color-mode presentation, while TIFF bakes the selected rendering."
kind: reference
status: accepted
implementation: incomplete
---

# Output strategies

## DNG

- The application **must** produce DNG output intended for further editing in Lightroom and Capture One, with compression enabled by default. The initial encoding **must** use unsigned 16-bit samples with lossless JPEG compression. DNG **must** be an output-only format for this application; regeneration uses the original X3F and its [editing sidecar](./editing-sidecars.md).
- DNG processing **must** prioritize editing latitude, retaining a useful linear-space image with color and tone presentation embedded as profile or look data.
- DNG export **must** rerun a modified processing recipe from the original X3F and selected editing settings, rather than serialize existing display buffers. The export recipe **must** omit native pixel-tone rendering and generate its selected appearance as embedded profile or look metadata.
- DNG **may** bake calibration and image-content improvements such as detail recovery and highlight reconstruction into its samples. These corrections **may** depend on the selected processing WB; later DNG WB edits need not recalculate them.
- Initial DNG sample encoding **must** clamp negative export samples to zero and use a common linear range scale across channels, with matching exposure compensation in metadata. It **must** publish `BlackLevel = 0` and `WhiteLevel = 65535`. Range allocation **should** follow the processed image's needs rather than bake a highlight shoulder. Signed-sample preservation and more advanced WB-headroom allocation are deferred refinements.
- White balance **must** remain an editing choice in the DNG workflow.
- The DNG color-mode path **must** reformulate metadata-derived channel tone curves into a common curve with a compensating color matrix.
- Embedded presentation **must** incorporate the selected camera mode and editor tone changes without baking that tone rendering into the linear samples. Composition **must** preserve the intended operation order and color spaces. Residual color look data **may** supplement the common curve and matrix. Receiving editors need not reproduce the application's slider values; their tone controls starting at neutral is expected.
- Embedded recipe metadata **must** document the selected edits and processing already baked into the samples. It **must not** be treated as recoverable editing state for this application.

The objective is useful further editing and attractive results, not exact reproduction across rendering engines. The common-curve and matrix representation may approximate the source color mode. The fitting method remains an implementation choice.

### Deferred encoding evaluation

JPEG XL compression and float16 sample storage **should** be explored after the initial uint16/lossless-JPEG slice. These are separate choices: compression codec and sample representation. Neither is required for the first export.

JPEG XL evaluation should compare lossless file sizes, encoding cost, dependencies, and target-reader support; using it for the main image requires DNG 1.7 or later. Float16 evaluation should compare shadow quantization, strongly lifted shadows, negative samples, highlight headroom, and reader compatibility. Lossless compression preserves the chosen encoded samples; conversion from the engine's float32 signal can still lose precision.

## TIFF

- The application **must** produce TIFF output with the selected white balance, color, and tone processing baked into the image.
- TIFF **must** use its own rendering and packaging path within the shared graph engine.
- TIFF ColorDQ conditioning **must** support an explicit gain override (0 Off). The opt-in reconstruction recipe **must** select native automatic processing-ISO gain capped by its noise-derived limit when no override is set. Without reconstruction, automatic selection **must** remain inactive.

## Conformance

The initial DNG stores uint16 samples with lossless JPEG compression; decompression reproduces those encoded samples exactly. It opens in Lightroom and Capture One for further exposure, white-balance, and tone editing. Review checks the embedded color-mode presentation, lifted shadows, retained highlight headroom, and available editing latitude in each application. A TIFF reflects the selected baked rendering.
