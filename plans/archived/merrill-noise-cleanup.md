---
summary: "Consolidate reviewed Merrill noise, banding, speckle and shadow controls before optimization."
status: archived
---

# Merrill noise cleanup

The owner approved a behavior-preserving organization pass over the current isolated implementation before optimization. Preserve Reference and both Multiscale choices, independent Reference/Directional banding, Color/Monochrome support, Compact Speckles, Neutralize shadows and the CIE mixer. Preserve training code for the later user-calibration workflow. That workflow remains deferred under [sensor defect calibration](../../docs/specs/sensor-defect-calibration.md).

The deliberate control change is two directional bands per axis: Fine and Coarse. The owner explicitly declined compatibility with older settings. Only the new Fine/Coarse format is accepted, and invalid older recipes follow the existing load fallback. Size is a direct group row rather than an indented child. The strength upper limit becomes three while defaults remain unchanged. Follow [Merrill banding](../../docs/specs/merrill-banding.md), [Merrill speckles](../../docs/specs/merrill-speckles.md), [Neutralize shadows](../../docs/specs/neutralize-shadows.md) and [chromaticity mixer](../../docs/specs/chromaticity-mixer.md).

## Work

- [x] Freeze representative image results before reorganizing numerical code.
- [x] Reduce directional band controls and estimates to Fine/Coarse, without compatibility parsing for superseded settings.
- [x] Separate native layer preparation from technical color construction while preserving lazy inspection and cache boundaries.
- [x] Keep Merrill Noise UI composition separate from other family tools and centralize control bounds/defaults.
- [x] Share identical color math and give shader fields, buffers and helpers descriptive names.
- [x] Remove unused processing paths without removing training scripts or evidence required by future training.
- [x] Check numerical parity where behavior is unchanged, new band controls, export/preview consistency and native app layout.
- [x] Refresh specifications and indexes, run required checks and package a review build.

## Validation

Capture fixed photographic settings across DP1, DP2 and DP3, including ISO800 shadows, portraits, camera edges, unlisted bright defects, band bypass, both shield options and Monochrome. Compare exact rendered-buffer hashes for unaffected processing. The requested two-band change has separate GPU and visual checks because arbitrary three-band settings cannot be represented exactly by two bands. Keep the earlier reviewed shader arithmetic and ownership boundaries intact. Do not introduce performance changes in this pass.


## Structural changes

The technical recipe now coordinates native-layer correction, tiled noise reconstruction and RGB finishing. Native corrections and tiled reconstruction have separate family-owned modules, retain their original operation order and still stop at requested inspection outputs. Numerical nodes remain independent of camera metadata interpretation.

The Merrill Noise panel is separate from rendering, detail and correction tools. Shared bounds and defaults come from processor control types. Directional banding has Fine and Coarse estimates plus one reflected Fine baseline needed for the Coarse difference. The retired middle estimate and its application path are removed. The compiled model coefficients, cap training scripts and future user-calibration requirements are retained.

Color Editor and Neutralize shadows use one attributed Oklab transform source. Host shadow uniforms name extent, guide support, filter direction and selectivity parameters. The shader retains its packed vector layout, with the component contract documented, to avoid changing numerical code generation unnecessarily. The row estimator no longer carries an unused ISO label in its uniform. New speckle and shadow edits participate in the shared changed-parameter tracking. The discarded layer-experiment settings adapter is removed in accordance with the owner's no-compatibility instruction.


## Checks and limits

All 675 ordinary workspace tests and strict release all-target Clippy pass. Twenty-four focused GPU checks cover shield prediction, row confidence, multiscale filtering, directional transpose and real-edge protection, independent Coarse strength through three, Compact clipping protection, shared Oklab editing, cached changes, shared photographic/DNG preparation, and whole-frame/regional behavior. Five artwork checks pass.

Thirty fixed photographic cases were captured across five images and three bodies. Reference-band and Monochrome cases matched exactly in the first comparison. Cases with active Neutralize shadows had small numerical differences. A full 16-bit TIFF comparison on the dark DP3 scene differed by at most one code value and no sample exceeded that difference. The owner explicitly accepted negligible rounding differences and asked not to pursue bit-exact output further. The packed shader uniform layout was retained. This cleanup does not claim new denoising quality, performance improvements, or completed user-cap training.

The rebuilt review app was signed and opened successfully. Native UI review confirmed Fine/Coarse groups on both axes, direct Size rows, unchanged default strengths and an accepted strength of three. Superseded review sidecars were moved aside so the existing invalid-recipe safety guard could allow fresh edits to save. No settings migration was added. Changes remain isolated and uncommitted for owner review.
