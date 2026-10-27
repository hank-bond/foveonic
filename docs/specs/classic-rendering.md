---
summary: "Classic photographic rendering uses format-selected decoding and family-authored recipes with functional GPU operations and shared engine resources."
kind: reference
status: accepted
implementation: incomplete
---

# Classic rendering

## Requirements

- Add an end-to-end Classic photographic pipeline as an isolated design experiment, beginning with an acquired equal-resolution format-30 camera and extending through the selected reference stages to display, TIFF and editable color DNG.
- Select decoding by image encoding and rendering by camera family and processing version. A shared encoding does not imply a shared rendering pipeline.
- Keep camera-family selection in recipes and metadata preparation. Operations must express their functional contracts and must not branch on a camera-family enum.
- Prefer parameters and small functional modes when they capture the actual variation. Separate operations or injected behavior may be used when they make the implementation clearer. Avoid a general dependency-injection framework before demonstrated need.
- Share code where the equations, sample meaning and ordering agree. Preserve explicit signed code units and fixed-point transitions where the reference depends on them.
- Reuse graph validation, execution, caching, immutable GPU ownership, inspection, presentation and output machinery.
- Use shared destination conversion and encoding after Classic photographic rendering. Keep working primaries separate from the camera output preference. Preserve measured Classic tone, detail and saturation behavior through functional contracts.
- Preserve the existing Merrill behavior. Quattro integration and custom monochrome mixtures remain outside this experiment.
- Unsupported formats and processing branches must fail explicitly rather than silently substituting Merrill processing.
- Keep the experiment reviewable. Record numerical approximations and incomplete camera coverage with the validation evidence.

## Conformance

Render an acquired Classic original through the shared public engine and TIFF path. Verify stage equations against independent reference measurements, exercise named inspection and retained results, and run existing Merrill checks. Review the resulting operation contracts and recipe organization before broadening the design.

## Complete family coverage

Support ordinary color rendering for SD9, SD10, SD14, SD15, DP1, DP1s, DP1x, DP2, DP2s and DP2x, including their normal ISO ranges. Every model must support the shared display and TIFF paths and editable DNG with an embedded approximate appearance. Use model-specific metadata preparation and functional operations to express differences. Preserve the validated initial slice while expanding coverage.

Where native parallel processing is unstable, a deterministic implementation may choose a documented scheduling policy. Validate its output rather than claim universal bit parity. Custom monochrome remains deferred.

## Processing routes

Sensor decoding selects format6 or format30 independently of the camera rendering family. CAMF decoding supports the legacy XOR stream and the later compressed metadata. Sensor crop coordinates account for the kept area's origin, including the SD9 and SD10 vertical offset.

The recipe prepares cached or legacy camera calibration, shield-derived or stored drift, row correction, spatial gains and polynomial correction from metadata. SD9 and SD10 select red-layer detail before highlight preparation. Below ISO400, processing uses signed chroma smoothing, fixed-point camera color and logarithmic reconstruction. Large chroma support selects an additional recursive pass after reconstruction. ISO400 and above select range filtering, guided local means, residual diffusion and floating camera color. Both routes join at root detail and shared photographic or editable-DNG completion.

The Classic recipe shares stage selection with its inspection catalogue. Selectors must remain lazy so inspecting an earlier stage does not require unrelated later metadata. Preparation separates calibration, camera color, reconstruction and appearance. Keep source-dependent white-balance estimation and its cache owned by the engine.

Graph ports distinguish RGB codes awaiting matrix-gain restoration from codes ready for root/detail. Logarithmic reconstruction and the unfiltered gain operation consume the former and produce the latter. Direct monochrome, high-ISO reconstruction and the native disabled-FIR route produce ready codes. Graph validation must reject a missing or repeated restoration step. A ready-code contract describes the selected route's required scale rather than claiming a particular multiplication took place.

Operations receive numerical controls and functional modes. They do not identify camera models. Signed chroma smoothing shares its equations across calibrated sensor layers and linear RGB codes, while graph ports preserve the distinction between those sample domains.

Accumulated bulb frames, alternate processing-step flags, multiple FIR iterations and custom Classic monochrome remain outside ordinary color coverage. Unsupported controls must fail explicitly.

The shared reconstruction toggle enables or omits spatial reconstruction, including the appropriate chroma/FIR or range-filtering route. Required color conversion and signal gain must remain active with reconstruction disabled, for both photographic and DNG rendering. Its custom Merrill noise presets and overrides are rejected. The camera repair setting uses the Classic exposure-derived cutoff, with no manual Merrill badness mapping. Highlight streaming uses camera thresholds. Color-field controls, custom highlight thresholds, manual ColorDQ gain and disabling the intrinsic camera row-correction contract are rejected. Custom monochrome mixture and Merrill noise overrides remain unsupported. DNG uses the boundaries below.

Classic uses its own [family editing panel](./family-controls.md). Show supported controls and omit unsupported noise/detail sliders and overrides, highlight-method/threshold selection and intrinsic row correction. Keep reconstruction and highlight restoration as independent on/off controls. When camera metadata arrives, normalize incompatible saved settings through the processing-owned control contract while preserving compatible edits and those on/off choices.

Sharpen Off omits signed detail addition. Amount one selects the measured request's unit detail addition. Auto currently selects that same Classic unit base. Exposure, tone, saturation, output size and TIFF destination remain shared controls. Native-size output is the photographic parity boundary. Requested resizing uses the existing final linear-image resampler, rather than recreating the reference implementation's preview-stage resampling.

## Monochrome rendering

Classic must expose its reference-derived monochrome processing through the dedicated rendering selection. Retain the ordinary color WB preference, but use effective Monochrome calibration while this mode is active. Unit neutral and the camera's monochrome spatial gain must replace ordinary WB estimates. Where no dedicated monochrome illuminant is recorded, derive the rank-one camera mixture from the flash calibration. Preserve selected color-mode compensation, which may change both that mixture and its contrast. Custom Merrill monochrome mix and noise controls must be rejected.

The monochrome recipe must retain signed calibration, repair and despeckling. Low ISO retains the small chroma pass with doubled thresholds and direct gain-bearing color transformation. Omit larger chroma passes, FIR, red detail and chromatic highlight correction. High ISO retains range reconstruction with the selected monochrome matrix. Required color gain must remain consistent when reconstruction is disabled. Reuse Classic root/detail, tone and shared output finishing. Node contracts must remain functional and camera unaware.

Monochrome must support display, TIFF and editable DNG. The selected mixture and detail are baked into linear neutral DNG samples, including the selected color-mode effect. Exposure and tone remain embedded presentation metadata. Use equal stored RGB channels and an invertible neutral reader basis. Do not invert the singular monochrome camera matrix. The selected mixture cannot be changed back to color or another color mode through DNG metadata alone. Preserve the original X3F for those changes.

## Finishing boundary

Classic photographic rendering produces normalized linear ROMM RGB. Its prepared response retains the custom photographic toe and shoulder while decoding the reference profile's pure-power encoding. The recipe selects this encoding by its numerical transfer function. Nodes remain unaware of the camera family.

Display and TIFF use shared primary conversion and destination encoding. A wider-gamut export must not pass through a clipped sRGB intermediate. An unspecified TIFF destination uses the camera's recorded preference, with sRGB as the fallback when none is recorded. The standard ROMM export profile retains its standard shadow toe, independently of the encoding used by the photographic response.

Classic and Merrill tone generation and saturation mathematics remain distinct where their equations differ. Sharing output machinery does not require substituting Merrill's photographic appearance. Numerical profile and rounding differences from the reference implementation must remain measured and explicit.

Shared pre-curve sensitivity, HDR, Local Contrast and Universal sharpening follow the [photographic finishing contract](./photographic-finishing.md). The adapter retains the native combined reference operation when these edits are neutral.

## Editable color DNG

Classic color DNG must follow the shared [output requirements](./output-strategies.md). Export reruns the original source through supported technical processing. Exposure, tone, saturation and the selected color presentation belong in the embedded appearance rather than baked photographic pixels.

The DNG path should reuse sample encoding, profile packaging and output ownership. Family-specific preparation may supply the Classic technical base and appearance parameters. Nodes must retain functional contracts rather than branch on a camera-family enum.

The selected embedded appearance may approximate Classic photographic rendering. The owner accepts a close visual match rather than exact reference pixels. Numerical comparisons must state their color space, metric, settings and reader. A percentage without a defined metric is not an acceptance guarantee. Preserve useful exposure and white-balance editing latitude and report clipping, quantization and approximation limits.

Conformance requires exports from the selected SD15, DP2 and DP1s coverage, metadata and sample validation, checks that deferred appearance controls do not change technical samples, and comparison of reader-rendered appearance with the photographic path. Preserve Merrill export behavior. Lightroom and Capture One review must remain explicitly pending until performed.
