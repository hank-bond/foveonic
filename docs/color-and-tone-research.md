---
summary: "Research catalog of perceptual saturation, color grading, tonal adjustment, highlight rendering and automatic exposure methods."
kind: reference
---

# Color and tone research

This reference compares color and tone methods from darktable, Capture One and the current Foveonic pipeline. It records mechanisms, source evidence and limitations independently of which features we choose to build. Candidate workflow and implementation decisions belong in the [perceptual editing exploration](../plans/perceptual-editing.md).

## Research scope and evidence

Research reviewed darktable documentation, its `release-5.6.0` source, selected development modules, Capture One documentation, and the current Foveonic implementation. Capture One documents control behavior but does not publish its processing equations. Owner-authorized local numerical inspection supplements those public descriptions below. Similar interfaces and outcomes do not establish numerical equivalence.

The development Saturation Curve and Spektrafilm references must be treated separately from the inspected 5.6.0 source snapshot. Recheck their status before implementation. Mathematical descriptions below summarize the cited methods. Proposed Foveonic work is tracked separately in the [exploration plan](../plans/perceptual-editing.md).

### Local numerical inspection

The owner authorized local inspection of Capture One 16.6.3.6 for Apple Silicon and a separate darktable checkout. The durable local archive convention is `~/git/capture-one-research/<version>-<architecture>/`. It retains version hashes, bounded host disassembly, Ghidra pseudocode, Metal bitcode and readable IR, operation indexes, extraction scripts and measured kernel probes. Keep this archive independent of disposable build output. Decoded code, reviewed equations, documented behavior and native validation are distinct evidence levels.

### What the local inspection establishes

The inspected version is 16.6.3.6 for Apple Silicon. The local reference implementation contains several generations of algorithms and different raw, processed-image and DNG routes. The research goal is to understand useful numerical choices. Matching its complete UI, processing-version compatibility or every camera profile is outside this investigation.

The completed isolated checks cover 184 configurations across original host functions and Metal kernels. They include ramps, neutrals, varied hues and chroma, random RGB samples, headroom, hue wraparound, selection falloff, overlapping corrections and combinations of three-way lightness settings. Each check has a bounded scope. Camera setup, every slider-to-field conversion and full color-managed application appearance are not established by these checks.

| Area | Mechanism established | Evidence and practical limit |
| --- | --- | --- |
| Global Saturation | Negative values scale RGB distance from gray. Positive values favor muted colors and taper as relative saturation increases. | Original GPU equations checked at six amounts over 1,108 RGB samples. This arithmetic has no explicit skin-hue selection. |
| Brightness | An anchored rational curve followed by hue reconstruction and partial restoration of original saturation. | Five GPU amounts checked. Its useful domain and placement differ from unrestricted scene-linear exposure. |
| Contrast | A pivoted polynomial curve followed by hue reconstruction, saturation blending and brightness normalization. | Five GPU amounts checked. Profile selection can change the pivot and gray weights. |
| Exposure | Linear gain plus a separately selected protective positive-exposure curve. | Five host gain-split checks and four GPU shoulder checks. Input type, baseline and flags choose the route. |
| Levels | Input endpoints, a versioned anchored midpoint curve, then output endpoints. | Eight GPU midpoint checks. Histogram handles are not independent exposure bands. |
| Three-way Color Balance | Separate tint and lightness operations with different responses for shadows, midtones and highlights. | Nine isolated GPU checks and 27 host combination checks. The packaged GPU and host formulas differ and must remain distinct. |
| Color Editor | Profile-based Lab/LCh selection, smooth hue/chroma falloff, chroma-scaled Lightness and versioned overlap policies. | Host HSL sweeps, hue wrap/falloff, overlap, neutral and affected-range checks. Internal field values are not assumed to equal UI units. |
| Curves | Cubic Hermite interpolation and a separate Luma remapping policy. | Six GPU checks with explicit knots. Arbitrary authored curves are not guaranteed to be monotonic. |
| Gamut compression | Convert into the destination RGB basis, soften the upper boundary and blend channel clipping with a hue-preserving remap. | Nine GPU checks with identity profile transports. This isolates the operation from output-profile selection. |
| Dehaze | Haze-reference subtraction with a soft floor, bounded correction gain, chromatic restraint and reference normalization. | Thirty-six host configurations over 256 reference/pixel pairs each. Haze estimation and camera calibration are separate. |
| White Balance | Camera-calibrated Kelvin/Tint conversion produces channel gains. | Host and GPU dataflow reviewed. A final multiplication does not define a universal Kelvin model. |
| HDR | Log-brightness guide, spatial filtering, residual-aware response and coupled scalar gain maps. | Earlier retained native probes cover coefficient filtering and reconstruction. They were not rerun as part of the 184 configurations above. |
| Clarity and Structure | Multiscale filtered bases, nonlinear residual responses and different color-preservation policies. | Host orchestration and pixel math reviewed. Exact method/radius UI matching is outside the present scope. |
| Monochrome | Color-dependent tonal response with a separate neutral reference and spatial stabilization. | Earlier native response and filter evidence retained. Sensor-layer blending remains a separate Foveonic responsibility. |

### Domains and processing order

A large part of the response comes from where an operation runs. The inspected implementation includes a Rec.2020-primary working profile with a toe and approximately gamma-two encoding, an older photographic RGB representation, camera matrices, film curves and output-profile conversions. These are distinct from linear sensor values and from Oklab.

One traced raw rendering route groups camera matrix, exposure, gamma/profile or film response, saturation and contrast inside an exposure operation. The modern processed-image chain places spatial HDR and Clarity before the main tone/appearance stack, with Brightness, Levels and Curves following the exposure operation. Modern three-way Color Balance appears late, before output gamut/profile conversion. Older versions and alternate outputs have different branches. This is a useful separation of responsibilities, not one fixed order to reproduce.

A broad workflow can therefore distinguish preparation and scene placement, tonal shaping, creative color grading, and output rendering. Sharing a tool's name or equation across those boundaries does not preserve its appearance. Baking an operation into linear-encoded DNG samples is possible without making its artistic behavior equivalent to applying it in scene-linear light. A bounded display-domain operation also needs an explicit headroom and negative-value policy before reuse in our linear stage.

### Global saturation versus selected color saturation

For the inspected global saturation kernels, let `Y = dot(weights, RGB)`. Negative amount `S` uses `gain = 1 + S/100`. Positive amount uses:

```text
k = 1 + 1.6*S/100
c = 1 - min(RGB)/max(Y, epsilon)
gain = k / (1 + (k - 1)*c), when c > 0
RGB' = Y + gain*(RGB - Y)
```

The positive response is restrained by existing relative colorfulness. A nonnegative primary with a zero minimum channel receives no additional boost from this equation. Muted colors receive more. The weights belong to the working profile. They are not a universal set of scene-luminance coefficients. Separate legacy/profile saturation splines also exist. Their presence does not establish a saturation-versus-saturation axis identical to our prototype.

The Color Editor instead classifies in profile-based Lab/LCh. Its hue sectors wrap around the circle. The selected interval has full weight, with cubic smoothstep falloff outside it. Chroma selection has a similar falloff. The two weights multiply, with optional inversion. In one validated correction branch:

```text
L' = clamp(L + C*weight*lightness*0.1, 0, 100)
C' = clamp(C*(1 + weight*saturation/80), 0, 181.01933)
h' = wrap(h + weight*hue)
```

These are internal parameter scales. The important photographic choice is that selected Lightness scales with input chroma. An achromatic pixel receives no change from this HSL correction. Low-chroma colors receive a smaller brightness change than vivid colors. A hue mask alone cannot offer that restraint because the hue of a near-neutral pixel is unstable.

Some correction groups normalize overlapping weights when their sum exceeds one. Another branch accumulates corrections. The optional restrained positive-saturation branch aims toward a bounded chroma response and only uses the result when it increases chroma. Skin uniformity is a convergence toward a target, with separate semantics. These findings support keeping selection, overlap policy and adjustment separate in our architecture.

Global and targeted Saturation can therefore have different useful jobs. Consistent naming and predictable strength do not require identical math. An approachable global control can favor muted colors, while a selected-color control gives direct chroma control. A common conversion and gamut policy may still serve both.

### Exposure, brightness and contrast

Scene exposure has a clear physical definition: multiply linear-light values by `2^EV`. Protective software behavior is an additional choice. The inspected baseline helper splits negative and positive gain. A selected positive-exposure path is linear below a knee, then bends toward an anchored white through a rational shoulder. This explains how a slider named Exposure can be more protective on some inputs than a literal multiplier.

The inspected Brightness curve uses `g = 2^(1.8*B/50)` and, per channel:

```text
f(x) = g*x / (1 + min(x, 1)*(g - 1))
```

Within zero to one it anchors black and white and remains increasing for positive gain. Above one, this initial curve leaves headroom unchanged. It moves midtone brightness through a different family of curves from Exposure. For large boosts and negative inputs, the rational expression can encounter a pole. Its upstream input policy matters.

After the per-channel curve, the operation reconstructs RGB using the original channel ordering and middle-channel position. It blends 25% original HSV-like saturation with 75% curve-result saturation. Thus its final behavior is neither three unrelated channel curves nor a scalar exposure gain. This preserves the RGB hue coordinate used by the operation, not exact perceptual hue.

The inspected Contrast curve fixes black, a profile-dependent pivot, and white. It uses separate cubic expressions below and above the pivot. Positive and negative strength have different scaling. After the curve it reconstructs hue, blends original and resulting saturation equally, and normalizes brightness to the independently curved result. These color corrections are substantial parts of the control's appearance.

The useful choices are therefore independent: the shape of a tone curve, its placement in the pipeline, how its strength maps to the slider, and the color invariant used when transferring it back to RGB. Choosing a pleasant curve does not settle the other three.

### Levels, Luma curves and three-way lightness

Levels separates black/white input placement, midpoint shape and output endpoints. Its version-11 midpoint blends identity with an anchored fourth-root curve for lightening or a fourth-power inverse for darkening. Negative inputs have derivative-matched linear extensions. Input endpoint clipping and output endpoint changes remain separate operations. Levels is useful for tonal placement but cannot reconstruct missing sensor samples.

The ordinary curve evaluator uses cubic Hermite segments with stored endpoint slopes. A Luma curve first evaluates a weighted brightness proxy. Darkening transfers the result as an RGB gain. Lightening mixes a bounded gain with movement toward white, based on the brightest channel, instead of allowing arbitrary channel overshoot. This policy avoids some saturation changes from independent RGB curves, but it can deliberately change saturation during a lift.

Three-way Color Balance supplies another useful family of lightness responses. Its wheels' Saturation means strength of a chosen tint. Neutral saturation of existing colors is a separate operation. The inspected host lightness path uses a maximum-channel guide, broad shadow lift, a bell-shaped midtone response and separate highlight behavior, followed by brightness normalization and highlight compression. The packaged Metal variant instead has multiplicative shadow/midtone/highlight factors. Both are pointwise at this step. They are distinct versions of the concept, not evidence that the masks must match.

This makes the three-band EQ analogy useful for the interface while leaving several numerical options open. The ranges can overlap smoothly without being three copies of the same exposure formula. Equal settings do not inherently need to equal global Exposure. A brightness control can preserve an endpoint, change a floor, or act as a midtone power curve depending on its intended role.

### HDR, highlight reconstruction and gamut handling

The inspected HDR implementation measures a log-brightness guide, filters a spatial base and retains residual information. Shadows/Highlights and Blacks/Whites use coupled gain responses with different endpoint behavior. Detail-sensitive limits restrain how the correction transfers across transitions. It is not merely four masks multiplying independent stop offsets.

Clipping recovery is a separate pointwise operation in the inspected path. It identifies channels approaching calibrated thresholds, borrows information from other channels and progressively blends strongly clipped colors toward neutral. This can remove harsh color patches, but it does not create captured texture where all channels lost it. A display shoulder addresses a further problem: fitting surviving headroom into an output range.

The gamut-compression kernel transforms into the destination basis, derives a soft upper knee from the minimum destination channel, and computes both independent-channel and hue-preserving range remaps. It blends these responses, clamps in the destination basis, and transforms back. This is an explicit output-gamut policy, not a perceptually uniform saturation operator or a complete AgX-style view transform. Its negative-value clamp also means that signed intermediate data requires a separate earlier policy.

For ETTR, preserve surviving headroom first, place the image's baseline exposure, and evaluate how a chosen shoulder renders its bright subjects. Sensor reconstruction, midtone placement, gamut mapping and display roll-off have different jobs. Applying another shoulder after the camera curve may compress highlights twice.

### Dehaze and local contrast

The reviewed global Dehaze application takes a haze-color reference and a white/reference level. Positive correction softly subtracts a bounded haze contribution, separates a neutral contribution from chromatic residual, and limits transmission gain. It then restrains excess saturation. The caller normalizes its reference response. Negative correction follows a different haze-adding response. Global application is pointwise once the reference is supplied. Regional edit maps supply different authored amounts and references. This does not establish a spatial dark-channel estimator.

Clarity forms filtered bases at more than one scale. One reviewed orchestration path uses an eighth-resolution pyramid and Gaussian filtering. Radius depends on native image size and processing scale. Neutral transfers a luminance-derived gain. Natural and Punch have different per-channel and residual behavior. Positive and negative amounts use different mappings, including rational limiting of strong darkening residuals. Structure targets finer detail separately.

These controls illustrate why a pleasing local-contrast effect is more than selecting a large blur radius. Choice of guide, scale, residual normalization, asymmetry and color transfer all affect its character. Its purpose remains separate from technical reconstruction and sharpening.

### What to carry into further experiments

The mature-platform examples support a small set of choices to evaluate independently:

- Use an anchored increasing curve for broad brightness shaping. Check the entire allowed strength range, including combined settings and headroom.
- Separate scene Exposure from a midtone Brightness control and a highlight-rendering shoulder.
- Evaluate a restrained global saturation response separately from tonal-range saturation and selected-color chroma adjustment.
- Give hue-selected Lightness explicit neutral/chroma restraint. Smoothing hue boundaries alone is insufficient.
- Treat color preservation and output gamut handling as part of each experiment's declared behavior.
- Declare the processing domain and guide input. Keep camera calibration, selection, adjustment, and output rendering responsibilities separate.

The rejected masked-EV prototype demonstrates a concrete failure: its normalized smooth weights still permit negative tone-curve slopes in crossovers. At +2 EV shadows or midtones, or -2 EV highlights, the minimum log-brightness slope is -0.2. Neutral equality and equal-band checks were insufficient. The gray appearance requires a separate downstream curve/clipping trace because an RGB scalar gain alone preserves channel ratios.

No replacement method, new default or processing change is selected by this research. Exact UI strength mappings, legacy compatibility, every camera profile and automatic exposure estimation remain outside the completed numerical slice. Those uncertainties do not prevent comparing the response families above.

## Color dimensions and saturation methods

Luminance measures a weighted light signal. Perceptual lightness models how light or dark a color appears. Chroma measures departure from a neutral of comparable reference conditions. Saturation relates colorfulness to brightness. Holding one of these constant does not hold the others constant.

| Method | Preserved quantity or response | Useful application and limits |
| --- | --- | --- |
| RGB distance from gray | A chosen RGB gray axis, often with constant calculated luminance | Cheap global saturation. Working primaries and gray weights affect the result. Later channel clipping can change hue and brightness. |
| HSV or HSL adjustment | The model's value or lightness coordinate | Convenient controls, but equal numerical adjustments are not perceptually uniform. The encoding and RGB basis matter. |
| Perceptual chroma scaling | Approximate hue and lightness in a model such as Oklab | Targeted edits with more predictable perceived lightness. Requires separate gamut handling. Constant lightness does not imply constant luminance. |
| Perceptual saturation and brilliance | Coordinated changes through colorfulness and brightness dimensions | Offers directions distinct from ordinary chroma scaling. Darktable separates these controls. |
| Vibrance | A boost weighted toward less-colorful pixels | Convenient global enhancement. Skin protection and neutral protection are separate design choices. |
| Gamut-relative saturation | Colorfulness normalized against a gamut boundary | A comparable position relative to the maximum available colorfulness across hues and lightness levels. The selected gamut and viewing model must be explicit. |

[Oklab's design](https://bottosson.github.io/posts/oklab/) explains perceptual hue, chroma and lightness operations and their tradeoffs. [Darktable's color dimensions](https://docs.darktable.org/usermanual/4.2/en/special-topics/color-management/color-dimensions/) distinguishes saturation, chroma, brightness and luminance. No single model makes every image operation perceptually exact.

### Three complementary selection axes

| Axis | Question it answers | Example |
| --- | --- | --- |
| Starting saturation | How should already-muted or already-vivid colors respond? | Increase moderate colorfulness while restraining vivid colors. |
| Tonal range | Where in the brightness range should colorfulness change? | Enrich midtones, protect noisy shadows and soften colorful highlights. |
| Subject hue | Which colors should change? | Reduce cyan, warm foliage or refine skin. |

These axes can share a perceptual adjustment operation while retaining distinct selection functions. They should not be presented as three interchangeable meanings of one slider.

## Catalog of color tools

| Tool | Processing concept | Relevance to Foveonic |
| --- | --- | --- |
| [Darktable Color Balance RGB](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/color-balance-rgb/) | Separates chroma, vibrance, perceptual saturation and brilliance. Provides overlapping shadows, midtones and highlights masks and soft gamut limiting. | Main reference for tonal-range colorfulness and separate brightness behavior. Its source offers JzAzBz and darktable UCS formulations. |
| [Darktable Saturation Curve](https://docs.darktable.org/usermanual/development/en/module-reference/processing-modules/satcurve-rgb/) | Curves drive saturation and brilliance using starting saturation normalized against a gamut boundary. Optional guided filtering stabilizes selection. | Main reference for baseline saturation shaping. This was inspected as development work. |
| [Darktable Color Equalizer](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/color-equalizer/) | Smooth hue-dependent hue, saturation and brightness adjustments, with guided filtering. | Main reference for evolving our Color Editor without harsh hue transitions or excess noise amplification. Filtering mitigates artifacts rather than guaranteeing their absence. |
| [Darktable Color Zones](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/color-zones/) | Lab/LCh curves selected by hue, lightness or chroma. | Useful interaction and selection ideas. Its documentation cautions about color transitions, so it is not the preferred numerical starting point. |
| [Darktable Color Harmonizer](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/color-harmonizer/) | Pulls colors toward palette hue anchors, with saturation adjustments, neutral protection and smoothing. | A creative palette tool with a distinct purpose from saturation or white balance. |
| [Darktable Color Contrast](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/color-contrast/) | Adjusts green–magenta and blue–yellow separation through Lab opponent axes. | A compact creative operation. Unequal axis scaling can change hue as well as chroma. |
| [Darktable Velvia](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/velvia/) | Weighted saturation enhancement with midtone protection. | A weighted saturation look with documented limitations. Its documentation warns about difficult hue and brightness shifts. |
| [Capture One global saturation](https://support.captureone.com/hc/en-us/articles/360002602197-Adjusting-saturation-in-the-Exposure-tool) | Positive values have a vibrance-like response with skin restraint. Negative values perform ordinary desaturation. | Evidence that a pleasant global control need not match a targeted saturation control's response. Its public documentation omits equations. The local inspection establishes selected numerical paths. |
| [Capture One Color Editor](https://support.captureone.com/hc/en-us/articles/360002601358-The-Color-Editor-overview) | Targeted HSL adjustments with adjustable selection smoothness and more precise range selection in Advanced mode. | Reference for approachable subject-color editing and showing the affected range. |
| [Capture One Skin Tone Uniformity](https://support.captureone.com/hc/en-us/articles/360002596077-Adjusting-skin-tones) | Pulls selected hue, saturation or lightness toward a reference color. | A tool for uneven skin and palette consistency. It is a convergence operation, not an ordinary HSL offset. |

The inspected [Color Balance RGB source](https://github.com/darktable-org/darktable/blob/release-5.6.0/src/iop/colorbalancergb.c) includes perceptual adjustment and gamut limiting. [Saturation Curve source](https://github.com/darktable-org/darktable/blob/master/src/iop/satcurvergb.c) is a moving development reference. Pin a revision and preserve attribution if adapting code.

## Catalog of tone and detail tools

| Tool | Processing concept | Relevance to Foveonic |
| --- | --- | --- |
| [Scene-linear Exposure](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/exposure/) | Multiply black-subtracted RGB by `2^EV`. | Keep a clear exposure meaning. Software gain changes signal values but does not reproduce the signal-to-noise improvement of collecting more light. |
| [Capture One Brightness](https://support.captureone.com/hc/en-us/articles/360002609938-Adjusting-brightness-in-the-Exposure-tool) | Primarily changes midtones and their contrast. | Reference for a visual brightness adjustment distinct from Exposure. |
| [Capture One RGB and Luma curves](https://support.captureone.com/hc/en-us/articles/360002604017-Improving-contrast-without-altering-saturation-in-the-Curve-tool) | RGB contrast can increase saturation. Luma contrast is designed to avoid that coupling. | Separate tonal intent from incidental color changes. The exact invariants still depend on implementation. |
| [Darktable Tone Equalizer](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/tone-equalizer/) | Brightness-dependent exposure corrections through an edge-aware guide. | A reference for changing broad tonal relationships while retaining local texture. The existing Foveonic HDR operation remains its own reviewed implementation. |
| [Capture One HDR](https://support.captureone.com/hc/en-us/articles/360002602737-Recovering-details-in-the-highlights-and-shadows) | Highlights, Shadows, Whites and Blacks alter tonal placement. Strong compression can reduce contrast and saturation. | Useful UI and behavior reference. Documentation does not establish exact spatial filtering or sensor reconstruction equations. |
| [Darktable Contrast Equalizer](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/contrast-equalizer/) | Wavelet-scale luminance and chroma detail adjustments with edge controls. | Multiscale contrast and texture control. Its horizontal axis represents feature size, not brightness. |
| [Darktable Local Contrast](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/local-contrast/) | Local Laplacian or bilateral processing of Lab lightness. | Reference for local separation with a different character from a single high-pass filter. |
| [Capture One Clarity and Structure](https://support.captureone.com/hc/en-us/articles/360002613998-Adjusting-local-contrast-in-the-Clarity-tool) | Larger versus finer transitions, with methods choosing different saturation and highlight behavior. | Keep local contrast, reconstruction detail and sharpening conceptually separate. |

[Capture One Exposure](https://support.captureone.com/hc/en-us/articles/360002602077-Adjusting-exposure-in-the-Exposure-tool) documents camera-like exposure behavior and additional protective behavior for processed inputs. A named Exposure slider does not establish identical behavior across raw and processed image paths.

## Catalog of highlight and rendering tools

| Tool | Role | Limits and relevance |
| --- | --- | --- |
| [Sensor highlight reconstruction](https://docs.darktable.org/usermanual/4.2/en/module-reference/processing-modules/highlight-reconstruction/) | Estimates missing clipped-channel information from surviving channels and neighboring evidence. | Fully clipped areas receive plausible estimates. Bayer-specific sampling algorithms cannot be transplanted directly onto Foveon layers. The linked description is an older versioned reference. Recheck current methods before implementation. |
| [Filmic RGB](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/filmic-rgb/) | Fits scene dynamic range to a display with a toe, shoulder, contrast and highlight color policy. Includes separate bright-source reconstruction/blending controls. | Explicit tonal placement and highlight appearance. Its transition treatment is distinct from recovering recorded sensor detail. |
| [Sigmoid](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/sigmoid/) | A compact view transform with contrast, skew and per-channel or ratio-based color processing. | A useful simple comparison candidate. Color paths toward white depend on the chosen processing and hue-preservation policy. |
| [AgX](https://docs.darktable.org/usermanual/development/en/module-reference/processing-modules/agx/) | Coordinates gamut preparation, primary mixing, log-domain tone response and color behavior near output limits. | Relevant to colored highlight plateaus, shoulders and out-of-gamut inputs. It does not recover missing sensor samples. Darktable's implementation has additional controls and should not be assumed identical to Blender defaults. |
| [Spektrafilm](https://docs.darktable.org/usermanual/development/en/module-reference/processing-modules/spektrafilm/) | Simulates film, development, printing and scanning from RGB input, including layer interactions and spatial effects. | A creative rendering alternative that combines several film-related effects. This was inspected in development documentation. |

The [AgX source](https://github.com/darktable-org/darktable/blob/release-5.6.0/src/iop/agx.c) explicitly handles negative RGB components before its log-domain tone operation. This is more than a one-dimensional shoulder curve. [Blender's AgX description](https://docs.blender.org/manual/en/5.0/render/color_management/displays_views.html) describes its high-exposure desaturation behavior.

## Current Foveonic findings

| Area | Current behavior | Consequence |
| --- | --- | --- |
| [Universal saturation](../crates/foveonic/src/nodes/finish/photographic/tone.wgsl) | RGB distance from luminance-weighted gray, with restrained positive boosts. | Preserves calculated luminance before later clipping. It is not the same operation as Color Editor saturation. |
| [Color Editor](../crates/foveonic/src/nodes/finish/color_editor.wgsl) | Oklab chroma scaling and rotation after guide-based selection and stabilization. Lightness edits transfer scalar exposure gains. | Retains Oklab lightness for chromatic adjustments, rather than physical luminance. Brightness and chromatic controls do not currently share one perceptual model. |
| [Shared finishing](../crates/foveonic/src/render/recipes/finishing.rs) | Universal contrast and saturation run after camera appearance. | Their result differs from the same numerical operator placed before the camera curve. |
| [Shared linear edits](../crates/foveonic/src/render/recipes/linear_edits.rs) | Color Editor, spatial HDR, Local Contrast and Universal sharpening precede camera response. | A common integration boundary already exists. Moving operations across it is a photographic change, not merely a refactor. |
| [DNG samples](../crates/foveonic/src/dng/writer.rs) | Unsigned storage with a common scale. Negative samples are clamped. | Signed intermediate data does not automatically survive export. |
| [DNG appearance](../crates/foveonic/src/dng/appearance.rs) | Fits camera presentation through profile data, with explicit headroom and tone-plateau handling. | A downstream mismatch can originate in storage, profile fitting or reader behavior as well as reconstruction. |

The present Oklab transform uses signed cube roots for numerical continuity. That extension does not establish perceptual accuracy for arbitrary negative or HDR samples.

## Highlight and output considerations

Missing sensor data, tonal compression and gamut mapping are different problems. Sensor clipping provenance describes the capture, while a display-white threshold depends on rendering. Darktable's highlight and view-transform modules illustrate this distinction.

Negative samples can arise from black-subtracted noise, color-basis conversion or calibration problems. An operation requiring a positive domain needs an explicit policy. Independent channel clipping does not provide a general gamut mapping solution.

Applying a full view transform after an existing camera shoulder can compress the image twice. Its nonlinear chromatic response cannot generally be represented by a scalar curve or a 3×3 matrix.

A DNG appearance mismatch can originate in reconstructed floating samples, stored samples, profile reprojection or reader rendering. [Capture One's DNG support policy](https://support.captureone.com/hc/en-us/articles/360003336397-DNG-Support) does not promise reproduction of another application's embedded edits. This policy does not identify which tag or stage causes a particular mismatch.

Spatial corrections and shoulders can be baked into linear-light RGB, but the samples then have modified scene relationships. Linear encoding is distinct from an untouched scene-linear signal. Baking more appearance can reduce downstream latitude and interact with a reader's own tone rendering.

## Automatic exposure references

ETTR seeks signal quality during acquisition. It does not imply that the brightest pixel should become display white or that every scene's average should become middle gray. Software exposure gain cannot restore signal lost to sensor clipping.

[Darktable Exposure](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/exposure/) documents percentile-based automatic placement and sampled-area exposure matching. [Capture One Exposure](https://support.captureone.com/hc/en-us/articles/360002602077-Adjusting-exposure-in-the-Exposure-tool) describes middle-gray and metering contributions to Auto. Neither establishes a universally correct target for low-key, high-key or backlit scenes.

Useful comparisons separate midtone placement from highlight rendering. A bright specular reflection and a large bright subject can require different treatment even when their maximum values match. Metering, view-transform choice and missing-data reconstruction therefore have separate roles.
