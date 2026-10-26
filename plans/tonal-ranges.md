---
summary: "Implement and evaluate coordinated Merrill HDR controls after researching Capture One tone mapping."
---

# Investigate tonal ranges

Requirements: [tonal ranges](../docs/specs/tonal-ranges.md), [processing graphs](../docs/specs/processing-graphs.md), [interactive rendering](../docs/specs/interactive-rendering.md), and [output strategies](../docs/specs/output-strategies.md).

## Work

- [x] Study darktable's actual tone-equalizer and fast guided-filter source.
- [x] Confirm in-app controls and defer DNG support with the owner.
- [x] Implement a shared GPU mask and linear RGB exposure adjustment for Merrill.
- [x] Add four experimental controls and mask/RGB inspection without changing zero output.
- [x] Verify cached editing, regional support, signed samples and explicit DNG behavior.
- [x] Evaluate photographs and measure completed processing cost.
- [x] Run repository checks and prepare the app for owner review, without committing.
- [x] Review range selectivity and Global/Spatial appearance with the owner. The photographic results were rejected.
- [x] Inspect Capture One's installed HDR engine and recover its four-control response.
- [x] Validate recovered response equations against the native Metal kernel on synthetic inputs.
- [x] Trace spatial filter scales, pivot inputs and settings-field normalization.
- [x] Validate coefficient arithmetic, base reconstruction and constant gain upsampling against native kernels.
- [ ] Compare a complete reconstructed spatial filter against application output on photographs.
- [x] Agree on the coordinated response, signed amounts and Global/Spatial comparison.
- [x] Replace the app experiment through the existing graph and cache boundaries.
- [x] Complete replacement checks, photographic comparisons and a local review build.
- [x] Review the new photographic response with the owner. Spatial was approved and Global rejected.
- [x] Normalize amounts to -1 through +1 and remove Global as a runtime choice.
- [x] Reproduce the numeric-field panel expansion and add a hover/edit regression.
- [x] Preserve the existing DNG pipeline while omitting HDR preview edits.
- [x] Complete the final checks and review bundle after these refinements.

## Current direction

The owner rejected the experiment's appearance and requested Capture One research instead. The four-control interface remains useful, but the fixed response anchors below are a rejected prototype rather than selected photographic behavior. The owner approved the spatial response after the Global/Spatial comparison, removed Global, and selected normalized -1 to +1 amounts. DNG must continue exporting through its existing pipeline while leaving HDR preview adjustments out. The owner subsequently approved committing this implementation with shared photographic finishing.

## Capture One findings

Research examined Capture One 16.6.3.6 application resources and the arm64 ImageCore binary. The Metal library has SHA-256 `b30ddb54b74ba922a53e93c758335e71cb84532f494329d26fe2a7b90e0806e4`. Internal symbols identify `CImgOpLocalToneMappingV8` and sixteen corresponding Metal passes. Embedded AIR bitcode can be extracted from the Metal library's bitcode wrappers and read with the installed Apple clang. CPU disassembly establishes parameter packing and calls between these passes. No application code was imported into the renderer.

The HDR stage is spatial. It constructs a logarithmic brightness map, estimates a filtered base through local moments, residuals and coefficients, and uses both the base and the original signal to compute gain. The broad shadow/highlight adjustments and the black/white adjustments share this calculation. Blacks incorporates the shadow gain. Whites incorporates the highlight gain. The final gain map is calculated on a half-width, half-height grid. Separable five-tap filtering reconstructs the sparse expanded gain map before multiplication into RGB. These final passes are principally gain upsampling, rather than an additional full-resolution blur. A separate clipping-recovery operation exists, so this tonal stage alone does not describe reconstruction of clipped channels.

The tonal pivot follows the inverse rendering curve at an output value of 0.25, divided by software gains and multiplied by `2^(-brightness/50)`. It is not a fixed 18% working-luminance anchor. Software gains include Exposure and camera/source gain handling. Objective-C settings type encodings, settings conversion and the HDR parameter pack establish these field identities.

### Recovered gain response

The following equations describe pass 13 with its default packed constants. Let `L` be original log brightness and `B` the filtered base, both in stops relative to the pivot. Let `D = L - B`. For an effective normalized amount `a`, the control coordinate is `X(a) = B + 0.5 D` when `a < 0`, and `B - 0.3 D` otherwise. This sign-dependent detail term is part of the response, rather than a separate sharpening control.

Define these scalar response functions:

```text
t(z) = 0.5 + 0.5 z (1 + |z|) / (1 + |z| + z²)
Q(z) = 0.5 max(t(z), z + 1 - t(z))
u = q / k
p = (u⁴ + u) (1 + u + u²)
C(q, k) = k p / (1 + p)
M(a, e) = 1 - |max(a, -1)| + |max(a, -1)| 2^e
```

For normalized shadow, highlight, black and white amounts `s`, `h`, `b` and `w`, compute:

```text
cs = C(Q(-1.8 X(s)), 2.1)
es = -cs                                      if s < 0
     cs + max(0, -0.0247487556 X(s)) D         otherwise
Gs = M(s, es)

ch = C(Q(1.8 X(h)), 3.0)
Gh = M(h, -ch if h < 0 else ch)

q = Q(-1.8 X(b))
A = C(q, 4.2) - C(q, 2.1)
eb = C(Q(-1.8 (X(b) + 1.666666746 + log2(Gs) + A)), 2.1) + A
Gb = M(b, -4 eb if b < 0 else eb)

ew = Q(1.8 (X(w) - 1.111111164 - log2(Gh)))
Gw = M(w, -4 ew if w < 0 else ew)

gain = Gb Gs / (Gw Gh)
```

For ordinary color rendering with Dehaze at zero, saved signed settings `S`, `H`, `B` and `W` map to these internal amounts as `s = S/100`, `h = -0.6 H/100`, `b = B/100` and `w = -W/100`. The public settings structure was recovered through its Objective-C type encoding and followed into the extended settings structure. Bundled style brushes provide a useful independent sign check: Highlights (recover) stores `HighlightRecoveryEx = -100`, Highlights (brighten) stores `100`, and Shadows (recover) stores `ShadowRecovery = 100`. These signed modern controls increase brightness with positive values. Legacy highlight-recovery descriptions do not establish their signed direction. Disabled control branches substitute unit gain.

The settings conversion adds part of Dehaze to the effective highlight and black amounts before HDR setup. A separate Dehaze operation also exists. Recreating this coupling alone would not implement Dehaze. Its color estimation, separate processing and controls remain outside this tone experiment. A separate inverted rendering mode changes the four settings' signs and association before this calculation. That mode is outside the ordinary color path studied here.

A standalone native Metal probe loaded the installed library and dispatched pass 13 on synthetic 16-bit log-brightness ramps. It compared the independently expressed equations above against 294,912 native outputs across 72 dispatches, using individual positive/negative controls, combined controls, and original-minus-base differences of -0.5, 0 and +0.5 EV. Maximum relative gain error was 0.00000274. Zero controls returned exactly unit gain. This validates the response arithmetic for those inputs. It does not establish full-image equivalence, photographic quality or end-to-end cost.

### Spatial filter

The RGB-input path initially forms a brightness estimate weighted 0.25/0.5/0.25, with a small separable spatial prefilter, then encodes logarithmic brightness in unsigned 16-bit samples at 4096 codes per stop. Another input path operates on the application's image pyramid. Their exact input-grid alignment and boundary treatment still need whole-image comparison.

The HDR filter is multiscale. At the native scale with its maximum configured radius of 192 guide pixels, the orchestration visits radii 1, 3, 6, 12, 24, 48, 96 and 192. Radii above 15 use a further grid reduction by four and resize the filtered coefficients back. The radius follows render scale and some source types rather than a fixed percentage of image dimensions.

Each iteration derives local mean brightness, its second moment and an absolute residual against a smaller neighborhood. It averages that residual, uses it to limit local variance, calculates edge-preserving coefficients, averages the coefficients, then updates the accumulated filtered base. The existing base is multiplied by the averaged slope and receives the averaged intercept. It does not restart from the original signal for every scale.

Pass 5 exposes the exact fixed-point coefficient calculation. Here `mu`, `second` and `residual` are its unsigned 16-bit input samples. All intermediate arithmetic is unsigned 32-bit, with wrapping subtraction:

```text
variance = (second << 16) - mu²
v = min(variance, residual << 15)
q = v / ((v >> 16) + epsilon)
a = min(q + (q >> 3), 65535)
b = ((65536 - a) mu) >> 16
```

The radius loop selects sigma codes 3686 for its largest scale and 1843 for the remaining scales. Squaring and shifting by 16 gives regularizers 207 and 51. The one-eighth slope boost and residual limit distinguish this filter from the prototype's standard `variance / (variance + epsilon)` guided filter. Pass 8 then reconstructs the base as `min(((a * previous_base) >> 16) + b, 65535)`.

A second native Metal probe verified 49,152 coefficient pairs and 49,152 reconstructed samples with exact integer equality, covering regularizers 1, 51 and 207. It also verified that passes 10 and 11 with default taps `[0, 0.25, 0.75, 0.75, 0.25]` reconstruct a constant sparse half-grid gain with zero error. Those taps sum to two because the source has inserted zero samples. They must not be interpreted as an ordinary unit-normalized blur over dense gain samples. Complete multiscale orchestration and photographic agreement remain unvalidated.

### Selected replacement

The owner approved the spatial response after comparing it with Global. Use the recovered coordinated response and multiscale base with normalized -1 to +1 amounts. Positive values brighten the selected range. Global was removed from the runtime choices and shaders after photographic review.

Derive the pivot from our own camera tone curve and Exposure. Reuse graph/session boundaries for unchanged linear RGB and the spatial base. Retain neutral bypass, signed/headroom samples, inspection and regional consistency. Dehaze remains a separate potential feature. DNG retains its existing samples and appearance and omits these HDR preview adjustments.

## Rejected first experiment

Read darktable revision `27276e5a4087d531579e591fc80702597617822d`: [`toneequal.c`](https://github.com/darktable-org/darktable/blob/27276e5a4087d531579e591fc80702597617822d/src/iop/toneequal.c) and [`fast_guided_filter.h`](https://github.com/darktable-org/darktable/blob/27276e5a4087d531579e591fc80702597617822d/src/common/fast_guided_filter.h). These files are GPL-3.0-or-later. The experiment independently implements the published guided-filter equations in WGSL and credits the source study. It does not import their implementation.

Darktable estimates brightness, constructs a reduced-resolution guided surface blur, and applies one multiplicative exposure correction to every RGB component. Its nine exposure anchors use a radial-basis interpolation. The rejected smaller experiment used four controls with a neutral middle-gray anchor, smooth bounded interpolation, and a log-brightness guide in known working RGB primaries. That response was inspired by darktable rather than an exact port. The guided-filter reference is [He and Sun, Fast Guided Filter](https://arxiv.org/abs/1505.00996).

The existing Shadows/Highlights controls adjust camera sigmoid endpoints by only one eighth of their numeric range in stops. They reshape the camera curve rather than selecting isolated brightness ranges. Preserve those saved settings while evaluating the new controls.

## Implemented replacement

The app retains the four experimental controls, now with normalized signed amounts between -1 and +1. Zero omits both HDR nodes. Tone bypass preserves the saved amounts. Existing camera Shadows/Highlights remain under Camera curve. Active HDR edits do not block DNG export. The existing technical base and appearance omit HDR, so there is no additional baking or appearance composition. TIFF uses the same photographic graph as the viewport.

Spatial averages linear brightness onto a half-width, half-height guide, constructs the multiscale base, evaluates the coordinated gain on that grid and reconstructs gain bilinearly before multiplying original RGB. Odd dimensions clamp the final cell. Working-space luminance uses the known sRGB or Adobe RGB primaries, rather than Capture One's fixed brightness mix. Coarse guide cells average their source luminance before taking the logarithm. The implementation uses floating EV moments, regularizers 0.45² and 0.9² EV², and replicated boundaries. At the finest one-cell radius, the residual limit uses the native maximum-residual case because no smaller neighborhood exists. These are explicit spatial implementation choices, not claims of bitwise equality with the application's fixed-point pyramid path. No proprietary shader implementation is imported.

The pivot inverts our generated camera tone table at 0.25 and removes the pending Exposure gain. The spatial base is independent of the curve, Exposure and the four amounts, so edits to those settings can reuse it. Graph keys include the resulting pivot in the gain node. Selective base inspection stops before later tone metadata preparation. No standalone Brightness or Dehaze setting was added.

`tonal-ranges.mask` exposes the base with display black at -8 EV and white at nominal white. The Spatial inspection field holds half-grid base values over each corresponding two-pixel cell. `tonal-ranges.rgb` supports the existing Input/Output and Difference workflow before the camera curve. The bounded active session retains unchanged linear RGB and the base. Temporary bases alternate exclusive buffers during the scale loop.

Regional Full processing expands by the largest coefficient, residual and interpolation support, aligns to the eight-pixel coarse source grid, and keeps the native radius of 192 guide pixels. Fast halves that radius with its working grid. Each iteration derives coefficients from the original guide and updates the accumulated base pointwise. Support therefore follows the largest neighborhood rather than summing every scale. The larger support can reduce regional latency savings when HDR Spatial is active. A whole-frame guide cache can be evaluated separately if measurements justify changing that boundary.

## Replacement evidence

Synthetic GPU tests compare individual flat-range gains and combined-control responses with signed detail against native Metal outputs. They check exact neutral aliasing, input immutability, finite signed/headroom samples, shared RGB gain, texture suppression, strong-edge retention, camera-curve pivot behavior and aligned regional bases. Original-file tests check fresh/cached equality, reduced framing, monochrome, inspection, Tone bypass and regional/full photographic agreement.

The one-off tonal-range comparison tool was removed after photographic approval. General completed-processing measurements and exact output comparisons use `profile_pipeline`, with `--tone-method universal --hdr-shadows 0.5 --hdr-highlights -0.5`. Source decoding and export remain separate from rendering measurements.

The spatial response has owner photographic approval. Further DNG HDR support remains deferred. The owner subsequently approved committing this implementation with shared photographic finishing. Release remains separate.

## Completed checks and measurements

Before the final normalization and DNG refinements, all 589 ordinary workspace tests passed. Five targeted numerical GPU tests and two portrait recipe tests pass serially. Strict workspace Clippy, formatting, documentation/index checks, diff whitespace checks and all five artwork tests pass. The macOS review bundle builds successfully. No installation, commit or release was performed.

On Apple M1 Pro, completed render-plus-view medians over five rotated trials were 826/840/930 ms for neutral/Global/Spatial on the portrait, and 641/651/735 ms on foliage. Fast medians were 489/500/528 ms and 312/318/339 ms respectively. Cached amount changes had medians of 79/78 ms for Global/Spatial on the portrait and 76/72 ms on foliage. The spatial base and denoising did not rerun for those edits. These measurements exclude source decoding and export and have uncontrolled background activity. They indicate roughly 94–104 ms of initial Full Spatial work and 27–39 ms of initial Fast Spatial work on these two photographs, rather than a cross-machine latency guarantee.

Visual review covered faces, hair, knitted fabric, sky, glossy leaves and dark background at Shadows +0.5 and Highlights -0.5. Spatial preserves visibly more texture than Global at those settings. The local toggleable comparison includes both photographs and neutral output. The owner approved Spatial and rejected Global after this comparison. Complete application-to-application Capture One spatial equivalence has not been established.

## Final UI and export decisions

The wider percentage values could exceed the fixed numeric field and expand the editor panel. A headless three-panel reproduction grew the editor from 330 to 333 points after a negative slider edit without dragging its divider. The normalized -1 to +1 range fits the existing input fields. A regression walks the pointer across the editor through positive, negative, extreme and neutral edits and checks unchanged panel width. Splitter behavior itself was not changed.

Earlier experimental sidecars have unversioned percentage amounts and may contain a Global/Spatial choice. Deserialization converts those amounts once and drops the retired choice. New HDR settings serialize with version 1 and normalized amounts. No migration rewrites source files by itself.

The DNG technical-base regression includes nonzero HDR preview amounts and checks exact equality with the original technical pixels. Existing DNG appearance composition remains separate and receives no HDR curve or spatial baking.

## Final verification

After normalizing and removing Global, all 589 ordinary workspace tests and strict workspace Clippy pass. Four targeted GPU response/base tests, two original-photo recipe tests and the DNG regression pass serially. The DNG regression verifies identical technical samples, mapping/neutral/forward/color matrices, tone curve, look table and baseline exposure with HDR amounts present versus absent. The panel-width regression passes across the full normalized range and pointer movement. Formatting, index/document checks and whitespace checks pass. The final macOS review bundle builds successfully. The owner subsequently approved committing this implementation with shared photographic finishing.
