---
summary: "Reference highlight restoration uses immutable original saturation provenance, ordered donor statistics, and clipped-neighborhood conditioning before despeckle."
kind: reference
status: accepted
implementation: conforming
---

# Highlight restoration

## Requirements

- The GPU **must** restore clipped-layer color and condition highlight neighborhoods using the reference Merrill algorithm after camera repair and before despeckle.
- Saturation provenance **must** come from original camera run/index maps, not processed-pixel clipping. Original metadata, samples, and retained artifacts **must** remain immutable.
- The operation **must** remove isolated flags, construct the native multi-layer 5×5 neighborhood map, accumulate blue/green donor ratios in original row-major order, fill the quantized correction table as the reference does, and apply its blue correction and brightness/neighborhood neutralization.
- The work rectangle and exposure-derived threshold **must** follow camera metadata. Global Sensor gain **must** preserve the stage's sensitivity by processing in the neutral-gain domain; zero gain **must** remain finite black.
- Restoration **must** default on with an independent shared GUI/CLI switch. The tone Highlights slider **must** remain a separate tone adjustment.
- Inspection **must** expose restored sensor layers and original clipping-neighborhood density. Density-only inspection **must** avoid calibration, camera repair, WB/color preparation, and donor statistics.

## Implementation and evidence

`highlight.layers` retains restored calibrated layers; `highlight.density` is a scalar 0–1 neighborhood fraction. Their processing/storage meanings remain independent of display. Density uses original saturation metadata only. Shared GUI/CLI settings expose `highlight_restoration`; CLI switches are `--highlight-restoration` and `--no-highlight-restoration`.

GPU donor compaction preserves row-major summation without photographic readback. Parallel isolated-flag removal is equivalent to native in-place removal: any connected pair preserves both flags, so only originally isolated flags can disappear. Float32 restoration differs slightly from native double arithmetic. Component fixtures cover connectivity, original flags independent of processed values, blue correction, density, borders, gain 0/1/1.3, and immutable inputs. Six full-sensor identical-input native replays have maximum error `0.001953` camera codes. Density-only tests verify stage-local execution and retained ownership.

Performance of sequential camera/cluster repair and donor accumulation remains an optimization concern; photographic owner review and broader camera/high-ISO coverage remain pending. Evidence and further work are in the [completion plan](../../plans/complete-reference-algorithms.md).
