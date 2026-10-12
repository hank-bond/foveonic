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
- Threshold WB factors **must** match the actual factors applied by calibration, including its 2× scale and selected metadata adjustments, before global Sensor gain. The camera adjustment **must** use the agreed native noise-derived scale without recomputing WB factors.
- Peak detection **must** preserve the reference's 4095 headroom clamp. Valleys **must** retain negative signal values. Camera-derived scan margins **must** be distinct from the photographic crop.
- Correction **must** trim neighbor minima/maxima and anchor repaired channels to unaffected channels. Adjacent detections **must** read earlier corrections in reference order.
- Positive Sensor gain **must** scale correction consistently without changing the detector's intended threshold meaning. Zero Sensor gain **must** remain black and finite.
- GUI/CLI settings and retained artifacts **must** preserve the selected enabling state. Disabled output **must** omit the stage; earlier inspection roots **must** avoid its preparation and execution.
- Inspection **must** expose the corrected layers as `despeckle.layers`, with the normal cropped/oriented layer views.

## Implementation

`Settings::despeckle` is independent of `Settings::repair`. CLI `--no-despeckle` / `--despeckle` and `--no-repair` / `--repair` control the two stages; both start enabled. Reconstruction remains opt-in. Settings remain session-only in the GUI.

Host preparation derives three 4096-bin float32 threshold tables from calibration's actual float32 WB factors and float32 `DespAdjust`, defaulting to `[15,15,5]`. It receives the already prepared factors rather than reconstructing a separate WB recipe. Integer adjustment vectors do not replace the default. The stage uses the shared native ISO-table despeckle scale, defaulting to neutral noise controls when reconstruction is disabled. Native recipe metadata preparation is separate from tile geometry; it does not require executing reconstruction. The six current originals select scale 1. The independent enabling switch is unchanged. The [WB-scale fix](../../plans/despeckle-wb-scale.md) replaces the earlier mismatched diagnostic WB factors.

The GPU detector compares each channel with its eight neighbors and writes deterministic per-block ranks. A prefix/scatter sequence compacts records into eight bounded banks (at most 624999 each). One correction invocation replays the compacted records in native order. This preserves neighboring repairs without data races or host pixel readback. Saturation provenance is unchanged.

Detection divides out positive global Sensor gain; correction operates on the scaled signal. Geometry currently requires full-sensor keep bounds and eight row banks aligned to 256-pixel blocks, as supported by the Merrill source geometry. Unsupported geometry produces an explicit error.

Highlight restoration, fuller camera-record coverage, and distinct packed `BadClustersF20` repair remain separate stages. Adjacent defects represented in the main `BadPixelsF20` list are handled by ordered camera-record repair.

## Conformance

Synthetic reference fixtures check every output sample, lookup rounding, peaks/valleys, seven layer masks, adjacent mixed-layer records, margins, headroom, bank capacity/order, immutable input, and positive/zero gain. Original-image checks verify both reported unlisted red speckles are corrected without changing their green/blue samples, and retained artifacts survive subsequent settings changes.

Preparation tests check actual calibration factors across original-file WB presets, adjustment typing, and invalid inputs. A nonzero-threshold GPU regression preserves a marginal point incorrectly selected by the old scale while still correcting a stronger defect at Sensor gain 0/1/1.3. Full-sensor reference replay with identical input/factors agrees within 0.001 camera codes on all six originals.

All six originals render with both corrections, with reconstruction, and with both disabled. Disabled TIFFs match the previous neutral baseline byte-for-byte. Native captures and performance evidence are tracked in the [despeckle plan](../../plans/despeckle.md).
