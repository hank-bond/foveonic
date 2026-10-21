---
summary: "Complete ordinary color rendering and editable DNG export across all pre-Merrill Sigma Classic cameras."
---

# Complete Classic camera coverage

Extend [Classic rendering](../docs/specs/classic-rendering.md) beyond the initial three-camera slice. The owner authorized autonomous implementation, sample acquisition and reasonable deterministic alternatives where the reference implementation has unstable behavior. Keep camera differences in recipe preparation and functional operations.

## Required models

SD9, SD10, SD14, SD15, DP1, DP1s, DP1x, DP2, DP2s and DP2x. SD1 belongs to the Merrill processing family. Custom Classic monochrome remains deferred.

## Work

- [ ] Acquire representative originals for all ten models, with provenance and ISO coverage.
- [x] Decode signed format-6 sensor samples and legacy CAMF compression without changing format-30 behavior.
- [x] Resolve legacy and cached calibration, drift, polynomial and shield corrections from metadata.
- [x] Add functional high-ISO reconstruction and inline floating-point color paths.
- [x] Preserve camera-specific correction order and low-ISO behavior.
- [ ] Validate TIFF, generic finishing, crop/orientation and DNG with an embedded approximate look for every model.
- [x] Keep unsupported UI controls disabled and normalize incompatible saved settings automatically.
- [x] Compare acquired originals independently against reference stage captures and Adobe DNG decoding/rendering.
- [x] Run regression checks and keep original captures separate from disposable outputs.

## Starting evidence

Format-30 SD15, DP2 and DP1s already have complete selected low-ISO routes. Original DP1 uses format30 but requires an inline calibration/color route and an ISO400 original reaches the separate denoiser. SD9, SD10 and SD14 use format6 and legacy metadata. Native sensor and metadata captures exist for these originals in the authorized reference checkout.

New public samples include a DP1 ISO100 original and a DP2x ladder at ISO50,100,200,400,800,1600 and3200. DP1x and DP2s sample acquisition remains open. Downloaded originals and provenance stay outside version control. Record coverage and remaining limitations explicitly rather than inferring support from camera-name aliases.

## Deterministic processing

The reference high-ISO diffusion uses overlapping workers with cross-iteration races. A deterministic GPU schedule is permitted for this integration. Document its equations, scheduling policy and measured differences. Keep the reference operation order, parameter provenance and sample units explicit. Do not silently substitute Merrill reconstruction.

## Decoder and operation evidence

The format6 decoder matches 35,615,232 independently decoded signed components across SD9, SD10 and SD14. CAMF bytes match the independently decoded originals for all three. Cached camera calibration remains unchanged. Six selected legacy/cached camera matrices agree with independent Float64 controls within 1e-12. Complete SD9 and SD10 red-detail passes match 21,164,544 components exactly. Recursive chroma matches eight independent whole-image fixtures, covering odd/even dimensions and signed extremes. The existing small and box supports retain their fixture results.

At matched neutral noise settings, DP1 ISO400 visible Stage2 differs in 4,813 one-code components out of 13,939,200. DP2x visible Stage2 mean absolute errors are 0.022226 code at ISO400 Sunlight and 0.58009, 0.753208 and 1.39672 at ISO3200 Sunlight, Auto and Incandescent respectively. The corresponding ISO3200 photographic sRGB16 mean errors are 92.28, 111.21 and 178.72 codes. Input to the Auto and Incandescent denoiser matches all 14,450,688 reference components exactly, and their nine camera-matrix coefficients agree within 1e-12.

SD9 and SD10 whole-sensor Stage2 means are approximately 0.15 and 0.27 code. Full photographic sRGB16 means are 17.60 and 13.78 codes respectively. SD14's visible Stage2 differs in 355 one-code components, with final mean 5.57 sRGB16 codes. These measurements use Standard mode and selected matched requests, rather than asserting identical application exports.

The reference diffusion races and its fringe storage behavior use deterministic GPU scheduling and initialized borders. Native control values must match the recipe before comparing images. Earlier zero-slider diagnostic captures apply different noise factors. Correct high-ISO parameters include pre-gain luminance normalization, separate noise-model and constant diffusion iterations, and DP2-family noise attenuation above ISO1000/2000. The stored third noise-model coefficient is not consumed by the reference kernels.

## Automatic and incandescent white balance

SD9, SD10 and SD14 use the weighted image estimator. Prepared camera/tone coefficients match independent reference controls within 1e-11, and brightness agrees within 1e-6. GPU neutral differences are at most 0.000031, 4e-16 and 0.000308 respectively. Repeated preparation dispatches no GPU work and performs no readback. Cached profiles use their recorded automatic calibration.

High-ISO Incandescent calibration mixes drift-adjusted channel means with the preset gains. Those means precede flare subtraction and gain application. Its matrix blend, and the high-ISO Auto matrix blend, happen before the working-basis conversion. Auto does not calculate unused gray-world statistics. Cached Auto/Custom chooses its closest preset spatial-gain grid using gain-weighted color matrices. The DP2x ISO3200 original selects Overcast, matching the reference row controls.

Preferred aperture and exposure-bias values come from the preview's optional Exif data. Capture-only reading remains bounded and avoids sensor payloads. SD9 uses an explicitly chosen EXPTIME-derived shutter fallback when SHUTTER is absent, rather than interpreting the display label's surprising historical parser behavior.

Engine-owned calibration statistics cache the latest immutable source. Public original samples are read-only through `Source::samples()`, so an in-place edit cannot silently reuse an earlier estimate. Nodes retain no cross-call state.

## DNG evidence

The final fixed-build pass exports default and adjusted Standard from all eight acquired models using Auto WB, plus DP2x ISO100/3200 and ISO3200 Incandescent cases. All twenty DNGs validate without Adobe SDK warnings. Independent SDK stage1 decoding preserves all 264,177,792 stored uint16 components exactly. Earlier technical-base checks confirm that exposure, tone and saturation leave samples unchanged. White balance and spatial detail alter them, and retained bases survive later renders.

Full-resolution SDK comparisons use 16-bit encoded sRGB. Mean absolute component errors range from 0.000189 to 0.002233 across these exports. The largest individual difference is 0.06122 on SD9, and at most one component per image exceeds 0.05. Metadata-only neutral and -2EV exposure edits validate on SD9, DP1 and DP2x ISO3200. All six preserve every stored sample and change the SDK render. Each exposure edit lowers mean rendered brightness. These measurements establish the embedded approximate appearance, rather than promising universal parity. Lightroom and Capture One review remain pending.

## UI and regression checks

The Classic recipe authors its inspection catalogue from ISO, enabled operations and metadata. The GUI offers only those stages and keeps layer/RGB/scalar component meanings. Comparisons follow actual compatible operation inputs. Changing photographs starts at Final, preventing a previous architecture's selected stage from becoming an invalid first request. Every advertised stage renders on acquired SD10 and high-ISO DP1 originals.

Merrill TIFFs remain byte-identical to main for default color, adjusted reconstruction and blue-layer monochrome. Color and monochrome DNGs are also byte-identical. Workspace tests, targeted GPU checks, strict all-target Clippy, formatting and documentation checks pass. Generated comparison images and repeated native captures are removed or deduplicated after retaining useful receipts and originals.

## Remaining validation

Genuine DP1x and DP2s originals remain unavailable after checking public sample catalogues, review sites, archived raw-pack links and photographer directories. Their camera routes are enabled from recovered metadata/version contracts. Synthetic processing-version probes verify DP1x's alternate tone selection, but do not count as original-file validation.

Accumulated bulb frames, alternate estimator flags, multiple FIR iterations, custom Classic noise controls and monochrome remain explicitly unsupported. Flash-specific estimator conditions have not received the same original-file validation as the nonflash corpus.

## Numerical policy cleanup

Shared reference color coefficients, neutral weighting, fixed-point and shutter units, profile exponent and route thresholds have one recipe-local definition. Camera policy resolves column-filter scaling, blue-axis mixing, drift gain, tone offsets and high-gain noise attenuation before operation construction. Metadata fallback noise models remain beside their named CAMF parameters. These changes preserve recovered coefficient values and arithmetic order. The color basis retains its unknown standard-space identity explicitly.

## Optional-stage contract audit

The low-ISO reconstruction bypass omitted the color matrix normalization and digital gain fused into FIR finalization. The bypass now applies that Q12 gain through a functional signed-code operation. Enabled FIR arithmetic is unchanged. The inspection catalogue exposes the gained codes, and photographic and DNG construction share the corrected boundary.

The audit also corrected exposure-dependent despeckle selection to prefer the recorded integer EXPTIME in microseconds, with unit-normalized CaptureShutter as a fallback. Early SD cameras store CaptureShutter in seconds. Auto sharpening now resolves to the same Classic unit default for the UI, photographic response and DNG detail. Merrill retains its metadata-derived default.

High-ISO reconstruction already preserves camera color conversion and nonlinear correction when filtering is disabled. Root conversion, detail controls, highlight/repair switches and DNG presentation separation retain their required conversion boundaries. Spatial filtering may change averages slightly, but must not control exposure normalization.

The corrected SD14 toggle changes mean photographic linear RGB by -0.00196 stops and mean DNG linear codes by -0.00063 stops. Across the eight acquired models' low-ISO originals, both boundaries change by less than 0.008 stops. At DP2x ISO3200, image-dependent range reconstruction changes photographic mean by -0.154 stops. This is not a dropped gain: a nonidentity, gain-bearing constant-field regression preserves all channels within one code with filtering on/off. The high-ISO filter's dark neutralization and residual recovery do not promise preservation of photographic image means. No image-mean normalization is applied.
