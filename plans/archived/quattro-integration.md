---
summary: "Implement Quattro input, calibration and reconstruction through family-owned multiresolution recipes."
status: archived
---

# Quattro integration

The owner approved the DP2-first implementation sequence after reviewing the integration assessment. Begin with decoding and independent-resolution plane inspection, then add calibration and reconstruction incrementally. Preserve the [recipe and numerical-operation boundaries](../../docs/specs/classic-rendering.md) established by Classic and the shared [output requirements](../../docs/specs/output-strategies.md).

## Assessment

- [x] Review existing decoder and reference processing evidence.
- [x] Identify equal-resolution assumptions in source loading and execution.
- [x] Separate reusable infrastructure from different numerical algorithms.
- [x] Propose the first photographic slice and validation sequence.
- [x] Start the DP2-first slice with independently validated GPU boundaries and immutable calibration maps.
- [x] Implement the initial DP2 slice incrementally.

## Organization before reconstruction

The owner approved a focused organization pass before extending the pipeline. Keep temporary rendering availability simple. Prioritize fewer shared-code edits when adding numerical stages.

- [x] Separate Quattro graph wiring from calibration preparation and its inspection catalogue.
- [x] Centralize image buffer layout and coordinate-view contracts used by execution and presentation.
- [x] Group Merrill-only technical and output builders under its family entry point.
- [x] Verify selective plane requests, retained values and existing rendering behavior after the refactor.

Merrill, Classic and Quattro have separate entry points under `render/recipes`. Quattro graph composition consumes numerical calibration parameters and recipe-owned inspection descriptions. It stops at completed requested boundaries instead of selecting dependencies through output-name prefixes. Calibration preparation receives source metadata and an immutable resource snapshot, without depending on the engine. The default snapshot uses bundled camera maps. A supplied directory is an explicit override.

The graph owns the mapping from signal types to image descriptions. Image contracts own canonical buffer layouts and whether inspection uses the full physical plane. The executor, artifact collector, viewport and scopes reuse those decisions. Later stages should reuse existing signal types when units and component meaning agree. Add a signal type when that contract changes, rather than for every intermediate stage. Output names distinguish retained histories that have the same signal contract.

After this pass, 354 ordinary workspace tests and the targeted GPU checks pass. Eight Classic/Merrill TIFFs remain byte-identical, and the complete DP2 blue-map graph still matches its native oracle exactly. GPU checks also cover scalar blue scopes, retained siblings and raw inspection with invalid later calibration metadata. Canonical channel selection fixes blue scopes previously advertising three channels. Processing equations remain unchanged.

The numerical readability pass labels map aperture knots, focus-bank ordering, calibration ceilings and Stage1 control tables. Recovered coefficients retain their exact types and arithmetic order. Stage1 comments explain the exposure-score, ISO strength fit and ambient-EV mapping without assigning unverified physical units. The extraction clip and phase-gain cutoff have separate names because only the former scales with sensor ISO. Adjacent Classic and Merrill preparation also identifies ISO-ratio noise fallback, row-filter decay in rows and fixed saturation weights.

## Input and dispatch

The existing source owns one interleaved RGB array. Quattro decoding produces six samples per low-resolution cell: R, G and four blue phases. Calibration produces low-resolution RGB and a separate blue plane at twice its width and height. Source storage now identifies this layout and its extent explicitly. Keep this storage choice at decoding/upload boundaries rather than adding camera-family checks to numerical operations.

The acquired DP2 Quattro uses format35, sd Quattro format37 and sd Quattro H format39. Their entropy decoding shares bounded category/predictor machinery but needs independently sized planes and a larger category table than format30. Format35's extra blue columns supply replacement samples through a remapping rule. Cropping them away would corrupt decoded data. Independent decoding already matches 175,371,264 reference Float32 words across five acquired originals.

Add X3F4.1/4.2 and CAMF5 support. CAMF5 has a byte-difference recurrence and feeds the existing typed metadata parser. These originals lack PROP sections, so both full loading and lightweight capture reading need consistent preview-Exif/maker-note fallback. Preserve the newer header's explicit processing version separately from camera labels and storage encoding.

Quattro and Tiger share plane geometry but differ in calibration, gains, highlights and some stage controls. The first public family panel can cover Quattro cameras, while recipe-local preparation selects the actual processing variant. A format number alone must not select the rendering equations.

## Shared engine changes

`image::Description` already carries extent, layout and sample meaning for each image. Graphs already have side outputs, selective dependencies and immutable retained values. Reuse those facilities.

The concrete changes are:

1. Give `Source` an explicit storage-layout contract. Add a source-only packed layout or expose the physically separate planes. Prefer preserving the decoder's packed data until a numerical extraction operation needs it.
2. Let extraction publish low RGB and high blue with distinct descriptions. The executor currently inherits one extent from its first live input and uses it for most outputs. Operations that change resolution need explicit per-output descriptions.
3. Validate operation-local geometry. Fusion requires matching low-resolution histories and masks, with high blue exactly twice the low extent. Avoid a general shape-inference system before these operations require one.
4. Retain calibrated RGB, Stage1 RGB and Stage2 RGB as separate immutable graph values through fusion. Native in-place pointer aliases express scratch reuse, not ownership contracts to reproduce.
5. Keep sensor coordinates, active crop and presentation dimensions distinct. Inspect low and high planes using their own dimensions and coordinate mappings.

Validate storage limits and live allocation sizes before promising SD Quattro H support. Its 6656×4480 Float32 RGB buffer alone needs about 341 MiB. Native unsigned RGB16 boundaries may fit the existing packed RGB16 layout, but generic floating finishing still needs a supported representation. Use adapter limits and measured lifetimes to decide whether tiling is necessary.

## Proposed recipe structure

Use a Quattro-authored recipe with preparation grouped by calibration, reconstruction, geometry and appearance. Select routes there and pass functional parameters to camera-unaware operations. No callback framework is needed.

| Boundary | Explicit graph values |
| --- | --- |
| Decode and extraction | Packed source, low RGB, high blue |
| Calibration | Corrected low RGB and high blue, defects and masks |
| Stage1 | Low-resolution filtered RGB |
| Stage2 | Low-resolution processed RGB |
| Fusion | All three RGB histories plus high blue and masks, producing full-resolution RGB and signed detail |
| Spatial appearance | Local contrast, detail and crop with their own signal and geometry contracts |
| Pointwise response | Prepared integer LUT response, followed by a defined normalized linear RGB boundary |
| Output | Shared destination conversion, transfer, display and TIFF packaging |

Quattro UV filtering, blue filtering, residual reconstruction, wavelet/local-contrast and proportional sharpening have different equations and units from Classic FIR/range reconstruction and Merrill reconstruction. Implement separate functional operations where required. Reuse resampling, matrix or LUT machinery only after checking arithmetic, rounding and borders. The reference has three materially different resampling contracts.

## First useful slice

Start with the acquired DP2 Quattro original, ordinary single-frame full-resolution color, fixed Sunlight WB and Standard mode. Its existing calibration and downstream evidence gives a consistent first target. Explicitly select noise/detail controls, neutral tone/editor/Retinex and omit optional color sharpening initially. Do not label a diagnostic bypass route as the reference default.

1. Validate decoding on acquired DP0/DP2/sd Quattro/sd Quattro H independently of photographic support.
2. Expose low RGB and full-resolution blue through inspection, with verified geometry and signal units.
3. Prepare and validate resource-enabled calibration, defects and masks for DP2.
4. Add Stage1, Stage2 and fusion incrementally against immutable named captures.
5. Reach a native-resolution photograph through the shared finishing boundary and TIFF writer.
6. Add ordinary spatial detail/local contrast, then establish the editable DNG technical boundary and appearance.
7. Extend recipe preparation to sd Quattro and Tiger variants after the first complete route is reviewed.

Defer monochrome, reduced-size processing, neural upsampling and SFD/X3I during this first slice. They have separate reference routes and validation requirements.

## Decisions and remaining evidence

- **Calibration resources:** The owner approved bundling the exact 24 blue-calibration maps for identifiers 80 through 83, with source provenance and file hashes. The default engine uses those maps without an installed reference application or environment setup. An explicit calibration directory remains an optional immutable override. The controlled equations and resource-enabled complete DP2 composition are validated. Preserve explicit errors for incomplete supplied map sets instead of selecting the missing-resource diagnostic route.
- **Parameters and captures:** replace captured request fields with source-derived preparation. Preserve distinct entry states when native processing mutates earlier surfaces.
- **Precision:** several native stages mix Float32, Float64, FMA and integer wrapping. Prepare tables on the host, compare Float32 GPU implementations with complete fixtures, then decide where paired arithmetic or measured approximations are warranted.
- **Borders and scheduling:** specify complete-image borders, strip halos and deterministic initialization. Default level6 wavelet processing has native scheduling variation. Bounded deterministic fixtures do not establish full-image parity.
- **Photographic response:** retain integer gains, tone/editor order and output-matrix arithmetic in a functional response operation. Then join generic linear color conversion and transfer encoding, checking the reference gamma convention to avoid double encoding.
- **DNG:** establish scene-linear scale, clipping and the storage color basis before using the shared fitter/writer. A pre-tone RGB16 surface is not sufficient evidence of scene linearity. Spatial Retinex cannot generally become a global DNG look.
- **UI:** add a family panel and capabilities after the supported controls are demonstrated. Decode and capture metadata must identify the family early. Shared widgets do not imply identical slider meaning.
- **Oracle coverage:** many workers and bounded stage compositions are independently exact. Whole-original replacement parity, matched application TIFFs, complete resource-enabled calibration composition and default level6 local contrast remain unverified.

The authorized reference checkout contains the detailed decoder, dispatch, calibration, Stage1/2/2B/3, finishing and backend research notes. Those notes distinguish static recovery from executed native evidence. This plan summarizes the integration implications without depending on machine-local captures.

## Implemented input and calibration boundaries

The [input and inspection specification](../../docs/specs/quattro-input.md) defines this slice. All five acquired originals decode exactly against independent native sensor captures: 175,371,264 Float32 components and 16,660,568 CAMF bytes. Full loading and bounded capture reading agree on camera properties, including originals without PROP. Processing versions and packed source storage are explicit.

The GPU exposes `sensor.rgb` and `sensor.blue`. Supported Quattro processing versions additionally expose `extraction.rgb` and `extraction.blue`, with offset/scale correction and high-blue clipping. Complete DP2 extraction matches all 37,836,288 independently captured RGB/blue values exactly. Tiger raw inspection is available, while its different gain-bearing calibration remains unimplemented.

Blue maps from the engine's immutable resources are blended from source aperture/focus metadata. The bundled files provide the default map set. All 84 tested native grids, totaling 1,792,896 values, match exactly. `phase-gain.blue` applies these maps to the extracted blue as an isolated correction boundary. Five bounded fixtures and the complete 21,620,736-value DP2 blue image match the native operation bit for bit. This does not claim complete preprocessing: other reference corrections still belong between extraction and reconstruction.

Paired Float32 arithmetic requires explicit FMA rounding boundaries on the tested Metal backend. Ordinary arithmetic allowed the compiler to collapse the compensation and changed several million blue components. The measured plain-Float32 alternative differed by at most 0.001953125 blue codes. The retained paired implementation matches the native Double resize before its final Float32 output exactly in the checked fixtures.

Graph tests exercise each plane alone, mixed raw/corrected requests, 90-degree view geometry and immutable retained siblings. A complete source-derived resource-enabled graph output also matches the native correction. At the input boundary, the GUI showed unavailable Final and permitted explicit plane selection with each plane's own dimensions and a 14-bit display scale. The later color slice below adds photographic output for its demonstrated camera route.

Eight complete Sunlight/Standard TIFFs remain byte-identical to the retained pre-Quattro build: SD9, SD10, SD14, SD15, DP1, DP1s, DP2 and Merrill. Merrill also matches the retained original-main executable. On the development GPU, warm full-DP2 extraction takes a median 2.17 ms and blue-map correction 4.27 ms. These timings exclude decoding, transfers and host preparation.

The foundation retained Stage1 parameter derivation as a tested preparation study using actual source exposure and explicit midpoint noise steps. The color slice below adds complete preprocessing and numerical execution with separately selected controls. Photographic rendering and DNG remain independent milestones.

The next comparison target is an owned full-frame capture at successful PREPROCESS return, with all native threads stopped and the process terminated before Stage1. Three resource-present runs produce identical RGB and blue hashes. Native map-loader return hooks confirm the selected DP2 map payloads were actually loaded. An otherwise identical empty-resource request records null map loads. Both outputs are finite. Resources change 5,491,327 RGB components and 21,215,845 blue values. Mean absolute differences are 0.0133 RGB codes and 2.5605 blue codes, with maxima of 694.60 and 582.96 respectively. Missing resources therefore cannot stand in for resource-enabled calibration. The request retains explicit diagnostic exposure and control settings. This establishes a stable preprocessing target, not application export parity or a completed replacement calibration pipeline.

## Bundled calibration resources

The approved default is the exact 24 `C80` through `C83` map files from the reference implementation. Their [provenance and hashes](../../crates/foveonic/assets/quattro/README.md) stay beside the bundled files. Applications use these resources automatically. The existing explicit directory API can replace the bundle with an immutable snapshot, without changing recipe or numerical-node contracts.

- [x] Embed and validate all 24 source map files with their provenance.
- [x] Verify default rendering and blue-map inspection without calibration environment configuration.
- [x] Preserve explicit-directory override checks and incomplete-map errors.

Bundled-resource verification passes: 383 ordinary workspace tests and strict Clippy. A full DP2 TIFF rendered from an unrelated working directory with the calibration environment variable unset is byte-identical to the earlier explicit-resource render. Default blue-map inspection matches the complete native gain capture exactly. The rebuilt GUI embeds the same map bytes.

## Initial color pipeline

The foundation was committed separately. The next slice composes the ordinary full-size DP2 Quattro ISO100 photograph, under the [color-rendering specification](../../docs/specs/quattro-rendering.md).

- [x] Complete source-derived HLP calibration and mandatory sparse, RGB and blue repairs.
- [x] Preserve the independent factory mask, sparse flags and highlight mask.
- [x] Compose Stage1 and retain the source after its first WB round trip.
- [x] Compose spatial color compensation, raw-color filtering and multi-axis correction.
- [x] Reconstruct full-resolution color and signed detail from RGB histories and blue.
- [x] Compose packed Lab, wavelet local contrast and proportional sharpening.
- [x] Reproduce the neutral Standard response and join shared linear finishing.
- [x] Export the complete active image as a 5424×3616 TIFF.
- [x] Finish connected-boundary comparisons, UI availability and workspace regressions.

The first recipe explicitly selects noise steps zero, ordinary zero blue-noise blend, local-contrast level six and neutral tone. These are a demonstrated diagnostic route, not a claim about application export defaults. Rendering is limited to fixed Sunlight/Standard color until other controls are demonstrated. Preparation stays beside each stage. Numerical nodes receive grids, matrices, tables, bounds and functional stages. The engine executes typed histories without camera-family branches.

Recorded WB selects early HLP shading. Factory repairs use the Auto calibration bundle. Stage1 uses fixed Sunlight gains, while the selected editing WB belongs to later color conversion. These choices cannot be collapsed into one global WB setting. The native sparse-repair mask and the initially empty factory mask are also different objects. A connected-graph comparison caught their accidental reuse even though the isolated numerical workers passed. Keeping those histories explicit removes the resulting skipped repairs.

Full native checks establish exact blue refresh within logical rows, sparse RGB/blue repair, ordered blue repair, blue despeckling, RGB despeckling, RGB/luma clusters, spatial RGB shading and per-cell blue shading. The complete 21,620,736-byte high highlight mask matches an independently resized low mask exactly. Metadata-derived multi-axis controls match all 940 native bytes. Complete Stage1 differs by at most 0.001953125 codes in the interior and retains its source exactly when given native input. Stage2's complete composition differs by at most 0.016974926 codes, with mean error below 0.000001 code. Comparisons use stopped successful boundaries rather than in-flight engine caches.

With native fusion inputs, all 58,839,552 active reconstructed RGB components match exactly before residual application. Residual gains differ in 1,079 components by at most 0.000000238419. This changes 50 final RGB components by one unsigned code. Final residual application itself matches every active RGB and signed-detail component when supplied native inputs. The port defines the physical outer five pixels explicitly. They lie outside this camera's active crop.

Four bounded wavelet compositions match every packed code, including nonzero crop origins and a deterministic level-six baseline. The reference implementation's level-six shared-kernel normalization has a scheduling dependency. The port normalizes one owned kernel deterministically. Packed Lab inversion matches the dense fixtures exactly. Forward conversion differs in 137 of 196,608 dense components by one packed code on Metal. Full proportional sharpening matches all 64,862,208 native RGB components at the enclosing worker-join boundary. The neutral Standard tone table matches all 196,608 native words. Shared finishing encodes the resulting linear response with its existing primaries and transfer machinery.

The native Stage1 threshold must retain separate Float32 multiply/add steps. Algebraically replacing them with a single multiplier changed a classification at an exact boundary and created a roughly four-code outlier. Full-image comparisons found this after bounded fixtures passed. Native unsafe extra-column writes, unowned tail reads and mutable cache aliases are documented rather than reproduced.

Ordered repair performance uses metadata-only dependency batches where independence is proven. Parallel candidate scoring preserves cluster point order and first-candidate ties. RGB cluster processing fell from about 39.6 seconds to 217 milliseconds in its full-frame check. Runtime image calculations stay on the GPU. Small calibration-table and control preparation remain in Rust.

The complete native-size export comparison catches a separate final-crop convention. The reference crops raw CAMF endpoints as exclusive, then resamples the slightly smaller rectangle to nominal inclusive dimensions, with a 15-bit cap on this interpolated branch. A direct nominal active crop therefore drifts spatially across the image. The functional code-crop operation preserves fixed-point coordinates, 16-phase interpolation and operation order before tone. All 40,960 independently captured kernel words, 128 bounded native wrappers and 48 actual crop samples match exactly.

With that operation composed, the complete 5424×3616 diagnostic export matches 58,469,443 of 58,839,552 encoded RGB components exactly (99.37%). Mean absolute error is 0.05398 of a 16-bit code, maximum 119. These are complete native request outputs, not an application export claim. The remaining differences include measured mixed-precision and deterministic level-six behavior. The connected preprocessing maximum is 0.001953125 codes; Stage1 can amplify tiny input differences across a classifier boundary, giving a composed maximum of 5.692383 with mean 0.0000532 code. Native-input worker limits remain separate.

Image descriptions now retain whether crop, rotation and output sizing are already baked into their pixels. This keeps final presentation from applying the source crop twice while physical intermediates retain their own views in mixed requests. The signal's color space and buffer layout remain independent of that coordinate contract.

Final checks pass: 382 ordinary workspace tests, strict workspace Clippy, formatting and documentation indexes. Eight existing Classic/Merrill TIFFs remain byte-identical. GPU checks cover the complete photograph, mixed physical/final views, retained raw planes and shared TIFF destination conversion. The CLI also supports an explicit calibration-directory override for resized ProPhoto RGB TIFF export. The GUI app bundle is rebuilt. Other Quattro cameras, adjustable processing controls and editable DNG remain outside this reviewed first slice.

## Ordinary camera coverage

The owner approved extending the completed DP2 slice to the ordinary Quattro line while leaving Tiger unchanged. This covers DP0, DP1, DP2, DP3 and sd Quattro. The sd Quattro H uses the separate Tiger processing route.

- [x] Acquire and independently decode a DP3 original.
- [x] Share DP preparation using each file's calibration tables and geometry.
- [x] Add DP1's three-pass optical color correction at its reference position.
- [x] Add sd phase-detection correction and aperture-selected directional blue correction.
- [x] Complete ISO100 original-image boundary and export comparisons for all five ordinary models.
- [x] Validate the ISO800 route and early-version column extraction.
- [x] Review family UI availability, source summaries and regression evidence.

The recipe selects functional operations. DP models use bundled blue maps and sparse highlight calibration. The interchangeable-lens model uses its own phase-detection tables and mask, then rejoins the factory repair and reconstruction stages. Camera identity remains inside recipe preparation.

Bounded native checks cover all 80 directional blue fixtures exactly, including both ordered correction passes and the explicit scratch-tail equivalent. The sd chroma and luma gain tables match all 1,700 independently prepared entries. DP1's 16 complete optical-correction fixtures retain unchanged source storage and differ by at most 0.0001221 camera code in dense-matrix cases. The high-ISO Stage1 extension matches four complete native two-pass fixtures exactly while preserving the low-ISO path.

DP0 and DP1 complete full-size TIFF smoke checks. The complete DP0 diagnostic native export has 99.11% exact encoded components, mean error 0.09041 of a 16-bit code and maximum 137. DP3 and sd reach photographic output, but their native export comparisons expose remaining differences and are not acceptance evidence yet. Small worker proofs do not replace those connected comparisons.

All five ISO100 models now pass complete native export comparisons. DP1 and DP3 processing versions precede 3.0.4.0, so their recipes skip sector color correction. Named color-mode tables alone do not enable that stage. DP1 reaches 99.445% exact components, mean error 0.05546 and maximum 139. DP3 reaches 99.370% exact components, mean 0.04385 and maximum 111. The sd recipe uses the raw-color brightness exponent eight, while DP uses four. Its final export reaches 99.341% exact components, mean 0.05425 and maximum 224.

Original-image sd checks also corrected two FPPD assumptions that bounded fixtures did not distinguish. Its exclusion mask uses image severity, independently of the pixel severity used by later repairs. The diagonal-gradient arithmetic follows ARM64 addition order. With those corrections, all 22,233,088 preprocessed blue samples match exactly. RGB maximum error is 0.00097656 camera code.

High-ISO validation uses a DP3 ISO800 original with capture ISO800 and sensor ISO400. This proves reconstruction detail controls derive from capture ISO rather than sensor normalization. Decoding matches all original sensor words. Preprocessing and Stage1/Stage2 boundaries have small measured errors, but the full export still requires the high-ISO spatial-average local-contrast route. Do not treat the successful ISO100 camera comparisons as evidence for that unfinished branch.

The ISO800 route now selects two spatial-average contrast passes at capture ISO gain eight or above, followed by ISO-derived proportional sharpening. All 58,839,552 active post-contrast components and the full sharpening worker match native inputs exactly. Residual-noise lookup rounding uses a refined square-root product for the power 1.5. This removes 39 larger Stage1 component errors without changing the reference thresholds. The complete native-input Stage1 maximum is now 0.0001221 camera code.

The connected ISO800 export matches 99.7694% of encoded components exactly, with mean error 0.01284 of a 16-bit code. Its maximum 966 occurs in a 9×7 patch. The 55 components above the ISO100 maximum comprise 48 blue components and seven green components, with green errors at most 345. An independent replay traces that patch to one input blue ULP changing a half-resolution range-table index from 73 to 74. The reference's hard 0.75 weight cutoff changes that neighbor's contribution from about 0.75179 to zero. This is sensitivity to a small upstream numerical difference, with the same processing route and equations. The dedicated ISO800 fixture permits only that measured patch. ISO100 checks retain their original stricter maximum.

Modern recorded white-balance aliases, including Custom, AutoLSP and ColorTemp, use their stored shading ratios. All 213,440 independently captured red/blue grid values match exactly across ten aliases and both caller flags. These capture-time corrections are independent of the currently fixed Sunlight editing choice. Early-version column extraction matches six complete native fixtures. Earlier photographic calibration catalogues still need original-file validation before claiming every historical firmware route.

Final camera-coverage checks pass: 397 ordinary workspace tests, strict workspace Clippy, formatting and documentation indexes. All eight retained Classic/Merrill TIFFs remain byte-identical. Native export checks cover all five ISO100 models plus the dedicated ISO800 sensitivity fixture. Six resized review TIFFs render through the CLI, and the GUI app bundle is rebuilt. Main remains unchanged. Changes stay in the isolated worktree for owner review.

### Header and orientation audit

The reference implementation reads the same fields after a twelve-byte insertion in X3F4: the unique identifier moves from byte8 to20, mark bits from24 to36, image dimensions from28/32 to40/44, and clockwise rotation from36 to48. The inserted fields hold byte order and the four-word processing version, which was already preserved correctly. Sensor dimensions remain in the image-section header and include physical borders. They must not be replaced with the file header’s presentation dimensions.

Full decoding and embedded previews now share version-aware rotation reading. Preview loading also validates the newer byte-order marker consistently with full and capture-only loading. Real DP3 ISO100 output verifies portrait3616×5424 TIFF dimensions and a baked final rotation of zero, while retained sensor-coordinate images keep270 degrees. The six ordinary Quattro preview originals verify the expected orientation. Native diagnostic export comparisons explicitly request zero rotation to match their unrotated reference captures.

Image-section headers remain28bytes in the acquired originals. DP0 and DP2 also contain a newer format25 preview with section version0x200000. The preview reader uses their supported JPEG18 section. Extended header fields and unknown sections remain independent of photographic calibration from CAMF.

The newer fixed header ends at byte52, followed by TIFF-style extension entries. These include saved adjustments and crop settings, which the fixed rendering recipe currently ignores. For example, a DP0 preview reflects a saved horizontal crop while the full photograph uses CAMF ActiveImageArea. The remaining forty-byte section lower bound is permissive for malformed v4 files but does not change valid-original decoding.

### First photographic controls

The Quattro panel now composes the shared white-balance widget and tone rows. Standard, Vivid and Neutral are exposed in camera catalogue order; incomplete modes remain unavailable. Selected WB flows through Stage2 color conversion and recorded-to-selected shading. Stage1 retains fixed Sunlight gains, as confirmed by a nondefault Shade reference request. Factory Auto calibration and recorded WB remain independent. Unsupported reconstruction and correction overrides still normalize to their established defaults, while photographic edits survive metadata loading and camera switching. Exposure is bounded to -2 through +2 EV to bound reference curve preparation work.

The generalized response matches all 2,359,296 native LUT entries in thirty-six complete cases across Standard, Vivid and Neutral, including fractional exposure, both exposure endpoints and combined tone changes. Saturation shares the existing post-tone operation. Its luma weights and operation order match the reference implementation, with generic finishing retaining its existing floating-point rather than native Q12 quantization. The default DP2 complete-image comparison remains unchanged: mean error 0.05398 and maximum 119 output codes. A GPU session test checks each tone control, grayscale saturation, unchanged reconstruction and exact restoration of cached defaults.

Luminance noise selects a histogram-bilateral blue-plane pass and adaptive blend after outlier repair. The signed guide retains the original repaired blue, while restoration receives filtered blue. A compact native composite fixture checks both histories. Stage1 retains its fixed descriptor baseline independently of this photographic slider. Residual Detail scales the native residual power independently of Stage3, whose separate local-contrast and sharpening levels remain at level six. The reference user-facing residual slider also changes those Stage3 levels; an independent-detail comparison must explicitly hold them at six. Chroma/banding and other color modes remain unavailable pending their demonstrated processing routes.

The parallel scalar-noise kernel uses the reference implementation’s quantized range weights and adaptive blend while rebuilding each neighborhood independently. It avoids the rolling native histogram’s accumulated Float32 history. Eight bounded native fixtures measure at most 0.01465 blue-code error; actual image tiles measure at most 0.019. On a 21.62 MP plane it takes about 91 ms warm and leaves its input unchanged. The nonzero-noise complete DP2 export has mean error 0.25054, maximum 126 output codes and 97.908% exact components. The sd export has mean error 0.06366 and maximum 185. This route has separate measured comparison bounds. Default photographic bounds, including the original high-ISO cutoff witness, remain unchanged.

Proper calibration resources are required for native photographic comparisons too. A newly captured Shade request without its BData working directory silently produced degraded pixels. Repeating with the resource contract gave mean error 0.05177 and maximum 137; Vivid gave mean error 0.05754 and maximum 144; Neutral mean error 0.05075 and maximum 124; combined Shade/Vivid mean error 0.05673 and maximum 124. Resource-backed receipts retain map hashes. Invalid exploratory captures were discarded. All five models render their nine camera WB presets and both additional modes with Sunlight/Shade, with exact restoration of cached defaults.

A native comparison with residual detail zero and Stage3 held at level six validates the independent Detail contract: mean error 0.05030 and maximum 133 output codes. GPU edit tests on DP2, sd and DP3 ISO800 preserve the factory corrections and early technical filter exactly, leave global brightness within 0.4% at the tested noise levels, and restore default pixels exactly.

A separate ISO800 luminance-step-two capture has mean error 0.18076 and maximum 944 output codes. Its preexisting cutoff witness remains confined to the same physical patch: 48 blue components in the original box and ten green components in a two-position-expanded box. The filtered witness has its own explicit bounds; the default witness remains unchanged. Actual native histogram controls, every range-weight byte and all 16,384 adaptive-blend entries match exactly. The remaining filtered differences come from accumulation history rather than parameter selection.

Final controls verification passes all 407 workspace tests, strict Clippy, formatting and documentation checks. Rebuilt GUI and CLI exports remain byte-identical to the retained eight Classic/Merrill baselines. Source changes remain in the isolated worktree for review.

The [performance pass](./quattro-performance.md) parallelizes independent repairs and improves the functional edit checkpoint without changing same-setting pixels. Unedited files now resolve WB from their recorded camera choice; explicit and saved selections remain intact.

## Reference quality completion

The owner approved extending the ordinary Quattro pipeline toward the reference implementation, prioritizing image quality over advanced tone editing. Keep Tiger deferred. Custom tone curves and spatial Retinex/Fill Light remain deferred. Retinex is a spatial operation and must not be described as an ordinary RGB curve.

- [x] Compare the sd Quattro crop at x2137, y2429, width653, height532 against native processing with matched controls.
- [x] Resolve the public chroma/luminance control mapping and expose the existing early RGB filter as Chroma noise.
- [x] Validate the remaining ordinary color modes, including Portrait's reconstruction and spatial appearance modifiers.
- [x] Complete adjustable sharpening through the demonstrated proportional and optional per-channel operations.
- [x] Trace the dedicated monochrome route and implement only demonstrated processing.
- [x] Audit banding and remaining correction controls for reached consumers.
- [x] Recheck connected output, editing caches, existing families and packaging after integration.

The earlier descriptor study's conceptual chroma/luma names were swapped relative to the public adjustment record. Public chroma is at byte0xf0 and luminance at byte0xf8. Chroma selects the early RGB filter strengths. Luminance selects the later high-resolution blue filter. They remain independent controls. Stage1 preparation now accepts only the chroma step instead of a misleading three-control record.

The exact reported crop at minimum noise matches the native minimum with 99.8912% exact encoded components. The newly connected chroma-step-two full sd Quattro photograph has 99.3510% exact RGB16 components, mean error0.052901 and maximum124 codes. Its reported crop has 99.8889% exact components, mean error0.024779 and maximum83. The native chroma adjustment visibly reduces the green/magenta blotches. These are explicit diagnostic settings and do not establish reference application defaults. The minimum baseline stays unchanged.

Optional per-channel sharpening preserves UInt16-wrapped vertical sums. Six native worker fixtures cover normalization1/8 and amounts0.5/1/1.7, including bright codes. The GPU comparison excludes the native worker's unowned bottom halo. Our operation defines owned-edge reads and initializes threshold index16384 to its continuous20-code limit. The reference wrapper leaves that threshold entry unwritten.


### Correct file defaults and native detail control

Saved X3F4 adjustment entries now feed capture-only metadata and full decoding. Fresh photo settings use those values before explicit edits. Missing entries use chroma and luminance step two, Smooth/Crisp one half, sharpness zero and highlight recovery one. Saved preset enums resolve through the actual camera catalogue. The UI presents seven Smooth/Crisp positions and signed sharpness. Smooth/Crisp jointly selects residual restoration, local contrast and proportional sharpening. The earlier independent-detail contract above records an intermediate investigation and is superseded by this public contract.

All ten acquired color modes have complete response-table and connected-image checks, including Portrait's spatial modifiers. Banding has no reached ordinary-color consumer and remains hidden. Reference diagnostics require their calibration resource working directory. A successful process exit without those resources does not establish valid photographic evidence. Resource-backed default-control comparisons measure mean RGB16 errors of 0.0521 codes on SD and 0.1471 on DP3 ISO800, with maxima 110 and 93 respectively.


### Monochrome composition and rounding

Monochrome has its own Stage1 channel-noise sequence and retained source, uses factory Auto calibration, bypasses the color-response transform, and retains the demonstrated highlight, chroma outlier protection and sector corrections. Its blue-noise filter remains active at the minimum level and uses a separate blend table. Residual power, local contrast, proportional detail and final response use the monochrome controls. Recipes select prepared numerical behavior. Nodes receive neither a camera identifier nor an architecture enum.

The native-input reconstruction RGB is exact. Its residual gain differs by at most 0.000069 and the resulting reconstruction RGB/detail by at most one code. Neutral mixing uses an exact integer mean. This avoids the paired-Float32 approximation's downward tie bias while keeping the measured difference from native Double mixing within one code. The full minimum-noise, Crisp SD monochrome photograph measures mean RGB16 error 2.48 and maximum 209 codes. With neutral equal-third mixing and the reference default luminance and Smooth/Crisp values, mean error is 2.21 and maximum 141 codes. These neutral-mix diagnostics do not include the saved file mixer described below. This is a declared rounding approximation, separate from the stricter color comparisons.

Color and monochrome Smooth/Crisp selections are stored independently. Source-derived sharpness remains unresolved until the selected rendering mode is known. Fresh GUI settings receive file defaults before launch overrides. Existing sidecars and early edits remain explicit. Saved X3F mark bits select monochrome, and initial tonal values follow that mode. The editor deliberately shares subsequent tonal edits between color and monochrome rather than maintaining separate editing stacks.


The complete native-input monochrome Stage3 comparison has maximum four camera codes and mean 0.187, from quantization through shared packed Lab and wavelet processing. Neutral mixing remains within one native code. These bounds are explicit and separate from the existing color checks. Complete monochrome comparisons pass dedicated measured bounds for minimum/Crisp and reference-default settings. Negative and positive exposure and combined monochrome tone edits each match every one of the 196,608 native response-table words. Positive monochrome exposure preserves the full curve domain and its response normalization.

All eight retained Classic/Merrill TIFF regressions remain byte-identical after this integration. The [performance plan](./quattro-performance-and-fast-preview.md) records accepted shader changes, rejected experiments and reproducible measurement conditions.


### Saved monochrome mixer

X3F4 tags 25–27 store unsigned fixed-point channel weights with unity 32768. All acquired ordinary Quattro originals contain red/green/blue words 10813/10813/11142. Fresh settings now retain those source weights, and the monochrome mixer reset restores them. Explicit sidecars and edits remain unchanged. The source weights are exact binary fractions, so their numerical mixing avoids the equal-third tie approximation. The corresponding full SD reference-default comparison has mean RGB16 error 0.0357, maximum 141 and 99.54% exact components. The corresponding DP3 ISO800 comparison has mean error 0.1482, maximum 61 and 98.78% exact components. Earlier equal-third monochrome comparisons above remain explicit diagnostic cases.


### Completion checks

The default-render completion passes 432 ordinary workspace tests, strict workspace Clippy, formatting and documentation checks. Independently captured native boundaries and complete exports cover color modes, chroma/luminance controls, Smooth/Crisp, sharpening and monochrome, including ISO800. Eight retained Classic/Merrill TIFFs are byte-identical. The native app bundle is rebuilt. Changes remain uncommitted in the isolated worktree for owner review. Reduced-resolution input, SFD, Tiger, advanced tone editing and Quattro DNG remain outside this completed photographic slice.

## Controls before reconstruction

The Quattro editing panel now uses the capture loader's camera model before complete sensor/calibration metadata or a rendered frame exists. Ordinary DP0/DP1/DP2/DP3 and sd models show their color or monochrome controls immediately. The existing complete-metadata inspection catalogue still validates the processing variant when available. Tiger remains inspection-only. Recorded control-reset defaults use the same property-only capture-adjustment parser. No worker scheduling change was needed.

Egui tests cover both rendering panels for all five ordinary models with capture properties and no processing metadata or image frame, preserve the requested settings, and keep Tiger's photographic controls hidden.

## DNG extension

The separately approved [Quattro DNG plan](./quattro-dng.md) adds editable color and monochrome exports using the accepted canonical-Standard approximation. Its unsigned-code source limitations and validation claims are tracked there.

## Closure

This completed implementation or experiment is archived during release 0.3.0 preparation at the owner's request. Earlier scope, review and working-tree statements describe that historical slice. Recorded limitations and separately tracked follow-ups remain explicit.
