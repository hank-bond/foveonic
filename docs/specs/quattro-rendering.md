---
summary: "Quattro recipes share numerical reconstruction and finishing while preserving camera-specific calibration."
kind: reference
status: accepted
implementation: conforming
---

# Quattro rendering

## Scope

Support ordinary single-frame full-resolution DP0, DP1, DP2, DP3 and sd Quattro color and monochrome rendering. Preserve the independently checked DP2 baseline while extending camera-specific preparation. An unedited file uses saved white balance, color mode and supported adjustments from its X3F header. Missing adjustments use the reference file-loader defaults: chroma and luminance step two, Smooth/Crisp one half, sharpness zero and highlight recovery one. Camera sensor normalization and capture ISO have separate roles.

Support camera WB presets, all demonstrated camera color modes, exposure from -2 to +2 EV, contrast, shadows, highlights, saturation, chroma- and luminance-noise steps zero through four and the native seven-position Smooth/Crisp control and signed sharpness from -2 to +2. Tone controls must use the selected mode’s demonstrated response. Saturation uses shared photographic finishing. Selected WB changes Stage2 gains, color matrices and shading while preserving fixed Stage1 daylight gains, recorded WB and factory calibration choices. Capture-only loading must not replace a missing selection with Sunlight. Resolve the recorded preset when its camera catalogue is available, while preserving explicit or saved WB edits. Local contrast must select the demonstrated low- or high-ISO route.

Support the dedicated monochrome reconstruction and neutral monochrome response. Banding adjustments, reduced-resolution input and SFD remain separate work. Advanced tone curves and spatial Retinex/Fill Light are deferred. The sd Quattro H follows the separate [Tiger contract](./tiger-rendering.md). Editable DNG follows the accepted canonical-Standard approximation and unsigned-code limitations in the [output requirements](./output-strategies.md), with integration tracked in the [DNG plan](../../plans/archived/quattro-dng.md).

Color and monochrome must retain independent Smooth/Crisp selections. Adjusting color chroma noise must not replace a saved monochrome detail default. Source-derived sharpness must resolve for the selected rendering mode. Fresh GUI settings must receive file defaults before explicit launch choices or edits. Existing sidecars remain explicit editing state.

The GUI must present the supported rendering choices honestly. Each camera retains its available inspection outputs independently of later preparation. Unsupported photographic adjustments must fail explicitly rather than silently selecting another recipe. The default recipe must use the bundled calibration maps under the [input specification's resource contract](./quattro-input.md). Rendering must not require an installed reference application or environment setup. Explicit calibration-directory overrides retain the same immutable resource contract.

The optional [Fast preview](./quattro-fast-preview.md) derives a reduced working image from supported HIGH input. Its separate workspace policy preserves the Full recipe for exports. Native LOW input remains outside this scope.

## Composition

Camera selection and metadata interpretation belong to family recipes. Numerical operations receive functional controls and immutable tables. Their switches must describe numerical behavior rather than camera identity. Share an operation only when its units, arithmetic, rounding and borders agree.

Preprocessing must include sparse-pixel calibration, factory RGB and blue repairs, despeckling, clusters and mandatory highlight corrections. Retain separate RGB and high-resolution blue histories. Keep sparse-repair flags, factory-repair flags and the photographic highlight mask distinct. Recorded WB, factory Auto calibration and selected rendering WB have different roles and must remain separate preparation choices. The chroma slider selects the early RGB filter strength. The luminance slider must leave that filter unchanged. Blue filtering must follow outlier repair, preserve the original blue guide and use the filtered blue only for restoration. Smooth/Crisp selects the corresponding residual-restoration power, local-contrast levels and proportional-sharpening levels together. Internal residual multipliers must not change its public meaning.

Color Stage1 must retain its source after the first white-balance round trip, in addition to filtered RGB. Monochrome uses its demonstrated channel-noise sequence and retains the original source. Monochrome bypasses the color-response transform while retaining highlight neutralization, chroma outlier protection and sector correction. Stage2 and fusion must consume the correctly shaded versions of those histories. Buffer reuse in the reference implementation does not authorize changing retained graph values or reading allocation padding.

Fusion produces unsigned integral RGB codes and signed detail, represented exactly in Float32 storage. Packed Lab codes used by local contrast need their own internal signal contract. Finished photographic response must produce linear RGB in declared primaries so shared display, output-space conversion, transfer encoding and TIFF writing can be reused.

The native active crop and orientation must be preserved. Define deterministic physical-edge behavior where the reference implementation reads unowned storage. Document its photographic extent and measured differences. Keep the scope of numerical claims tied to completed native boundaries.

Numerical cropping and resampling precede tone response in the reference route. The raw recorded right/bottom fields and the nominal output dimensions have different endpoint conventions. Keep that operation explicit. An image whose view is already baked must not be cropped or rotated again by presentation or export. Retained physical intermediates keep their original view independently of the final image.

## Validation

Check numerical operations against independent bounded fixtures and complete original-image captures. Check the connected graph as well as individual operations, including masks and retained histories. Export a native-size TIFF through shared finishing. Preserve existing Classic and Merrill outputs.

Track exact comparisons, measured approximations and outstanding variants in the [integration plan](../../plans/archived/quattro-integration.md).
