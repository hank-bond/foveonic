---
summary: "Merrill monochrome rendering mixes calibrated sensor layers before dedicated GPU filtering and exports editable linear monochrome DNG samples."
kind: reference
status: accepted
implementation: conforming
---

# Monochrome rendering

This specification describes the Merrill monochrome recipe. Classic uses its [reference-derived monochrome route](./classic-rendering.md#monochrome-rendering).

## Processing

- Color and Monochrome must select distinct authored GPU recipes using the original Merrill X3F. Monochrome must mix calibrated sensor layers before its dedicated noise, column, despeckle, and detail operations. It must not desaturate the color reconstruction.
- Shared calibration, camera repairs, highlight correction, speckle correction, and row correction retain their independent controls and immutable original provenance.
- The monochrome route must implement reference-style directional filtering, column-offset correction, histogram filtering, mono despeckling, signed detail, gain application, final mono sharpening, and mono tone. ColorDQ, color residual reconstruction and saturation must not affect its technical base. Independent Fine/Coarse Detail and Size controls operate on the cleaned monochrome signal. Neutral amounts bypass their spatial filters.
- Mixing must support the entire nonnegative R/G/B triangle, including isolated layers. Barycentric coordinates represent actual proportions and sum to one. White-balance gain normalization must account for gains already applied to the layers. Original reference sequential weight normalization is not required.
- Photographic arithmetic must remain on GPU. Host code may resolve camera parameters and prepare filter/tone tables. Necessary numerical or scheduling departures must be documented and tested against independently evaluated arithmetic.
- Preserve linear samples and headroom until the selected operation requires clipping or encoding. Match supported reference boundaries explicitly. Estimated ISO above 6400 currently has no established reference column-correction table and must report an unsupported request rather than substitute another algorithm.

## Interaction

- A Color/Monochrome toggle must switch pipelines. Inactive mode-specific settings must survive switching, sidecar saving, and reopening.
- Monochrome must provide a triangular Color Mix selector with labeled layer proportions, pure-layer corners, and reset to an equal mix.
- Show only controls relevant to the selected mode. Hide color mode, saturation, chroma reconstruction and ColorDQ controls in Monochrome. Dedicated monochrome Fine/Coarse Detail and Size controls remain available. Hide the layer mixer and mono noise/column-banding controls in Color. Preserve all hidden values. White balance, exposure, tone, corrections, output sizing, and sharpening remain available in both modes.
- GUI, CLI, sidecars, inspection, interactive caching, TIFF, and DNG must use the same interpretation. Mode/mix changes must invalidate the appropriate cached stages without overwriting retained images. Inspection must list only supported outputs for the selected mode.

## Shared finishing

[Photographic finishing](./photographic-finishing.md) may request an independent color-guide branch for six hue-sensitivity controls. The native monochrome mixture and filtering remain authoritative for the detailed base. Guide generation must not depend on that mixture, creative saturation or tonal edits. Sensitivity uses a cleaned linear color guide to derive a spatially stabilized brightness correction before tone. Zero sensitivity is an exact bypass. The same sensitivity gain is used for Merrill Color and Monochrome, and active sensitivity is baked into DNG linear samples. Independent monochrome reconstruction Detail is technical processing and applies to DNG samples.

## DNG

- Monochrome DNG must bake the selected layer mix, technical mono reconstruction and active pre-curve sensitivity, HDR, Local Contrast and Universal sharpening into linear samples. Exposure and global tone presentation must remain separate metadata, retaining tonal editing latitude. Spatial edits use the same gain as photographic rendering and preserve equal RGB channels.
- Changing the layer mix or restoring color requires regeneration from the original X3F and sidecar. Exported metadata documents the baked processing.
- Storage may use equal neutral RGB channels for reader interoperability. No source color can be recovered from that rank-one sample content. Single-channel container optimization is not required.

## Conformance

Checks cover pure-layer isolation, normalized mixtures, filter/column/detail arithmetic, neutral equal-channel output, mode-specific UI and persistence, cache switching and ownership, selective inspection, TIFF encoding, and DNG sample/presentation separation. Existing color rendering retains exact float and TIFF goldens.

GPU histogram columns initialize independently and use bin-major GPU scratch reused across fixed strips. Reference Float32 residual history across adjacent columns is not preserved. The column-parallel optimization preserves the original filter arithmetic within its modeled range. The histogram filters brightness up to 4095 camera codes and carries excess signal through per pixel. Histogram sums use that same bounded base, preventing recovered highlights from collapsing into one averaged bin or contaminating lower-valued neighbors. The arithmetic fixture's maximum difference is below 0.003 camera codes. Mono tone removes tiny Float32 Bezier plateau inversions with a prefix maximum, preserving a monotone DNG curve. Neither departure changes color rendering.

Seven original photographs produce finite neutral mono output. Adobe SDK 1.7.1 validates mono exports and independently decodes all tested stored samples exactly. The owner accepted the implemented photographic results. Capture One/Lightroom appearance and editing latitude remain unverified, as does broader high-ISO photographic quality. Details and known boundaries are in the [implementation plan](../../plans/archived/monochrome-rendering.md). Reader acceptance remains tracked in the [DNG export plan](../../plans/dng-export.md).
