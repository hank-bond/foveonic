---
summary: "Dehaze adds automatic haze-color estimation and manual shadow-color picking in Corrections."
kind: reference
status: accepted
implementation: incomplete
---

# Dehaze

## Agreed scope

The editor must offer Dehaze in Corrections, with automatic haze-color estimation and a manual color selector. Recover the missing estimation and selection behavior through the existing private numerical study before adapting the operation. Keep private research locations, external application identities and generated decompilation outside this repository. Name the tool and processing operations by function.

## Behavior

Provide a signed Amount slider with zero as its default. Positive amounts remove haze and negative amounts add haze. Zero must preserve existing photographic output exactly and skip correction work.

Shadow tone starts in Auto. Offer an eyedropper that estimates the haze color from a selected image patch, a swatch showing the selected tone, and a way to return to Auto. Sample an uncorrected processing image. Keep stored processing RGB separate from the display conversion used for the swatch.

Auto must analyze a stable whole-photo basis. Changing the viewport, preview quality or Dehaze amount must not select a different haze color. Manual picking must use the same numerical estimator on a local patch. Estimation must account for the image's white balance, primaries and nominal white.

Share GPU estimation and correction through the existing graph. Retain immutable inputs and reuse unchanged analysis while Amount changes. Apply correction at the shared linear photographic boundary before the camera tone response. Sidecars, scoped presets, defaults, clipboard and frozen batch requests must retain Amount, Auto/manual intent and manual color. Display, TIFF and DNG must apply the same selected correction. DNG must bake its chromatic effect through the existing shared edit route.

## Evidence and adaptation

The inspected estimator takes a neighborhood minimum of the smallest RGB channel, selects candidates with strong minimum values and a bounded intensity, and averages their original colors. Global and local selection use different preparation factors and intensity ceilings. Eighty isolated native synthetic checks establish the recovered selection and normalization equations. Raw preparation, mask routing and complete photographic appearance remain separate evidence boundaries.

The correction softly subtracts a haze contribution, limits transmission gain, restrains chromatic excursions and normalizes its upper response. A negative amount follows a haze-adding response. Working-space adaptation and calibration must be explicit. Reusing the equation does not establish complete appearance equivalence.

## Implementation slice

The owner approved the signed Amount and Auto/eyedropper/swatch design. Share the correction across supported camera families in Color and Monochrome. Monochrome must remain achromatic. Work in normalized linear RGB with explicit primaries. Preserve negative residuals and highlight headroom. An empty Auto estimate must leave the image unchanged. An empty manual pick must retain the previous settings and explain that a different area is needed.

The first slice uses whole-frame processing while Dehaze is active so Auto and manual estimates remain independent of preview size and viewport. Reduced and regional processing may resume after a stable shared analysis artifact is available. Validate photographic strength and placement without changing the zero default.

## Validation

Check recovered arithmetic, finite edge cases, exact zero bypass, saved-setting round trips, cached/fresh agreement, preview/export consistency and picker behavior during pending renders. Review haze removal and addition on hazy shadows, sky, foliage and low-contrast scenes. Keep unperformed photographic acceptance explicit.
