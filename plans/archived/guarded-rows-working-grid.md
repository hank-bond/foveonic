---
summary: "Moves Merrill guarded-row estimation onto resized evidence with shared calibration, projected noise and bounded native rounding differences."
status: archived
---

# Merrill guarded rows on resized layers

Implements [small previews](../../docs/specs/small-previews.md) and [Merrill banding](../../docs/specs/merrill-banding.md). Cross-family work and broader application acceptance remain in the [early-reduction plan](../early-sensor-reduction.md).

The owner approved moving the remaining Merrill guarded-row estimator after resizing. Keep native shield measurement, calibration and mapped repairs. Reduced measurement uses area means of the original and post-shield raw codes. Those buffers describe the correction already applied and stay separate from the repaired calibrated numerical input. Full retains its original order and equations.

- [x] Project Gaussian supports and noise parameters beside the node, including row-prior high-pass response and small-grid span counts.
- [x] Defer reduced guarded rows until after pooling and before Multiscale, using existing graph and cache owners.
- [x] Verify native equations, resized known row disturbances, clipping, tiny grids and immutable repair evidence.
- [x] Compare native Full floats, reduced photographs and retained/fresh rendering with relevant option combinations.
- [x] Measure resident-source preparation and retained-edit work separately.
- [x] Rebuild the review app.
- [x] Owner approved the scoped checkpoint and requested a commit. Broader application review remains in the active plan.

The implementation treats native row offsets as independent across rows when projecting the existing covariance. Pooled row-offset variance therefore scales with the row count rather than the full cell's pixel count. The discrete high-pass filter's energy supplies the additional response conversion. The shipped model has no spatial covariance function, so this is an explicit preview approximation to validate. Keep Gaussian sigma at a representable working-pixel minimum and preserve useful spans when the active grid is narrower than twelve pixels.

The owner approved small native rounding differences to keep one parameterized estimator. A native-speaker comparison changed 19,816 of 48,254,976 intermediate components, with a maximum difference of 0.0000152588 raw codes. Separate fixed-support shader configurations recovered bit identity but were rejected as unnecessary complexity and removed. Five native Full comparisons cover two photographs, Color, Monochrome, Standard recovery and both Color Field methods. Their largest final RGB difference is 0.00041932 on a nominal 0–1 scale, with RMS differences below 0.000000265. The owner also approved retaining distinct raw measurement and calibrated numerical inputs for this slice, with possible later reassessment.

The brightness admission gate remains in native-equivalent signal-to-noise units. Pooled pixel variance is scaled back by the covered pixel count for that gate, independently of the working-grid uncertainty used to estimate rows. Otherwise averaging alone would admit brighter scene structure as row evidence.

Seven node checks cover an independent double-precision estimator, native equations, projected pixel and row noise, original clipping, calibrated repairs, immutable inputs and grids down to one cell. Seven Merrill integration checks cover native export preparation, regional output, noise bypasses and retained control changes. Seventy-five source-free edits match fresh output exactly across Directional Color, Monochrome, combined processing and both Color Field methods at divisors two, eight and sixty-four. Their decoded source owners are released before editing. Ordinary workspace tests, strict Clippy, formatting, documentation indexes and artwork checks pass.

Sixteen photographic comparisons include Fast, Draft and Thumb on hazy coast, low-light speaker and yellow flowers. Additional speaker comparisons at +3.35 EV check visible shadow structure. The overview comparisons show no obvious new artifacts. The owner authorized the checkpoint while broader application review remains pending. Original and post-shield means remain on the reduced grid and the scalar black table joins the existing source-free dependency cut. The photographic preview signature advances to invalidate earlier derived previews.

Three warmed fresh renders per configuration, from already-resident decoded sources, give Thumb medians of about 271–272 → 112–116 ms and Draft medians of about 292–304 → 137–148 ms across those three photographs with Multiscale rows and Directional banding enabled. Full on the speaker measures about 768 → 776 ms. Separate synchronized GPU diagnostics put guarded-row work at about 157.8 → 1.0 ms at Thumb, while numerical pooling increases from about 3.9 to 11.8 ms for the additional evidence inputs. These stage timings are not additive end-to-end latency. Two retained Thumb Exposure edits complete as a group in about 7.5–8.1 ms, reuse the row stage and match fresh rendering. These are engine measurements with explicit Sunlight/Standard settings, not application interaction timings. Workload monitoring reports no competing app or compiler during either comparison run.
