---
summary: "Compare scale-separated native row and column correction against the accepted fine-band baseline."
---

# Extend Merrill banding width coverage

Requirements: [Merrill banding correction](../docs/specs/merrill-banding.md).

The owner observes a useful column improvement in dark regions, including a car hatch, while wider native row streaks remain visible in leaf shadows and below a desk. A near-edge column strip also persists. Extending spatial coverage is the present goal. Increasing fine-noise suppression alone does not satisfy it.

## Work

- [x] Preserve the accepted production multiscale commit and unchanged photographic controls.
- [x] Record the owner-selected fine guarded-row and per-layer column-budget control independently of the joint-column alternate.
- [x] Derive four native Gaussian difference bands, matching three-layer cap priors, and measurement-variance scaling.
- [x] Check the exact cap split, held-out coverage, sensor endpoint effects, and filter mathematics.
- [x] Compare medium row and column additions separately, then their combination at three strengths.
- [x] Compare the existing local column estimator with its row transpose and both sequential orders.
- [x] Test wider local row neighborhoods without changing column correction or tile coverage.
- [x] Evaluate combined corrections on the owner-identified areas and genuine image structures.
- [x] Record observed improvements by visual review as well as band-specific measurements.
- [x] Prepare a comparison report with unchanged controls and independently selectable baselines.
- [x] Record approval for an isolated interactive build with fine-band defaults retained.
- [ ] Port the guarded fine-row prepass to GPU and compare its native predictions with the reviewed baseline.
- [ ] Add independent spatial-band sizes and strengths to both local axes.
- [ ] Integrate controls with settings, recipe keys, preview/export, and Noise-scoped editing.
- [ ] Build and visually inspect the isolated app and tuned image comparisons.
- [ ] Select a final production integration after owner review.

## Findings and limits

The earlier fine-row estimator attenuates a 16-pixel-period variation to about 0.11 and a 64-pixel-period variation to about 0.002 at its extraction stage. These are filter responses, not measured denoising percentages. Wider disturbances require different frequency coverage.

Difference-band white-noise variance must use the actual kernel energy and effective sample count. Treating a smoothed band as unfiltered pixel noise can make its uncertainty dominate the signal and effectively disable correction. Common sensor-offset covariance and independent measurement covariance serve different roles. A common offset cancels when comparing independent strips for agreement.

The current prior study preserves 174 fitting caps and 98 held-out caps across 20 conditions. Broad endpoint variability can exceed interior variability. The amount also changes with capture conditions. These are empirical calibration measurements with limited camera coverage, not universal confidence bounds.

The first regenerated control exposed a neighborhood-boundary difference from the owner-reviewed per-layer column trial. The private axis runtime now preserves that trial's terminal neighborhood rule explicitly. All six controls across three scenes and two display exposures reproduce every displayed pixel, the existing row correction table, noise model, settings, and color matrix exactly. A control for one shader mode does not establish parity for another mode.

The local row transpose passed exact transpose checks on a non-square fixture for seven budget modes and two terminal neighborhood rules. Adding this row operation after the accepted column correction visibly reduces fine vertical streaks in the rotated leaf and desk photographs. Replacing column correction with row correction loses useful column cleanup in the car hatch. The wider residual patterns and broad color casts remain. Sequential operations also reestimate and clamp their outputs, so order differences cannot be attributed to clipping alone.

Cap-bounded Gaussian difference fields spanning sigma 2 through 32 were tested on both native axes at strengths 0.5, 1, and 1.5. Their additional visual effect is small, including when combined with the local row operation. The bounded fields are small relative to some coherent photographic brightness variations. The broadest fields remain uncertain; these measurements do not establish that increasing their correction is safe. An independent brightness component must not be discarded merely because the original investigation emphasized chroma.

The latest owner screenshot was matched to the retained capture 51 rendering at correlation 0.9994. Its visible area corresponds approximately to oriented coordinates x 176 through 640 and y 3980 through 4704. The comparison includes this exact area as well as the whole bottom context, leaf shadows, the car hatch, and genuine detail. Fixed-region profile measurements describe aligned structure; they are not aesthetic quality scores or proof that all aligned structure is noise.

Local row windows of 27, 63, and 119 pixels were compared while retaining the existing 27-pixel column operation, correction budget, block phase, and tile geometry. All three 27-pixel controls reproduce the prior matched local-axis result exactly. The 63-pixel window gives a modest additional improvement in medium-scale evenness. Portrait hair, fabric, and colored car edges show no obvious new muddiness in the selected stress comparisons. The portrait's boosted face is clipped, limiting conclusions about skin color there. These are selected photographic checks, not evidence of universal preservation.

The 119-pixel neighborhood fits the existing tile context, whereas 127 does not. This geometry check does not establish photographic safety: the source atlas clamps to the full sensor rather than reflecting the active image boundary, so a wider window can include inactive or shield pixels near physical endpoints. The 119-pixel trial remains exploratory. No padding or column-block phase was changed to hide this difference.

Selected correction-change profiles did not show a strong new concentration of steps at the 40-pixel block grid. This does not establish seam absence. The wider changes remain small, and the broad desk cast and upper-edge brightness strip are unresolved. The comparison therefore starts with the clearer narrow-band improvement and exposes wider variants separately. No new banding method has been added to the production commit or made a default.

A shared three-layer decision protects correction direction, but it does not establish that a proposed offset is noise. Real stripes, gradients, and edges remain essential checks. The original reference row detector also changes its acceptance after joint layer filtering. New experimental behavior must remain isolated from the accepted production method until reviewed.


## Current review additions

The isolated app exposes independent denoising and Reference/Directional banding choices in both Color and Monochrome. Monochrome captures initialize the rendering mode separately from the inactive Standard color look. Native shield correction, guarded rows and multiscale filtering precede the dedicated mono mixture. Directional banding uses the shared calibrated-layer implementation. Hidden Reference amounts do not change Directional operation identities.

Disabling Banding exposed signed raw layer values to texture restoration. Opposite signs could cancel the weighted normalization denominator and create thousands of saturated points. Using nonnegative signal proportions only for the raw texture color mixture removes this numerical amplification while retaining signed upstream data. The reviewed Banding-on photograph remains pixel-identical. Actual dark-crop review and a signed-cancellation GPU fixture verify the correction.

The sharp white points are a separate issue. All 24 ranked points in one DP2 crop had persistent same-location positive offsets across 14 same-body, matched-setting cap frames. Only four locations appeared in the camera defect list. Their records passed the current cutoff and geometry but flagged individual layers rather than the whole bright joint defect. Native photo/cap residual correlation was 0.936 across the selected layer components. This identifies per-camera defects. It does not justify global smoothing or shipping those coordinates. Follow [sensor defect calibration](../docs/specs/sensor-defect-calibration.md) for the proposed user workflow.

The requested [chromaticity mixer](../docs/specs/chromaticity-mixer.md) adds a CIE guide without changing the native layer weights or pipeline. The owner subsequently replaced named gamut geometry with fixed spectral red, green and blue anchors and a dashed triangle. Display gamut settings no longer enter the mixer.

The current review build passes 670 ordinary workspace tests, strict workspace Clippy, formatting, documentation indexes and five artwork checks. Focused GPU checks cover signed texture cancellation, monochrome graph/export agreement and cached mode switching. All three monochrome photographic outputs have equal RGB channels. The signed-texture fix leaves the accepted Directional Banding-on rendering byte-identical in the checked photograph. The chromaticity widget was rendered and visually inspected in its initial named-gamut presentation, then verified in the isolated desktop app. Per-body calibration remains a documented follow-up rather than a shipped profile workflow.

The fixed visual anchors use representative 700, 520 and 460 nm locus points. They illustrate layer proportions without claiming a measured sensor profile. The owner clarified that the 24 identified persistent speckles were only a selected sample. Other visible points may be transient, so wider classification must precede generalizing the defect result.

The native X3F properties already expose `CAMSERIAL`, which the GUI metadata panel displays. Profile training must save this identity for both body-specific and generalized data. Only the former uses a matching serial as an application gate. Residual speckles require independently tunable threshold and size controls rather than treating all visible points as persistent defects.

## Speckle and shadow follow-ups

The owner authorized comparative speckle threshold/size trials and moving Speckles from Corrections into Noise. Reference detection remains the default while an optional positive compact detector tests wider immutable rings. Threshold scales the camera-derived tables. Native neighborhood sizes 3, 5 and 7 target isolated points and small clusters. Metadata bad-pixel repair remains in Corrections. The Noise section must own speckle recipes and bypass for Merrill without changing other camera families.

After those trials, investigate [Neutralize shadows](../docs/specs/neutralize-shadows.md) as an independent appearance adjustment with dark-region, regional-neutrality and green/magenta direction gates. This is a separate problem from persistent defects and general denoising. Keep useful brightness grain and genuine colored detail as regression checks.

Speckle trials compared nine configurations on the DP2 car, DP3 speaker/desk and DP1 portrait, then seven on a DP1 car and ISO800 leaf, followed by five leaf configurations at a reduced display exposure. The initial leaf stress rendering clipped its bright detail and was not used as evidence of fine-detail preservation. Compact size 5 with threshold 0.25 is the current review starting point. Size 7 added little in selected crops. Reference threshold 0.10 removed many points too, with a stronger occasional residual and more fine-noise changes. These are visual recommendations, not promoted defaults. Eye color, hair, fabric, speaker edges and ordinary grain were reviewed. No universal preservation claim follows from these selected photographs.

The compact detector's synthetic GPU fixture verifies isolated-point and small-cluster cleanup, straight-line preservation and immutable source storage. Actual-photo method/threshold/size switching agrees between cached and fresh renders. The controls preserve the Reference default and move Merrill speckle copy/reset and bypass into Noise.

Neutralize shadows now has an optional GPU implementation after reconstructed linear RGB and before sharpening and camera tone. The regional guide combines Oklab lightness, chroma and brightness with separable edge-aware filtering. A green/magenta hue gate and regional/pixel colorfulness gates attenuate chroma, followed by a brightness normalization. This is a selective desaturation heuristic, not missing-color reconstruction or a calibrated sensor-offset estimate. Strength defaults to zero.

Nine strength/selectivity combinations were reviewed on the DP3 speaker, desk and screen, DP2 car and DP1 portrait. Six combinations were then checked on a separate DP1 car and ISO800 leaf at +1 display EV. Strength 0.8 with shadow limit -4, color tolerance 0.25 and hue width 35 is the current comparison starting point. The desk's broad purple cast and screen's green are reduced without an obvious change to the selected portrait eye or skin crop. Real muted green and magenta shadow colors can be weakened. Wide color tolerance or hue settings require more caution. No new default has been promoted.

A GPU fixture verifies both bias directions approaching neutral without reversing, retained brightness variation, bright-region bypass, strong-color protection and immutable source storage. Actual cached/fresh switching, zero-strength bypass and unchanged monochrome were checked. The accepted earlier default photograph remains pixel-identical with this control off. Warm cached full-image parameter trials took roughly 120–135 ms in the measured session, excluding TIFF encoding. This is not a guaranteed latency on other hardware or recipes.

Final speckle/shadow checks pass 676 ordinary workspace tests, strict all-target Clippy, formatting, documentation indexes and five artwork checks. GPU checks cover compact speckle source ownership and cluster/line behavior, cached/fresh speckle controls, shadow brightness/hue-direction preservation, shared photographic/DNG shadow parameters, zero-strength and monochrome bypass, and whole-frame/regional agreement with the added native halo. Both comparison reports retain fixed settings and native crops. The earlier accepted default TIFF remains pixel-identical.


## Bright unlisted defect missed by Compact

A reported saturated magenta point in a dark DP3 capture is at oriented pixel (116, 4622), native sensor pixel (145, 152). Original bottom/middle/top codes are [1158, 12, 21]. No main, JPEG or packed-cluster defect record covers this location. The nearest main record is three pixels away. The original-signal red-excess predicate accepts the point, but camera-record repair only tests immediate neighbors of existing main/JPEG records, so that dynamic extension never reaches it.

White-balance calibration raises the bottom-layer code to about 6923. Compact unconditionally bypasses calibrated values at or above 4095, incorrectly treating a white-balance-amplified, unclipped native sample as a protected bright highlight. Threshold 0.05 and size 7 still leave the point unchanged. Reference speckle detection clamps its test peak instead of bypassing it and repairs the point at threshold 1. An otherwise identical final render with Reference Speckles removes both the saturated point and its directional color streak. Changing denoiser method alone leaves the Compact bypass in place.

The owner authorized correcting this Compact clipping-domain defect. The speckle graph now receives the immutable original sensor layers alongside calibrated input. Compact uses the original per-layer 3900-code near-clipping guard already used by native layer filtering. White-balance-amplified values above calibrated white remain eligible for detection. Reference equations and their output are unchanged. A GPU regression covers the reported raw 1158 / calibrated 6923 case at every support size, preserves near-clipped and clipped samples above and below calibrated white, and checks both input buffers remain unchanged. Actual-photo GPU traces confirm the point is repaired by Compact with Reference, Multiscale Rows and Multiscale Rows + Columns denoising, and in Monochrome. The same-settings photographic crop changes the saturated magenta point to neutral [25, 25, 25] display codes and removes its streak. The checked Reference Speckles crop is pixel-identical before and after. All 676 ordinary workspace tests, strict release all-target Clippy, formatting, documentation indexes and five artwork checks pass. Four focused compact/region GPU checks and two ordered-reference GPU regressions pass. The fixed review app was rebuilt, signed, verified and reopened. Every saved photo sidecar was preserved byte-for-byte.
