---
status: archived
summary: "Ordered camera repair fixes the leaf defect; the node audit led to the approved WB-scale fix and identifies remaining camera coverage gaps."
---

# Fix camera repair and audit implemented nodes

## Closure

The owner accepted the implemented photographic and interface results and authorized closing review-only plans. This repair/audit slice is complete. The audit records the implementation at that checkpoint, not today's remaining stage inventory. Subsequent camera coverage, reconstruction, and recipe integration are recorded in the active [algorithm completion plan](../complete-reference-algorithms.md).

The owner approved fixing the remaining leaf defect and auditing implemented-node logic. Requirements: [processing graphs](../../docs/specs/processing-graphs.md), [detail and noise](../../docs/specs/detail-and-noise.md), and [despeckle](../../docs/specs/despeckle.md).

## Steps

- [x] Replace per-pixel merged flags/isolated averaging with ordered `BadPixelsF20` records, original masks, trimmed means, and chroma anchors.
- [x] Preserve immutable input/artifact ownership, shared cutoff controls, selective execution, and GPU-only photographic arithmetic.
- [x] Compare synthetic masks/order/borders/zero sentinels to the read-only reference and regress the reported 2×2 leaf defect.
- [x] Render/review six originals, default and reconstructed routes; verify both corrections disabled retains the earlier neutral baseline.
- [x] Audit every implemented operation's preparation, arithmetic, boundaries, and orchestration against reference code and agreed scope.
- [x] Separate unintended omissions/bugs from approved alternatives, documented departures, and unimplemented stages; record evidence and propose follow-up scope.
- [x] Run workspace/GPU tests, native launch, Clippy, formatting, and documentation/index checks.
- [x] Owner authorized grouped commits after code and automated checks.
- [x] Owner accepted the implemented photographic and interface results and closed the review checkpoint.

## Scope

The replacement covers the existing main camera-record node, not a complete native conditioning stage. JPEG repair lists, original-signal bad-red detection/overrides, distinct `BadClustersF20` handling, highlight restoration, row/column/pattern conditioning, and high-ISO reduced processing were assessed rather than silently added. The leaf block is represented in `BadPixelsF20`; ordered masked repair corrects it without changing detection thresholds or adding photographic controls.

The audit used a reference implementation source snapshot based on revision `e47af35`. The owner authorized grouped commits after code and automated checks.

## Repair and evidence

Preparation preserves metadata order, duplicates, original neighbor masks, and cutoff selection. The GPU initializes a separately owned output from the input and applies records sequentially, reading prior repairs. All seven layer combinations use reference trimmed means and unaffected-channel anchors. Native asymmetric border rules and genuine-zero extrema sentinels are preserved. Empty selections still copy the input. Scratch buffers, including recycled uploads, support `COPY_DST`; this fixes the validation error exposed by copy initialization.

- `ordered-repair-reference.json` and [repair tests](../../crates/foveonic/src/nodes/sensor/repair_test.rs) cover masks, duplicates, adjacency, reversed order, cutoff, borders, zero sentinels, unknown layer flags, gains 0/1/1.3, and immutable artifacts.
- The leaf's four red samples change approximately `551/1229/1210/766` → `315/321/324/317` in GPU calibrated units. Green/blue remain unchanged; subsequent despeckling leaves the repaired block unchanged.
- Default, reconstructed, and disabled routes each produced six TIFFs. Every disabled TIFF is byte-identical to the preceding neutral baseline.
- Native leaf review captures the enlarged magenta block before correction and its disappearance afterward, plus reconstructed/default views. Six complete 18-frame photograph runs exercise reconstruction intermediates and controls (114 complete captures including the leaf run). The first yellow-image run hit the enclosing command's time limit after four captures; a fresh rerun completed successfully. Owner visual review remains pending.
- Review covered six whole-image and six native-crop comparisons: prior correction defaults / ordered-repair defaults / ordered-repair reconstruction, left to right. The broad look is unchanged by the camera fix; reconstruction reduces color noise. Clipped flower highlights and some bright/magenta shadow points remain, so this is not a claim that every photographic defect is corrected.
- Full-sensor reference replay uses identical GPU inputs and current explicit parameters on all six originals. Maximum camera-repair error is 0.001465 camera codes; maximum despeckle error is 0.000977. This checks bank ordering and every sample, not just selected defect centers.
- Calibration comparison against the raw-WB reference diagnostic accounts for production's explicit 2× WB. Active-area mean absolute errors span 0.00053–0.01668 camera codes; maximum errors span 0.31–2.61, with maximum relative error below 0.000274 using a 100-code denominator floor. Direct float32 spatial interpolation differs slightly from the reference's incremental/double arithmetic; no missing WB scale was found in calibration itself.
- Release workspace tests pass: core 35 ordinary tests, CLI 2, GUI 26, plus documentation tests. All 26 ignored GPU/photograph/worker tests pass. Release all-target Clippy with `-D warnings`, workspace build, documentation generation/check/tests, formatting and diff checks pass. Release-profile checks avoid rebuilding the development artifacts removed with owner approval during disk cleanup.
- Owner-approved cleanup removed historical optimization TIFFs, its old executable, and development build artifacts, preserving logs/helpers and current neutral baselines. Temporary review generation initially exceeded a command time limit when compiled without optimization; the optimized helper completed all twelve review images. No reference files were changed.

## Implemented-node audit

Each row includes metadata preparation, numerical stages, boundary behavior, dependencies, selective roots, and retained ownership where applicable. Numerical fixtures complement source comparison; they do not establish photographic equivalence for every camera or setting.

| Operation | Audit result |
| --- | --- |
| Source / RenderedInput | Merrill geometry and encoding checks, typed imports, engine identity, retained source/metadata and imported buffer ownership agree with current scope. Six originals decode to finite integer codes in 0–4095. |
| Black | Dark-shield geometry, histogram initialization, trimming, extrema removal, and mean arithmetic checked against reference `black.rs`; small GPU fixture covers outliers. Current originals fit the unsigned 12-bit histogram domain. |
| Calibrate | WB/aperture/temperature/sensor factors, spatial table routes/interpolation, black subtraction, shading and signed/headroom preservation checked against `gains.rs`, `spatial_tables.rs`, and preprocessing. Full-source differential evidence above. |
| SensorGain | One finite nonnegative scalar after calibration; negative samples/headroom retained. Zero gain and stage ordering covered by GPU tests. |
| Repair | Main ordered `BadPixelsF20` arithmetic now agrees with reference replay. Full native setup/list coverage remains incomplete, as detailed below. |
| Despeckle | Table arithmetic, immutable extrema detection, peak clipping/negative valleys, margins, deterministic eight-bank capacity/order, and sequential correction agree for identical inputs/parameters. The subsequent WB-scale fix shares actual calibration factors; production noise-recipe adjustment remains deferred. See below. |
| NoiseBase | Effective ISO, camera knots/presets, gain-scaled noise model, 4×4 tiles/64-pixel context, adaptive bilateral, edge tables, opposing recursive means and initialized borders checked against reference noise/filtering/local-mean code and fixtures. Full-resolution path only. |
| Detail | Weighted brightness, noise/diffusion maps, iterative diffusion, extrema limiter, ratios and residual gains follow reference stages. Component/scalar fixtures and negative/zero/extreme-control finite checks pass. Preset 0 remains a filtering recipe, not Off. |
| TiledColor | Uses the same prepared camera/WB/ISO/mode matrix as untiled conversion on filtered layers; tile geometry and selective dependencies checked. |
| TiledColorDq | Uses the same explicit-gain signed periodic lookup as untiled ColorDQ; no second automatic-gain path. |
| Reconstruct | Reference residual mixing/restoration, ratio/detail-strength application, initialized context and overlap projection checked against fixtures. Signed 3×3 high-pass is inspectable but not applied to final output. |
| Color | WB correction, processing/capture/sensor ISO behavior and selected color-mode compensation checked against reference. GPU matrix arithmetic is float32 rather than native double. |
| ColorDq | Metadata selection, explicit amplitude gain, signed wrapping/rounding/table indexing checked against reference and shader fixtures. Float32 matrix/rounding can differ near integer thresholds; bit identity is not required. Native automatic gain remains deferred. |
| Tone | Camera tone-row ordering, exposure, curve interpolation, endpoint/contrast controls and negative/high-end policy checked against reference tone/color-mode preparation and finish behavior. |
| Saturation | Reference-style brightness weights and one post-tone saturation blend; neutral bypass and typed color-space handling checked. |
| Blur | Two normalized separable Gaussian passes with clamped borders; small convolution fixture passes. Approved sharpening alternative, not reference conditioning. |
| Sharpen | Post-tone unsharp blend reads immutable original/blurred inputs; neutral bypass and amount arithmetic checked by fixture. Not the reference reconstruction high-pass application. |
| Encode | Active crop, inclusive-to-exclusive endpoints, all quarter-turn orientations, pixel-center bilinear resize, transfer function and RGB16 packing checked by geometry/GPU fixtures. Bilinear minification is the agreed initial alternative. |
| Tiff | Dimensions, packed-channel extraction, ICC/profile selection and baked WB/color/tone checked against current output requirements. Neutral disabled-correction baselines remain exact. |

Texture preparation, logical cropped/oriented views, component inspection and scopes were also checked for signed/ratio/camera-code mappings, luminance coefficients, finite/under/over diagnostics, geometry, and ownership. Graph fan-out/fan-in and selective stage preparation have lifetime/early-root tests. The repair replacement retains the input until its copy and never mutates retained preceding artifacts.

## Remaining gaps and proposed follow-ups

### Camera correction coverage

Ordered arithmetic is fixed, but the node still consumes only the main camera list. Native `repair_setup.rs` also assembles `Jpeg_BadClusters`, original-signal bad-red detections/override masks, and separate packed `BadClustersF20` repairs. These are previously deferred scope, not defects in the new ordered-record arithmetic.

All six originals share 143827 main records, of which cutoff 2 selects 41093. They also contain 41 JPEG-list records and 233 packed cluster values; the latter is not a count of clusters. Main-list regression success therefore cannot establish complete camera defect coverage. Proposed next correction scope: reproduce native setup/order and distinct cluster handling with inspectable inputs, keeping the existing independent switches.

### Despeckle WB scale and recipe coupling

At audit time, `despeckle::prepare` used raw/unscaled WB factors and a fixed 1× adjustment while calibration applied 2× WB. Reference production passes its adjusted WB into both calibration and threshold generation and supplies a noise-derived despeckle scale. The earlier diagnostic-based departure was documented, but applying diagnostic thresholds to a differently scaled calibrated signal made that detector more aggressive than a scale-matched detector.

Holding repaired input, margins and `[15,15,5]` adjustment fixed, native detector replay finds:

| Photograph | Pre-fix 1× WB thresholds | Scale-matched 2× WB thresholds |
| --- | ---: | ---: |
| Blue sign | 10120 | 4800 |
| Leaves | 16571 | 7061 |
| Rocks/shadows | 100348 | 62632 |
| Red flower | 6339 | 3164 |
| Turquoise still life | 14859 | 4742 |
| Yellow flowers | 29522 | 16597 |

These counts isolate WB scale, not complete production preprocessing, and do not classify every detected point as a defect or useful detail. The owner subsequently approved sharing actual calibration WB factors with threshold preparation, implemented in the [WB-scale fix](./despeckle-wb-scale.md). Sensor-gain invariance and zero-gain behavior are preserved. Noise-recipe adjustment remains a separate decision.

### Reconstruction completeness and output alternatives

Reference final high-pass application, highlight restoration, row/column/pattern conditioning, and reduced high-ISO processing remain absent. Reconstruction's existing full-resolution stages match their fixtures, but this is not the complete native recipe. Gaussian post-tone sharpening, bilinear resize, explicit ColorDQ gain and float32 photographic arithmetic are established alternatives/departures, not newly discovered skipped operations. True noise Off/strength and high-pass integration need owner decisions.

DNG and monitor-profile handling remain future output/presentation work, not implemented-node logic failures. No further ordering, border, typing or retained-buffer defect was established beyond the repaired camera kernel and exposed copy-usage issue. Evidence covers six DP2 Merrill photographs plus synthetic/metadata fixtures; DP1/DP3/SD1 runtime and broader high-ISO/extreme-scene coverage remain review limits. Follow-up implementation requires owner approval.
