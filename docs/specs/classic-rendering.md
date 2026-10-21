---
summary: "Classic photographic rendering uses format-selected decoding and family-authored recipes with functional GPU operations and shared engine resources."
kind: reference
status: accepted
implementation: conforming
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
- Preserve the existing Merrill behavior. Quattro integration and Classic monochrome behavior remain outside this experiment.
- Unsupported formats and processing branches must fail explicitly rather than silently substituting Merrill processing.
- Keep the experiment reviewable. Record numerical approximations and incomplete camera coverage with the validation evidence.

## Conformance

Render an acquired Classic original through the shared public engine and TIFF path. Verify stage equations against independent reference measurements, exercise named inspection and retained results, and run existing Merrill checks. Review the resulting operation contracts and recipe organization before broadening the design.

## Selected experimental coverage

The validated entry slice covers equal-resolution format-30 SD15, DP2 and DP1s originals below ISO 400 with cached camera matrices and the supported zero-drift calibration route. Sunlight and Standard are the measured baseline. Metadata-derived camera color modes and white-balance presets use their file catalogues. Earlier interleaved encodings, inline-float DP1 rendering, nonzero polynomial/drift branches, accumulated bulb frames, red sharpening, large chroma blur, multiple FIR iterations and high-ISO range reconstruction remain explicit errors.

The shared reconstruction toggle enables or omits small/box chroma smoothing and logarithmic FIR reconstruction. Its custom Merrill noise presets and overrides are rejected. The camera repair setting uses the Classic exposure-derived cutoff, with no manual Merrill badness mapping. Highlight streaming uses camera thresholds. Color-field controls, custom highlight thresholds, manual ColorDQ gain and disabling the intrinsic camera row-correction contract are rejected. Classic monochrome remains unsupported. Color DNG uses the boundaries below.

Sharpen Off omits signed detail addition. Amount one selects the measured request's unit detail addition. Auto currently selects that same Classic unit base. Exposure, tone, saturation, output size and TIFF destination remain shared controls. Native-size output is the photographic parity boundary. Requested resizing uses the existing final linear-image resampler, rather than recreating the reference implementation's preview-stage resampling.

## Finishing boundary

Classic photographic rendering produces normalized linear ROMM RGB. Its prepared response retains the custom photographic toe and shoulder while decoding the reference profile's pure-power encoding. The recipe selects this encoding by its numerical transfer function. Nodes remain unaware of the camera family.

Display and TIFF use shared primary conversion and destination encoding. A wider-gamut export must not pass through a clipped sRGB intermediate. An unspecified TIFF destination uses the camera's recorded preference, with sRGB as the fallback when none is recorded. The standard ROMM export profile retains its standard shadow toe, independently of the encoding used by the photographic response.

Classic and Merrill tone generation and saturation mathematics remain distinct where their equations differ. Sharing output machinery does not require substituting Merrill's photographic appearance. Numerical profile and rounding differences from the reference implementation must remain measured and explicit.

## Editable color DNG

Classic color DNG must follow the shared [output requirements](./output-strategies.md). Export reruns the original source through supported technical processing. Exposure, tone, saturation and the selected color presentation belong in the embedded appearance rather than baked photographic pixels.

The DNG path should reuse sample encoding, profile packaging and output ownership. Family-specific preparation may supply the Classic technical base and appearance parameters. Nodes must retain functional contracts rather than branch on a camera-family enum.

The selected embedded appearance may approximate Classic photographic rendering. The owner accepts a close visual match rather than exact reference pixels. Numerical comparisons must state their color space, metric, settings and reader. A percentage without a defined metric is not an acceptance guarantee. Preserve useful exposure and white-balance editing latitude and report clipping, quantization and approximation limits.

Conformance requires exports from the selected SD15, DP2 and DP1s coverage, metadata and sample validation, checks that deferred appearance controls do not change technical samples, and comparison of reader-rendered appearance with the photographic path. Preserve Merrill export behavior. Lightroom and Capture One review must remain explicitly pending until performed.
