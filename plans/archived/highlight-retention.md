---
summary: "The fixed highlight-retention curve was implemented, photographically rejected and superseded by regional adaptive tone."
status: archived
---

# Highlight retention prototype

This fixed-curve experiment is superseded by the selected [regional adaptive base response](../../docs/specs/adaptive-tone.md). The [research reference](../../docs/fuji-rendering-research.md) distinguishes relative DR tone behavior from raw capture conventions and camera color calibration.

- [x] Compose an original monotone shoulder with the existing Merrill response and share its preparation between photographic output, HDR pivot calculation and DNG appearance.
- [x] Add the default-Camera dropdown and carry it through saved settings, scopes, bypass and CLI.
- [x] Verify midtone/black anchors, retained headroom, unchanged sensor recovery, cache toggling, Color/Monochrome and DNG appearance.
- [x] Build and open the application for photographic review without committing or publishing.

The first response averages two bounded rational shoulders inspired by relative D200/D400 tone behavior. Their shapes are fitted to median color-simulation tone warps, with baseline compensation paired before the curve and a fixed 0.18 normalized input anchor. Only original analytic equations and scalar shape parameters enter the application. This is a qualitative starting point in our calibrated signal domain, not a claim of matching a camera simulation.

Preparation composes numerical tone tables on the host. Photographic sample arithmetic remains in the existing GPU tone operation, adding no full-image dispatch. Regional framing and viewport size do not affect the response. A shared helper owns the composed table so DNG and HDR cannot accidentally use the unmodified curve.

AgX remains a possible alternative base rendering for later comparison, particularly for bright saturated colors. It must not be stacked as another complete display transform over this response.

## Findings and checks

The fitted shoulder exponents are 1.56 and 1.14. Their mean preserves black and the 0.18 nominal linear anchor, gently raises lower values and compresses higher values. Composition uses the first white plateau in the selected camera curve, so a mode that reaches white early cannot accidentally retain that same plateau. The prepared table is shared by native rendering and DNG fitting, and its HDR pivot follows the selected response.

DNG preparation previously stripped all shared photographic controls. The base-response selection is now explicitly retained as deferred appearance. The new response also uses the existing profile-curve range compensation instead of positive BaselineExposure, keeping highlight headroom available until the reader applies tone. The default Camera policy is unchanged.

All 724 ordinary workspace tests pass with one test thread. An earlier parallel run hit timing-sensitive background/cache tests under concurrent processing load. The serial rerun passed without changing those tests. Strict all-target Clippy, formatting, documentation indexes and five artwork tests pass.

Two original-file checks verify unchanged reconstructed RGB and technical DNG samples, changed appearance, Color/Monochrome, cached/fresh toggling, active HDR and an independently evaluated DNG profile with retained highlights. Six native captures exercise Camera, Highlight retention, HDR and Monochrome. Actual CLI exports produce a 320×480 RGB16 TIFF and a full-resolution RGB16 DNG. The rebuilt application is open on an isolated review copy of the blue parking sign with Highlight retention selected. Global defaults and the example original were not changed. Photographic feedback remains the next step.

## Photographic feedback

The owner rejected the fixed-curve appearance on `_P2M0020.X3F` and `_P2M0015.X3F`. Bright skin, white fabric and knit texture lose tonal separation. Camera with the existing spatial HDR controls gives the preferred result. Example settings are Highlights −0.40 and Whites −0.67 on the first image, and Highlights −0.37 and Whites −0.57 on the second. Other visible tone adjustments are zero. These examples establish a photographic target, not a universal strength selected from scene analysis.

The rational input warp alone retains approximately 47% of local log contrast at nominal white, 26% at twice nominal white and 12% at four times nominal white. The subsequent camera curve further determines the final contrast. Monotonicity, headroom and anchor tests did not evaluate this photographic failure. The spatial HDR operator instead separates a broad brightness base from local detail and limits the detail contribution to its correction. It also transfers one gain to RGB instead of warping each component independently before the camera response.

The owner subsequently chose a separate visibility-oriented spatial step, with Exposure and existing HDR slider behavior unchanged. The [visibility experiment](./visibility-tone-experiment.md) tracks this trial. Sensor recovery and calibrated color remain upstream. Any eventual DNG integration would need the spatial gain baked through the existing HDR route. It cannot retain the fixed-curve prototype's unchanged-technical-sample expectation.

- [x] Reproduce the two originals with Camera, the rejected curve and the preferred manual HDR adjustments.
- [x] Produce an isolated spatial visibility-fit comparison with unchanged user controls.
- [x] Review the photographic comparisons and select the regional adaptive application slice.

The owner supplied readable workspace copies of both originals after removable-volume access failed. Ten-photo comparisons now include both portraits and eight example photographs. The owner selected regional adaptive processing for app integration. The fixed curve is retained only in the historical experiment generator, and has been removed from application rendering choices.
