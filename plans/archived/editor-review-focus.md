---
status: archived
summary: "Linked reconstruction controls, grouped inspection, folder restoration, and viewport reporting are implemented; the two identified residual speckles are corrected by the new GPU despeckle stage."
---

# Implement editor review focuses

## Closure

The owner accepted the implemented photographic and interface results and authorized closing review-only plans. This implementation slice is complete. Pending-review statements below describe earlier checkpoints. Cross-platform runtime checks were not performed and remain a coverage limitation.

The owner approved implementation after collecting the discussion. UI/settings changes and the camera-derived enabled repair cutoff are implemented. Reconstruction remains opt-in. The owner subsequently requested separate camera-record repair and despeckle controls, both enabled by default; this is implemented in the [despeckle plan](./despeckle.md). The owner authorized grouped commits after code and automated checks; photographic/interaction review remains pending.

Requirements: [GUI workspace](../../docs/specs/gui-workspace.md), [information bar](../../docs/specs/image-information-bar.md), [pipeline inspection](../../docs/specs/pipeline-inspection.md), [linked reconstruction controls](../../docs/specs/detail-and-noise.md#linked-advanced-controls), and [camera repair defaults](./camera-repair-default.md).

## Steps

- [x] Record the owner's focuses before inspecting the supplied screenshots.
- [x] Implement preset-linked advanced fields, captured overrides, darker/read-only linked styling, and shared GUI/engine calculation.
- [x] Fold RGB channels and sensor layers into the grouped Inspect menu; reuse textures for component changes.
- [x] Persist and reopen the last successful folder, respect explicit launch paths, and report stale paths without opening their parents.
- [x] Add capture settings / centered filename / visible image-pixel bounds below the viewport.
- [x] Use the cropped/oriented image grid, 0-based inclusive bounds, and visible pixel counts including partial boundary pixels.
- [x] Implement per-source automatic repair cutoff and explicit GUI/CLI override.
- [x] Compare the remaining defects with source samples, camera records, and reference repair/despeckle prefixes.
- [x] Verify tests, native workspace/restart captures, neutral exports, formatting, Clippy, and documentation/index checks.
- [x] Owner accepted the implemented controls and layout and closed the review checkpoint.
- [x] Obtain approval for the proposed full-image despeckle stage.
- [x] Confirm separate controls, both enabled by default, and implement the [despeckle plan](./despeckle.md).

## Implementation choices

Seven existing preset-driven fields have single basic parents; the parameter table, bounds, and CLI keys live in the [noise specification](../../docs/specs/detail-and-noise.md). Camera-only radius/range, diffusion step/contrast, and extrema clamping remain internal. Detail remains a separate multiplier. Noise preset 0 still filters; an Off/continuous-strength redesign and reference high-pass application are separate follow-ons.

`inspection::image_outputs` now describes component meanings. Worker frames retain their requested root and source path; pending root changes cannot reinterpret the component of the displayed artifact. Capture readouts use that artifact's metadata. Scopes still analyze the complete buffer.

The information bar reserves space below the shared preview/pipeline canvas. Pixel bounds derive from the current canvas/image intersection, excluding letterboxing. Tooltips retain full text when narrow panes clip the centered filename or side readouts. The artifact image variant is boxed after override settings increased snapshot size past Clippy's enum-size threshold; ownership and sample storage are unchanged.

## Residual-defect diagnosis

The two reported positions in `blue-parking-sign.x3f` are:

| Region | Cropped/oriented image | Original sensor |
| --- | --- | --- |
| Bolt | (2466,156) | (220,705) |
| Blue field beside white panel | (907,3560) | (3624,2264) |

The active crop is `[64,36,4767,3171]`, rotated 90°. Neither position is selected from `BadPixelsF20` at any cutoff 0–3. No adjacent main/JPEG records appeared in the one-pixel record search. Both are red-layer outliers, so changing the camera-record cutoff cannot fix them.

The existing read-only reference diagnostic library was replayed with Sunlight WB, aperture 3.2, one exposure, and exposure gain 1. Calibration, regular repair (including its other lists/clusters), and highlight restoration leave both centers unchanged. Its full-image despeckle stage emits `0x43ff` records for both and reduces the calibrated red sample:

- Bolt: 754.5583 → 124.3752.
- White-edge region: 382.0426 → 38.1213.

Green/blue samples remain unchanged. The detector emits 2679 records across this photograph. [The retained fixture](../../crates/foveonic/tests/data/repair-reference.json) and its [provenance](../../crates/foveonic/tests/data/README.md) preserve these results. The ignored camera-selection test checks the original samples and missing flags.

This establishes the missing despeckle stage as the cause of the two reported residuals. The current immutable-neighbor shader's cluster rejection, merged masks, and arithmetic still differ from reference repair and warrant follow-on comparison for other defects. No direct application-output comparison was performed.

Approved next slice: metadata-prepared, shader-based full-image speckle detection and corresponding reference-style correction before reconstruction. Separate controls with both enabled by default are implemented; current evidence is tracked in the [despeckle plan](./despeckle.md).

## Evidence

- Native review covered 84 captures, fourteen states on each of six originals, covering linked/manual/relinked values, parent changes, RGB/layer inspection, automatic/manual repair, zoom/region reporting, and neutral return.
- Restart review covered fourteen captures from a no-path launch using isolated saved preferences. It reopens the recorded folder, restores organizer/scope/section choices, and renders with neutral session editing settings.
- Six neutral TIFFs are byte-identical to the previous neutral baseline. The blue-sign default export is byte-identical to the preceding reference reconstruction export.
- Automatic and zero-cutoff/low-noise comparisons were reviewed. Noise settings can conceal some residuals; low presets expose the unlisted white-edge speckle.
- Tests, GPU tests, Clippy, and builds pass. Diagnostic probes located magenta peaks and camera records, then replayed reference prefixes for numerical comparison.

One initial native run captured a transient miniature main image and subsequent partial-fit views. A fresh instrumented run, no-path restart, and final six-image rerun did not reproduce it; geometry logging showed fit at zoom 1. Temporary instrumentation was removed. Its cause is unestablished; watch for recurrence during owner review.

Cross-platform runtime checks remain pending.
