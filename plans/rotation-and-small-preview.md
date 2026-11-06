---
summary: "Prototype Draft and Thumb sensor reconstruction after committing rotation and preview-retention fixes."
---

# Smaller sensor previews

The rotation and preview-retention corrections were committed separately as `63c6ec9`. Their behavior and checks are recorded in the [completed plan](./archived/rotation-targeting-and-preview-retention.md). Keep the existing release draft and tag unchanged while reviewing this experiment. Preserve unrelated help-example changes.

Implements the experimental [small-preview requirements](../docs/specs/small-previews.md).

- [x] Name quarter-width/height reconstruction Draft and eighth-width/height reconstruction Thumb. These represent one sixteenth and one sixty-fourth of the pixels.
- [x] Replace the earlier post-reconstruction RGB reduction experiment with calibrated sensor-layer pooling before Merrill noise and color reconstruction.
- [x] Preserve native corrections, immutable originals, recorded framing, camera orientation and requested edits. Keep Full, inspection and exports authoritative.
- [x] Pool through the existing triangular numerical pyramid. Use one aligned reconstruction tile for small previews to avoid repeating 64-pixel halos and fractional internal resampling geometry.
- [x] Preserve fractional active-image edges with the existing GPU resize sampler after reconstruction. Propagate the actual native-pixel scale into shared photographic effects.
- [x] Expose the two choices only when launching with `FOVEONIC_SMALL_OVERVIEW=1`. Unsupported geometry uses Full. Quattro currently falls back to Fast and Classic to Full.
- [x] Compare completed warm exposure edits and edits that invalidate reconstruction on two Merrill photographs.
- [x] Complete workspace tests, Clippy, formatting, documentation and artwork checks. Native Draft and Thumb comparison smoke checks and Full fallback GPU checks passed. A third low-light Merrill sample completed both reduced reconstruction paths.
- [ ] Obtain owner assessment before committing the experiment or choosing a release default.

## Measurements and appearance

On the 14.7 MP parking-sign sample, Full, Fast, Draft and Thumb produced 3136×4704, 1568×2352, 784×1176 and 392×588 photographs. Completed reconstruction-changing chroma edits measured medians of 152.20, 55.54, 35.16 and 25.35 ms respectively. Warm exposure edits measured 67.44, 24.79, 18.99 and 16.97 ms. These local optimized measurements include display-texture preparation and GPU completion, without GUI presentation overhead. They describe one photograph, not a general performance guarantee. Original decoding and native correction preparation remain outside the reconstruction speedup.

The fitted comparison retains the photograph's framing and overall appearance. Thumb loses fine texture at larger display sizes, as expected. A hazy-coast Merrill sample also rendered both grids. Full remains necessary for detail inspection.

The original RGB-only experiment measured smaller warm exposure costs but retained Fast reconstruction. That approach has been removed in favor of the requested sensor-level experiment.

## Remaining family and scale work

Shared photographic controls expressed in native pixel distances must receive the working-grid scale. Native sensor-coordinate corrections precede pooling. Statistical noise models and filter support need photographic validation rather than blanket division of their values. Whole-image Adaptive and Dehaze analysis retain their established shared analysis paths, which can set a performance floor.

Quattro already reconstructs at Full or its lower-resolution color grid. Further reduction requires coordinated color-history, blue-guide and mask geometry and noise approximations. Classic's signed-code, fixed-point and logarithmic routes need a separately validated linear pooling boundary. Neither family is claimed to implement Draft or Thumb in this first Merrill prototype.

## Profiling the fixed cost

Separate wall and timestamp runs on the parking-sign sample explain why warm exposure edits converge in Draft and Thumb. In the planning-instrumented wall run, medians were 17.88 ms for Draft and 16.77 ms for Thumb. Interactive checkpoint/cache planning cost 6.33 and 7.02 ms, and regenerating the tone lookup curve cost 5.61 and 5.64 ms. The tone and display kernels totalled about 1.8 and 1.3 ms in the separate GPU-timestamp run. Timestamp collection adds synchronization, so its overall completion times are not used as interactive performance evidence.

The executor also waits for GPU completion after every executed interactive graph node. Warm exposure edits execute only Tone plus display preparation, with upstream reconstruction cached. Reconstruction-changing chroma edits retained a larger difference: 35.97 ms versus 26.41 ms in the planning-instrumented run. Removing noise from a Thumb recipe would principally benefit preparation and reconstruction-changing work, rather than an exposure edit that already reuses those stages.

The next optimization candidates are reusing unchanged tone preparation and operation keys, then revisiting stage-by-stage GPU waits while preserving cancellation and bounded queued work. This investigation adds measurements only and does not implement those changes.

The maintained multi-photo harness, host scheduling comparisons and graph-boundary attribution are recorded in the [editing harness plan](./editing-harness.md). It replaces the ignored test's one-off timing exports. Preview tests retain framing, fallback and optional image-comparison coverage.
