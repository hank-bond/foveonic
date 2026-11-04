---
summary: "Adapt recovered haze estimation and patch picking into a saved GPU Dehaze correction."
---

# Implement Dehaze

Follow the [Dehaze requirements](../docs/specs/dehaze.md). The owner requested automatic estimation and manual color selection in Corrections. The owner approved the controls and implementation.

- [x] Extend the private numerical study to global estimation and local shadow-color picking.
- [x] Compare recovered estimator equations with native synthetic inputs.
- [x] Identify the shared photographic-edit boundary and existing picker workflow.
- [x] Agree the controls, camera coverage, input-domain policy and empty-estimate behavior.
- [ ] Add shared settings, GPU estimation and pointwise correction with exact neutral bypass.
- [ ] Integrate Corrections controls, asynchronous picking and a display-converted color swatch.
- [ ] Carry edits through sidecars, scoped recipes, batch requests and TIFF/DNG export.
- [ ] Validate arithmetic, persistence, preview consistency and photographic behavior.

## Findings

The estimator uses a spatial dark-channel minimum and bounded candidate averaging. The correction is pointwise after the haze color and calibration are supplied. The local picker runs the estimator on a supported patch rather than reading one displayed pixel. Eighty synthetic configurations cover four sizes, five patterns and global/local preparation constants with unit and non-unit white balance. Maximum successful RGB error is below `3e-8`. Empty-candidate status agrees. This does not validate full photographic preparation or GUI replay.

Existing shared linear edits provide camera-supplied primaries, nominal white and geometry. Their DNG route already distinguishes scalar brightness transfer from chromatic baking. The new correction must use the latter when its color response is active. A stable whole-photo analysis basis is needed to avoid viewport-dependent Auto estimates. Selection must use the input before Dehaze to avoid feedback.

Use normalized linear RGB for the shared correction and retain signed residuals and highlight headroom. Keep whole-frame processing while active until reduced and regional rendering can share a stable analysis artifact. Empty Auto estimates bypass correction. Empty manual picks retain the previous settings. Recovered preparation constants belong to their inspected input domain and require explicit adaptation before use here.
