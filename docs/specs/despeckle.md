---
summary: "Full-image GPU despeckling detects channel extrema and corrects them in deterministic reference order, independently of camera-record repair."
kind: reference
status: accepted
implementation: conforming
---

# Dynamic despeckling

## Requirements

- Camera-record repair and dynamic despeckling **must** have separate controls and both **must** be enabled by default in the shared engine, GUI, and CLI.
- Despeckling **must** run after calibration/Sensor gain and any enabled camera-record repair, before noise/detail reconstruction and camera color.
- The stage **must** use reference-style metadata-derived channel thresholds, immutable-input extrema detection, and deterministic bank/row-ordered correction. Image arithmetic **must** run in shaders.
- Peak detection **must** preserve the reference's 4095 headroom clamp. Valleys **must** retain negative signal values. Camera-derived scan margins **must** be distinct from the photographic crop.
- Correction **must** trim neighbor minima/maxima and anchor repaired channels to unaffected channels. Adjacent detections **must** read earlier corrections in reference order.
- Positive Sensor gain **must** scale correction consistently without changing the detector's intended threshold meaning. Zero Sensor gain **must** remain black and finite.
- GUI/CLI settings and retained artifacts **must** preserve the selected enabling state. Disabled output **must** omit the stage; earlier inspection roots **must** avoid its preparation and execution.
- Inspection **must** expose the corrected layers as `despeckle.layers`, with the normal cropped/oriented layer views.

## Implementation

`Settings::despeckle` is independent of `Settings::repair`. CLI `--no-despeckle` / `--despeckle` and `--no-repair` / `--repair` control the two stages; both start enabled. Reconstruction remains opt-in. Settings remain session-only in the GUI.

Host preparation derives three 4096-bin float32 threshold tables from the selected unscaled WB factors and float32 `DespAdjust`, defaulting to `[15,15,5]`. Integer adjustment vectors do not replace that default. The independent stage uses the reference diagnostic's 1× adjustment scale, independent of reconstruction presets. Production noise-recipe coupling remains deferred. The [node audit](../../plans/ordered-repair-and-node-audit.md#despeckle-wb-scale-and-recipe-coupling) records the mismatch with calibration's 2× WB signal and the proposed follow-up.

The GPU detector compares each channel with its eight neighbors and writes deterministic per-block ranks. A prefix/scatter sequence compacts records into eight bounded banks (at most 624999 each). One correction invocation replays the compacted records in native order. This preserves neighboring repairs without data races or host pixel readback. Saturation provenance is unchanged.

Detection divides out positive global Sensor gain; correction operates on the scaled signal. Geometry currently requires full-sensor keep bounds and eight row banks aligned to 256-pixel blocks, as supported by the Merrill source geometry. Unsupported geometry produces an explicit error.

Highlight restoration, fuller camera-record coverage, and distinct packed `BadClustersF20` repair remain separate stages. Adjacent defects represented in the main `BadPixelsF20` list are handled by ordered camera-record repair.

## Conformance

Synthetic reference fixtures check every output sample, lookup rounding, peaks/valleys, seven layer masks, adjacent mixed-layer records, margins, headroom, bank capacity/order, immutable input, and positive/zero gain. Original-image checks verify both reported unlisted red speckles are corrected without changing their green/blue samples, and retained artifacts survive subsequent settings changes.

All six originals render with both corrections, with reconstruction, and with both disabled. Disabled TIFFs match the previous neutral baseline byte-for-byte. Native captures and performance evidence are tracked in the [despeckle plan](../../plans/despeckle.md).
