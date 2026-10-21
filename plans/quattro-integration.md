---
summary: "Implement Quattro input, calibration and reconstruction through family-owned multiresolution recipes."
---

# Quattro integration

The owner approved the DP2-first implementation sequence after reviewing the integration assessment. Begin with decoding and independent-resolution plane inspection, then add calibration and reconstruction incrementally. Preserve the [recipe and numerical-operation boundaries](../docs/specs/classic-rendering.md) established by Classic and the shared [output requirements](../docs/specs/output-strategies.md).

## Assessment

- [x] Review existing decoder and reference processing evidence.
- [x] Identify equal-resolution assumptions in source loading and execution.
- [x] Separate reusable infrastructure from different numerical algorithms.
- [x] Propose the first photographic slice and validation sequence.
- [x] Start the DP2-first slice with independently validated GPU boundaries and explicitly supplied immutable calibration maps.
- [ ] Implement the approved slice incrementally.

## Organization before reconstruction

The owner approved a focused organization pass before extending the pipeline. Keep temporary rendering availability simple. Prioritize fewer shared-code edits when adding numerical stages.

- [x] Separate Quattro graph wiring from calibration preparation and its inspection catalogue.
- [x] Centralize image buffer layout and coordinate-view contracts used by execution and presentation.
- [x] Group Merrill-only technical and output builders under its family entry point.
- [x] Verify selective plane requests, retained values and existing rendering behavior after the refactor.

Merrill, Classic and Quattro have separate entry points under `render/recipes`. Quattro graph composition consumes numerical calibration parameters and recipe-owned inspection descriptions. It stops at completed requested boundaries instead of selecting dependencies through output-name prefixes. Calibration preparation receives source metadata and an explicit resource snapshot, without depending on the engine.

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

## Decisions and evidence still needed

- **Calibration resources:** DP cameras use external blue-calibration maps. Their controlled native equations are validated. A resource-enabled whole-original preprocessing capture now provides the comparison target, while the replacement composition and the application's resource-selection association remain open. The engine accepts an explicit calibration directory and snapshots its validated files. No installed-application path or process working directory is used. Do not silently substitute the missing-resource diagnostic route.
- **Parameters and captures:** replace captured request fields with source-derived preparation. Preserve distinct entry states when native processing mutates earlier surfaces.
- **Precision:** several native stages mix Float32, Float64, FMA and integer wrapping. Prepare tables on the host, compare Float32 GPU implementations with complete fixtures, then decide where paired arithmetic or measured approximations are warranted.
- **Borders and scheduling:** specify complete-image borders, strip halos and deterministic initialization. Default level6 wavelet processing has native scheduling variation. Bounded deterministic fixtures do not establish full-image parity.
- **Photographic response:** retain integer gains, tone/editor order and output-matrix arithmetic in a functional response operation. Then join generic linear color conversion and transfer encoding, checking the reference gamma convention to avoid double encoding.
- **DNG:** establish scene-linear scale, clipping and the storage color basis before using the shared fitter/writer. A pre-tone RGB16 surface is not sufficient evidence of scene linearity. Spatial Retinex cannot generally become a global DNG look.
- **UI:** add a family panel and capabilities after the supported controls are demonstrated. Decode and capture metadata must identify the family early. Shared widgets do not imply identical slider meaning.
- **Oracle coverage:** many workers and bounded stage compositions are independently exact. Whole-original replacement parity, matched application TIFFs, complete resource-enabled calibration composition and default level6 local contrast remain unverified.

The authorized reference checkout contains the detailed decoder, dispatch, calibration, Stage1/2/2B/3, finishing and backend research notes. Those notes distinguish static recovery from executed native evidence. This plan summarizes the integration implications without depending on machine-local captures.

## Implemented input and calibration boundaries

The [input and inspection specification](../docs/specs/quattro-input.md) defines this slice. All five acquired originals decode exactly against independent native sensor captures: 175,371,264 Float32 components and 16,660,568 CAMF bytes. Full loading and bounded capture reading agree on camera properties, including originals without PROP. Processing versions and packed source storage are explicit.

The GPU exposes `sensor.rgb` and `sensor.blue`. Supported Quattro processing versions additionally expose `extraction.rgb` and `extraction.blue`, with offset/scale correction and high-blue clipping. Complete DP2 extraction matches all 37,836,288 independently captured RGB/blue values exactly. Tiger raw inspection is available, while its different gain-bearing calibration remains unimplemented.

Explicitly supplied blue maps are blended from source aperture/focus metadata. All 84 tested native grids, totaling 1,792,896 values, match exactly. `phase-gain.blue` applies these maps to the extracted blue as an isolated correction boundary. Five bounded fixtures and the complete 21,620,736-value DP2 blue image match the native operation bit for bit. This does not claim complete preprocessing: other reference corrections still belong between extraction and reconstruction.

Paired Float32 arithmetic requires explicit FMA rounding boundaries on the tested Metal backend. Ordinary arithmetic allowed the compiler to collapse the compensation and changed several million blue components. The measured plain-Float32 alternative differed by at most 0.001953125 blue codes. The retained paired implementation matches the native Double resize before its final Float32 output exactly in the checked fixtures.

Graph tests exercise each plane alone, mixed raw/corrected requests, 90-degree view geometry and immutable retained siblings. A complete source-derived resource-enabled graph output also matches the native correction. The GUI shows an unavailable Final honestly and permits explicit plane selection. It uses each plane's own dimensions and a 14-bit display scale. Photographic export remains disabled.

Eight complete Sunlight/Standard TIFFs remain byte-identical to the retained pre-Quattro build: SD9, SD10, SD14, SD15, DP1, DP1s, DP2 and Merrill. Merrill also matches the retained original-main executable. On the development GPU, warm full-DP2 extraction takes a median 2.17 ms and blue-map correction 4.27 ms. These timings exclude decoding, transfers and host preparation.

Selected Stage1 parameter derivation is retained as a tested preparation study. It uses actual source exposure and explicit midpoint noise steps, rather than promoting earlier diagnostic captures to application defaults. Complete preprocessing and Stage1 numerical execution remain next work. Match every later intermediate before declaring photographic or DNG support.

The next comparison target is an owned full-frame capture at successful PREPROCESS return, with all native threads stopped and the process terminated before Stage1. Three resource-present runs produce identical RGB and blue hashes. Native map-loader return hooks confirm the selected DP2 map payloads were actually loaded. An otherwise identical empty-resource request records null map loads. Both outputs are finite. Resources change 5,491,327 RGB components and 21,215,845 blue values. Mean absolute differences are 0.0133 RGB codes and 2.5605 blue codes, with maxima of 694.60 and 582.96 respectively. Missing resources therefore cannot stand in for resource-enabled calibration. The request retains explicit diagnostic exposure and control settings. This establishes a stable preprocessing target, not application export parity or a completed replacement calibration pipeline.
