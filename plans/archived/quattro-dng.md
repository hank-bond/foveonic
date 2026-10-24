---
summary: "Add ordinary Quattro color and monochrome DNG export with a canonical technical base and one approximate embedded appearance."
status: archived
---

# Quattro DNG

Requirements: [output strategies](../../docs/specs/output-strategies.md) and [Quattro rendering](../../docs/specs/quattro-rendering.md). The owner approved the same useful-editing approximation accepted for [Classic DNG](./classic-dng.md). Keep the existing uncompressed writer, GPU encoding, export scheduling and single-appearance policy.

## Scope and boundaries

Support full-resolution DP0, DP1, DP2, DP3 and sd Quattro, in color and monochrome. Tiger, reduced-resolution and SFD remain deferred.

Rerun a canonical Standard technical recipe from the immutable original X3F. Retain calibration, selected-WB-dependent corrections, noise/detail, supported spatial effects and reconstruction in the samples. Interpret reconstructed unsigned RGB codes in a linear technical basis using `1/16384`. Stop before photographic response and later RGB presentation. Embed the selected mode, exposure, tone and saturation as one appearance. Color-mode differences and mode-dependent spatial processing are an accepted approximation rather than an exact inversion of the native pipeline.

For monochrome, bake the selected recorded or edited channel mixture and technical reconstruction into neutral RGB samples. Keep mono exposure and tone in embedded appearance. Later editor WB cannot restore the sensor channels or change this baked mixture.

The source reconstruction has already produced unsigned 16-bit codes. Export cannot restore negative samples or highlights lost at that boundary. The shared zero-clamp and common headroom scale prevent an additional display shoulder, but do not create missing source headroom. Keep this limitation explicit in recipe history and validation claims.

## Implementation

- [x] Add the Quattro technical builder and existing DNG dispatch route.
- [x] Prepare its camera/storage basis and selected color or mono appearance.
- [x] Reuse shared range allocation, GPU quantization, writer and atomic destination replacement.
- [x] Exercise GUI color and monochrome exports from the immutable original while another inspection output is selected.
- [x] Check all five ordinary models and the high-ISO sample.

The GUI already provides Export DNG and sends it through the existing image worker. Its DNG branch uses the decoded original source, independently of the displayed inspection artifact. Enable the processor route without adding another export scheduler or a separate capability framework.

## Validation

- [x] Check exposure/tone/saturation separation, retained buffer ownership and baked WB/mix behavior.
- [x] Check scalar and matrix preparation against bounded independent expectations.
- [x] Validate representative color and mono containers with the existing Adobe SDK validator.
- [x] Independently decode stage-1 samples and compare them exactly with the stored uint16 payload.
- [x] Compare SDK appearance renders with photographic TIFFs and report measured differences.
- [x] Confirm metadata-only WB/exposure edits preserve stored samples and change rendering.
- [x] Run targeted GUI export-worker tests plus workspace source, formatting and documentation checks.

SDK validation establishes container and reader behavior. Lightroom and Capture One photographic acceptance remains a separate editor review. Existing local validation tools may be reused without importing their source or binaries into this repository. Keep only bounded review exports and receipts, deleting redundant decoded rasters after exact-sample checks.

## GUI integration checks

The ordinary Quattro color and monochrome panels appear from capture properties before a rendered frame exists. Export readiness separately requires complete metadata, valid settings and an available photographic `final` output. This keeps Tiger and reduced inputs from enabling TIFF or DNG export while retaining their inspection controls.

A background-worker integration test exports adjusted Vivid color and recorded-mix monochrome from SD and DP2 originals while the requested view is `sensor.blue` with comparison enabled. Exports complete without producing a display frame, preserve the selected settings and source metadata in the reply, and write DNG containers. The tests delete their temporary exports. All 149 ordinary GUI tests pass, including capture-only panel and export-availability checks. Strict GUI all-target Clippy passes. Reader/profile validation remains a separate gate below the processor tests.

## Independent reader results

The ignored processor tests cover appearance-independent floating and packed samples, WB/noise/smoothness/sharpening changes, frozen monochrome mixing, retained-source ownership, cross-engine rejection and baked orientation. DP0, SD and rotated DP3 high-ISO statistics show samples above the nominal white code of 16384. DP0 reaches 24503; the high-ISO DP3 reaches 32767 with 6,038,605 components above nominal white. Neither reaches the final unsigned limit. These counts describe retained technical headroom, not recovery of values clipped earlier in reconstruction.

Adobe SDK 1.7.1 validates 21 exports without warnings. All 58,839,552 stored uint16 components per export decode exactly; reader images and photographic TIFFs have the native oriented dimensions, including 3616×5424 for rotated DP3. The matrix includes all five ordinary models, DP3 ISO800, adjusted tone/exposure/saturation, monochrome, Vivid, Portrait and FCBlue.

Actual saved-default comparisons use Auto WB and saved modes: SD Vivid, DP0 Neutral, and Standard for the remaining samples. Mean absolute encoded-sRGB component errors range from 0.000177 to 0.000859; 99th-percentile errors range from 0.000717 to 0.009583. These are reader-versus-photographic-render differences, not a percentage of photographic fidelity. Mode-dependent spatial behavior is approximate: DP2 FCBlue has mean 0.005552, 99th percentile 0.030396 and maximum 0.123430; 0.02446% of components differ by more than 0.05. SD Portrait has mean 0.002181 and 99th percentile 0.010803. Monochrome means are below 0.000096 and maxima below 0.000260 in the SD and high-ISO DP3 cases.

Metadata-only AsShotNeutral and BaselineExposure edits were tested on SD, DP2 and high-ISO DP3. Stored sample bytes remain identical, SDK renders change, and the −2 EV edit lowers rendered brightness. These checks establish editing behavior in the independent reader; Lightroom and Capture One remain manual review targets.

Local receipts are under `target/quattro-dng-review`: `sdk-validation.json`, `edited/validation.json`, boundary statistics, settings and SDK logs. `small-review/` holds 1400-pixel matched PNG pairs for Standard, monochrome, Vivid, Portrait, FCBlue and representative saved defaults. Representative retained files include `sd-quattro-recorded.dng`, `dp0-quattro-recorded.dng`, `dp2-quattro-fcblue.dng` and `sd-quattro-monochrome.dng`. Exact-decoding intermediates and duplicate full TIFFs were removed after validation, reclaiming 6.26 GB; ten selected DNGs and bounded review images remain.

Final workspace validation passes: 438 ordinary tests, formatting, strict all-target Clippy and generated documentation indexes. Targeted GPU and GUI export tests pass separately. The release app bundle is rebuilt; changes remain uncommitted for owner review.

## Closure

This completed implementation or experiment is archived during release 0.3.0 preparation at the owner's request. Earlier scope, review and working-tree statements describe that historical slice. Recorded limitations and separately tracked follow-ups remain explicit.
