---
summary: "Validated independent monochrome detail and shared color sensitivity while preserving the sensor mixture."
status: archived
---

# Monochrome refinement

Requirements: [monochrome rendering](../../docs/specs/monochrome-rendering.md) and [photographic finishing](../../docs/specs/photographic-finishing.md).

The shared finishing work was committed separately. The owner requested research and discussion before implementation, rejected the current sensitivity strength and hard-looking color transitions, and authorized studying the reference implementation. That research request alone did not authorize implementation. The owner subsequently approved a broader layer-operation experiment, then requested its removal after photographic review. The retained scope is sensor mixing, independent monochrome Detail and improved monochrome color sensitivity.

- [x] Study the reference B&W conversion, parameter mapping and following noise filter.
- [x] Validate independently expressed conversion and filter-coefficient arithmetic against native kernels.
- [x] Discuss and approve the sensor-mix-preserving adaptation before changing photographic behavior.
- [x] Improve monochrome color sensitivity through photographic review of the source-color guide, hue selection, protection and amount response. The owner accepted the current behavior after comparison with the reference application; per-color HSL is a separate deferred feature.
- [x] Add an independent detail-restoration stage to monochrome noise reconstruction. The owner wants the same Fine/Coarse amounts and Size controls as Merrill color. These technical Detail edits also apply to DNG samples.

Preserve authored sensor-layer mixing and keep Noise, Detail, Local Contrast and Sharpening distinct. This implementation slice is Merrill-only. Local Contrast is an accepted standalone photographic effect, not a substitute for reconstruction detail.

## Sensor mixture constraint

The authored sensor mixture must remain the source of monochrome texture and noise behavior. The owner relies on favoring the less noisy blue layer. Sensitivity must not replace that image with a conventional RGB-to-gray conversion. A companion color guide may determine the adjustment, but it must not transfer noisy color texture unfiltered into the cleaner monochrome image. Noise, Detail, Local Contrast and Sharpening remain separate.

## Reference sensitivity findings

Research examined the installed reference implementation version 16.6.3.6, its arm64 host parameter construction and embedded Metal B&W v8 kernels. These findings describe the recovered numerical stages, not full application-to-application photographic equivalence. No proprietary implementation is imported into production code.

The conversion first uses a built-in linear RGB profile identified as Adobe 1998 G10. Its Y colorants are approximately 0.3328, 0.6121 and 0.0551. Its exact profile matrix differs from this project's existing Adobe RGB profile, so the name alone is insufficient to match the guide space.

For a pixel, sort RGB into low, middle and high values while retaining which channel is lowest and highest. The conversion is a neutral floor plus two channel differences:

```text
M = low + (middle - low) * secondary[lowest_channel]
        + (high - middle) * primary[highest_channel]
```

Primary coefficients are indexed Red, Green, Blue. Secondary coefficients are indexed Cyan, Magenta, Yellow, complementary to the lowest channel. The six neutral coefficients in Red, Yellow, Green, Cyan, Blue, Magenta order are 0.3328, 0.9449, 0.6121, 0.6672, 0.0551, 0.3879. Host construction adds the corresponding percentage slider divided by 100 to each coefficient. The native path quantizes them to signed 1/1024 units. Channel differences vanish toward gray, naturally reducing sensitivity without dividing by tiny chroma. The function is continuous across channel-order boundaries, although its first derivatives need not be.

Strong negative results use a smooth darkening limiter. Ignoring integer rounding, when `M < Y/8`, the conversion substitutes `max(0, 25*Y*Y/(288*Y - 1024*M) - Y/32)`. The native fixed-point implementation has specific shifts and rounding around this equation. Output packing divides both adjusted and neutral brightness by a power of two when coefficients require headroom. The later split-tone stage independently reconstructs the same scale and restores it before output. This packing is distinct from a photographic exposure change.

A native Metal probe compared an independently expressed integer conversion with 365,104 pixels across 38 control configurations. Samples include full hue ramps at seven saturations and three brightness levels, random RGB samples, neutral settings, all six controls at both signs and a combined-control case. All four packed output components match exactly. This verifies the recovered conversion for the supplied coefficients. It does not validate every host/UI conversion or the entire application pipeline.

### Spatial cleanup

A following dedicated B&W denoising stage extracts adjusted and neutral brightness, adds a small black offset and converts both to fixed-point log brightness. It constructs neighborhood means, variances and cross-covariance, derives bounded two-input affine coefficients, smooths those coefficients and evaluates the filtered brightness. Full-scale host orchestration sets a maximum radius of 16 with successive larger filter scales. Reduced rendering changes that footprint and sufficiently small footprints bypass the operation. A final log-domain blend combines filtered and unfiltered brightness according to its noise amount before conversion back to linear brightness.

Recovered coefficient uniforms are `[1, 1, 1048576, 8192, 0.125, 8192, 0, 0]`. The fitted slopes are bounded to 0 through 3.999 and packed at 8192 codes per unit. The offset is packed at 0.125 codes per input unit around 32767. A separate native probe compared 4096 synthetic neighborhood statistics with independently expressed double-precision equations. Maximum differences were one code for each slope and zero for the offset. Full multiscale orchestration, border behavior, image-level noise suppression and application photographic agreement remain unvalidated.

## Difference from the earlier approach

The earlier shared shader computes normalized RGB hue, interpolates neighboring hue controls, applies saturation and near-black protection, then uses an exponential exposure gain of up to two stops per signed unit. It has no spatial stabilization for the sensitivity correction. A moderately saturated subject can already receive full strength, and local hue variation or color noise can become exaggerated brightness variation. Applying the gain after the camera appearance also differs from changing a linear brightness mixture.

The reference response is not uniformly weaker at every hue or extreme setting. Its small-change behavior depends on actual channel differences and neutral luminance, and strong darkening is separately restrained. Reducing the current two-stop constant would not reproduce it.

## Approved adaptation

Keep the dedicated sensor-mixed and denoised monochrome image. Derive neutral and adjusted brightness from an independent white-balanced color guide using the recovered six-coefficient response. Their ratio gives a brightness correction independent of the selected sensor mixture. Spatially stabilize that correction with the cleaned neutral color structure as a guide, then apply it without replacing the underlying texture. Zero settings must remain an exact bypass.

The Merrill implementation uses an ordinary cleaned linear color reconstruction with texture boosts disabled, independent of the authored mono mix and retained creative color controls. It applies a bounded log brightness correction before tone, with a separable edge-aware filter guided by the cleaned neutral color image. This filter is an adaptation, not the reference multiscale guided filter. Photographic tuning remains subject to owner review. In particular, copying the reference RGB-to-gray output directly would defeat the sensor-mixture constraint. A faithful response adapted to a separate sensor-derived mono base is an intentional departure from the reference's complete B&W pipeline.

## Retired experiments

Photographic review rejected synthetic layer matrices, curves, development inhibition, dye overlap and luminance inversion. Symmetric layer interactions mostly duplicated saturation or contrast, while transferring corrections from uncleaned signals exaggerated shadow noise. These runtime paths and their UI were removed. Pre-release sidecars discard their retired layer settings while retaining unrelated edits. Future creative work should demonstrate a useful photographic result before introducing another processing branch.

## Validation

The refocused implementation passes workspace tests, strict Clippy, formatting, documentation indexes and artwork checks. GPU fixtures cover six-color brightening/darkening, gray protection, signed and black samples, exact neutral bypass and flat-field Detail. The original portrait checks equal monochrome channels, changed mix/detail/sensitivity results, cached/fresh equality, typed inspection and DNG separation. A broader finishing test covers reuse with HDR, Local Contrast and universal sharpening active. Sensitivity inputs receive cache admission priority over cheaper downstream checkpoints so a hue edit does not rebuild the color guide. The existing cache byte limit remains unchanged.

The local optimized app bundle is the review build. The owner subsequently accepted the sensitivity behavior. Per-color HSL remains separate from this completed slice.

## Highlight headroom correction

A high-contrast macro original exposed flat patches in monochrome with histogram noise filtering enabled. The sensor mixture retained bright texture, but the histogram assigned every value above 4095 to its last bin while accumulating their full values. Different highlight levels then received the same neighborhood mean. Tone mapping made the discontinuities more visible. This occurred before independent Detail and sensitivity.

The correction filters a bounded base through the existing 1024-bin noise model and carries the positive excess through unchanged. Initial accumulation, sliding removal and sliding insertion all use the same bounded samples. This preserves highlight texture without increasing GPU scratch or changing color processing. The no-histogram route remains unchanged. GPU checks compare crossing-ceiling neighborhoods with independently evaluated weighted means and verify unchanged texture where the entire neighborhood exceeds the modeled range.

## Shared color sensitivity and DNG

The owner approved the same sensitivity group and operation in Merrill Color and Monochrome, and baking active adjustments into DNG samples. Originals remain archived as X3F. The response now lives in shared linear finishing rather than monochrome reconstruction. Both classification and range filtering use the same neutral cleaned color guide, making the scalar gain independent of the displayed rendering and sensor mixture. Multiplication preserves the color image's RGB ratios. Neutral controls bypass the branch.

The color guide follows the active Full, reduced or regional working grid. Draft keeps clean classification on its reduced grid even when the displayed base omits optional reconstruction. Viewport support includes the sensitivity filter radius. DNG selects native geometry and bakes the same gain, while the existing camera tone presentation remains metadata. The established sidecar key is retained for saved edits. A new CLI color-sensitivity option preserves the selected rendering mode, while the existing mono option retains its mode-selecting behavior.

HDR and Local Contrast baking were initially deferred. The placement review and approved shared linear stage below supersede that initial export boundary.

Shared sensitivity checks pass on an original Merrill macro photograph: Color and Monochrome produce byte-identical clean guides, RGB receives a common gain, and the DNG technical base receives that same gain within Float32 tolerance. Full/reduced/regional rendering and Smart continuation remain valid with sensitivity combined with HDR, Local Contrast and universal sharpening. Cached edits match fresh rendering without rerunning calibration or noise reconstruction. Existing saved-key round trips, rendering-mode UI persistence, CLI mode selection and native DNG geometry are covered. The following placement review supersedes the initial deferral of HDR and Local Contrast baking.

## Local Contrast placement review and following noise audit

The owner supplied a saved Merrill color edit exercising all four HDR controls and approved a before/after placement comparison. The local review keeps the original settings and compares the existing post-curve Local Contrast with the same six-band filter applied after HDR but before the camera tone curve. Brightness extraction is normalized to the same nominal-white units in both placements. Current-order reconstruction matches the ordinary renderer byte for byte, and zero-amount reconstruction matches its ordinary bypass. The review also includes half/double pre-curve strength and an Off reference. The owner reported no perceptual difference and approved the pre-curve placement and DNG baking described below.

The following luminance/banding audit varied all five positions on that original, with Detail neutral and at the authored values. A fixed 256-pixel dark-background window showed monotonically decreasing fine residual RMS for Luma (about 71% lower at 4 than 0) and monotonically decreasing detrended column RMS for Banding (about 52% lower). Banding changed the fine residual measure by less than 1%. The trends persisted with authored Detail, which increased grain as expected when boosting texture. These window statistics include scene structure and do not establish whole-image SNR. Strongest-position cached/fresh outputs matched in all four series. Isolated GPU checks passed luminance/chroma endpoint and frequency separation and all five native column presets. No processing change was needed. Banding 0 remains the weakest active preset, and Row banding remains separate. Broader high-ISO photographic review is still useful.

## Adopt pre-curve spatial edits for DNG

After comparing the saved macro edit, the owner approved moving Merrill Local Contrast after HDR and before the camera curve, and baking active HDR and Local Contrast alongside sensitivity. Color and Monochrome now share that linear-edit recipe with DNG. Export derives the selected-look reference gain and applies it to the established technical DNG base. Selecting the strongest absolute reference component avoids unstable luminance division for signed RGB. Zero reference vectors retain the original technical sample. This transfer is valid for scalar brightness edits only.

The camera curve and Exposure still determine the HDR pivot but remain in DNG presentation metadata. Reference tone, neutral amounts and section bypasses omit the spatial work. DNG always uses native full-frame geometry. The subsequent shared stage extends this boundary to Classic and Quattro and includes Universal sharpening; the format-support labels introduced in the Merrill slice are removed.

Original-file checks pass on the supplied macro edit and a portrait, in Color and Monochrome, with Local Contrast at the authored strength and both extremes. They verify matching combined DNG gain, finite samples, exact section bypass, native export geometry and cached/fresh equality without repeated reconstruction or HDR-mask generation. The existing regional/Smart continuation check passes with sensitivity, HDR, Local Contrast and Universal sharpening active. A GPU fixture covers signed samples, zero luminance, tiny reference components, zero gain, immutable inputs and mismatched geometry. Source originals and sidecars remain unchanged.

Production TIFF pixels match the reviewed pre-curve candidate exactly at native size after lossless RGB16 PNG conversion. The same saved recipe completes DNG packaging, and the system image reader recognizes the expected oriented dimensions. This checks packaging and sample behavior, not appearance in downstream editors. Workspace tests, strict Clippy, formatting, documentation indexes, artwork checks and whitespace checks pass.

The owner subsequently approved extending the linear stage and DNG baking to Classic and Quattro, including Tiger, and moving Universal sharpening before camera response. The [shared implementation](./shared-linear-edits.md) records its adapters, cache fixes, output-section preferences and checks.
