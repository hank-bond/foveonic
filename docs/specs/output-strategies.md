---
summary: "DNG preserves editing latitude and embeds color-mode presentation, while TIFF bakes the selected rendering."
kind: reference
status: accepted
implementation: incomplete
---

# Output strategies

## DNG

- The application **must** produce DNG output intended for further editing in Lightroom and Capture One. The initial implementation **must** use uncompressed unsigned 16-bit samples; compression is evaluated after this path works. DNG **must** be an output-only format for this application; regeneration uses the original X3F and its [editing sidecar](./editing-sidecars.md).
- DNG processing **must** prioritize editing latitude, retaining a useful linear-space image with color and tone presentation embedded as profile or look data.
- DNG export **must** rerun a modified processing recipe from the original X3F and selected editing settings, rather than serialize existing display buffers. The export recipe **must** omit native pixel-tone rendering and generate its selected appearance as embedded profile or look metadata.
- DNG for Classic, Merrill and Quattro, including Tiger, **must** bake active [color sensitivity, HDR, Local Contrast and Universal sharpening](./photographic-finishing.md) into linear samples in both Color and Monochrome, using the same combined pre-curve brightness gain as native photographic rendering. Chromatic [Color Editor](./color-editor.md) edits use inverse-profile-basis baking of the selected pre-tone result. Retain the original X3F and sidecar to change these adjustments. Neutral or bypassed edits must leave the established sample recipe unchanged. Camera appearance and global tone presentation remain in metadata.
- DNG **may** bake calibration and image-content improvements such as detail recovery and highlight reconstruction into its samples. These corrections **may** depend on the selected processing WB; later DNG WB edits need not recalculate them.
- Initial DNG sample encoding **must** clamp negative export samples to zero and use a common linear range scale across channels, with matching exposure compensation in metadata. It **must** publish `BlackLevel = 0` and `WhiteLevel = 65535`. Range allocation **should** follow the processed image's needs rather than bake a highlight shoulder. Signed-sample preservation and more advanced WB-headroom allocation are deferred refinements.
- White balance **must** remain an editing choice in the DNG workflow. For baked monochrome, later neutral/tint edits cannot reconstruct the original sensor layers or change their baked mix.
- [Merrill monochrome DNG](./monochrome-rendering.md) **must** bake layer mixing and technical mono reconstruction into linear neutral samples, keeping exposure and mono tone in presentation metadata. Equal RGB channels are permitted for interoperability. Selected color mode and saturation are inactive.
- [Classic monochrome DNG](./classic-rendering.md#monochrome-rendering) **must** bake its selected color-mode mixture into neutral samples and retain the Classic tone response in presentation metadata. Ordinary color WB is inactive.
- Quattro DNG **must** support ordinary full-resolution DP0/DP1/DP2/DP3 and sd Quattro color and monochrome, plus the full-resolution [Tiger route](./tiger-rendering.md). Reduced-resolution and SFD inputs remain deferred. It **must** rerun a canonical Standard technical recipe and interpret its reconstructed RGB codes using a linear scale of `1/16384`, omitting the photographic response from samples. The selected mode, exposure, tone and saturation **must** remain in one embedded appearance. Selected-mode and spatial-processing differences **may** be approximated under the same photographic-usefulness policy accepted for Classic DNG. This export does not claim exact reference rendering or a calibrated sensor-linear reconstruction.
- Quattro monochrome DNG **must** bake the selected recorded or edited channel mixture and technical reconstruction into neutral samples. Its mono exposure and tone **must** remain in presentation metadata. Regeneration from X3F is required to change the baked mixture.
- Quattro DNG **must** document that its technical source has already passed through unsigned 16-bit reconstruction. The shared export range policy preserves available headroom but cannot recover negative values, clipped highlights or detail already lost at that source boundary.
- The DNG color-mode path **must** reformulate metadata-derived channel tone curves into a common curve with a compensating color matrix.
- Each DNG **must** carry one selected presentation, not multiple selectable embedded looks. Choosing another Foveonic mode requires regenerating the export.
- Embedded presentation **must** incorporate the selected camera mode and non-baked RGB edits, including exposure, tone, and saturation, without flattening them into the linear samples. RGB gains or color-balance edits, when supported, **must** follow the same boundary unless needed for baked technical corrections. Composition **must** preserve the intended operation order and color spaces. Residual color look data **may** supplement the common curve and matrix. Receiving editors need not reproduce the application's slider values; their tone controls starting at neutral is expected.
- Embedded recipe metadata **must** document the selected edits and processing already baked into the samples. It **must not** be treated as recoverable editing state for this application.
- DNG **must** preserve available capture details in standard EXIF tags across supported camera families, including capture date, shutter speed, aperture, ISO, focal length, exposure compensation, exposure/metering modes, and camera/lens identifiers. Export **must** use the loaded source metadata without rereading the original. Dates retain the camera's recorded wall clock without guessing a timezone. Missing or malformed optional fields are omitted. EXIF dimensions and orientation describe the already-oriented export.

The objective is useful further editing and attractive results, not exact reproduction across rendering engines. The common-curve and matrix representation may approximate the source color mode. The fitting method remains an implementation choice.

### Deferred encoding evaluation

Lossless JPEG and JPEG XL compression **should** be compared after the initial uncompressed uint16 slice. No compression library or codec is selected yet. Float16 storage is a separate, deferred sample-representation choice; neither compression nor float16 is required for the first export.

Compression evaluation should compare lossless file sizes, encoding cost, dependencies, and target-reader support; using JPEG XL for the main image requires DNG 1.7 or later. Float16 evaluation should compare shadow quantization, strongly lifted shadows, negative samples, highlight headroom, and reader compatibility. Lossless compression preserves the chosen encoded samples; conversion from the engine's float32 signal can still lose precision.

## TIFF

- The application **must** produce TIFF output with the selected white balance, color, and tone processing baked into the image.
- TIFF **must** use its own rendering and packaging path within the shared graph engine.
- TIFF output resizing **must** default to the reference photographic 4×4 phase-kernel approach for explicit output dimensions, replacing bilinear export resizing. Exact dimensions **may** change aspect ratio. Native-size exports **must** retain their existing crop/orientation behavior. Preview sampling and the reconstruction resampler **must** remain independent.
- Photographic resizing **must** run on the GPU and retain floating signed samples and headroom until destination transfer/encoding. Kernel coefficients and coordinate mapping **may** be prepared on the host. Sampling at physical storage edges **must** stay within owned storage without introducing diagnostic marker colors.
- TIFF **must** offer sRGB, Adobe RGB, and ProPhoto RGB destination spaces with matching embedded ICC profiles. The default **must** retain the camera's stored working space. Conversion **must** transform tone-mapped linear RGB before destination transfer and clipping, without changing the working image, preview, or DNG samples. ProPhoto transfer **must** retain its linear shadow toe.
- GUI and CLI **must** share TIFF destination conversion and packaging. The GUI selector **must** appear in the mode-specific Processing settings dialog, with a remembered destination folder and immediate Process action, following the [import workflow](./import-workflow.md). These GUI output preferences **must** override any legacy sidecar output-space selection without changing the saved photographic recipe. CLI settings and legacy sidecars retain the shared TIFF space representation. RGB16 remains the selected TIFF depth.
- Color TIFF ColorDQ conditioning **must** support an explicit gain override (0 Off). The opt-in reconstruction recipe **must** select native automatic processing-ISO gain capped by its noise-derived limit when no override is set. Without reconstruction, automatic selection **must** remain inactive.

## Conformance

The initial DNG stores uncompressed uint16 samples; independent decoding reproduces those encoded samples exactly. A subsequent lossless-compression path must preserve the same encoded samples. It opens in Lightroom and Capture One for further exposure, white-balance, and tone editing. Review checks the embedded color-mode presentation, lifted shadows, retained highlight headroom, and available editing latitude in each application. A TIFF reflects the selected baked rendering.
