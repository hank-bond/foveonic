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

Shadow tone starts with Auto off and a fixed neutral manual color of [0.5, 0.5, 0.5] in linear sRGB. Offer an eyedropper that estimates the haze color from a selected image patch, a swatch showing the selected tone, and a way to return to Auto. Sample an uncorrected processing image. Keep stored processing RGB separate from the display conversion used for the swatch.

In Auto, the swatch must show the actual estimated color, including when Amount is zero. Turning Auto off must copy an available estimate into the manual color at processing precision so the owner can adjust it without an initial image change. If no valid estimate exists yet, turning Auto off uses the fixed manual default. Editing the swatch directly must start from the visible estimate. Resetting Corrections or all editing controls must respect the configured defaults, including the Auto/manual choice. Never show another photograph's estimate or a fabricated substitute while estimation is pending or unavailable.

Use the shared `A` button for Auto. The color swatch must match the adjacent square controls' height and be twice as wide as it is tall. While the automatic color is pending or unavailable, use the same rectangle with a transparent fill and an X joining opposite corners. Keep the row layout stable and omit a status text label.

Zero must omit correction and brightness-calibration work. Background color estimation must run only while Auto is selected and Corrections is enabled. Explicitly enabling Auto may estimate its swatch at zero Amount. Nonzero manual correction must skip dark-neighborhood filtering and haze-color candidate selection, retaining only the brightness calibration its response requires. A user-requested local color pick remains available independently of Auto.

Auto must analyze a stable whole-photo basis. Changing the viewport, preview quality or Dehaze amount must not select a different haze color. Manual picking must use the same numerical estimator on a local patch. Estimation must account for the image's white balance, primaries and nominal white. Manual colors must use normalized linear sRGB so saved colors retain their meaning across camera working spaces.

Dehaze must support the existing quarter-pixel Fast mode on supported color recipes. Whole-frame estimation does not require full-resolution correction. Full, Fast and regional previews must use the same whole-frame analysis. Export keeps its normal Full resolution.

Share GPU estimation and correction through the existing graph. Retain immutable inputs and reuse unchanged analysis while Amount changes. Apply correction at the shared linear photographic boundary before the camera tone response. Sidecars, scoped presets, defaults, clipboard and frozen batch requests must retain Amount, Auto/manual intent and manual color. Display, TIFF and DNG must apply the same selected correction. DNG must bake its chromatic effect through the existing shared edit route.

## Evidence and adaptation

The inspected estimator takes a neighborhood minimum of the smallest RGB channel, selects candidates with strong minimum values and a bounded intensity, and averages their original colors. Global and local selection use different preparation factors and intensity ceilings. Eighty isolated native synthetic checks establish the recovered selection and normalization equations. Raw preparation, mask routing and complete photographic appearance remain separate evidence boundaries.

The correction softly subtracts a haze contribution, limits transmission gain, restrains chromatic excursions and normalizes its upper response. A negative amount follows a haze-adding response. Working-space adaptation and calibration must be explicit. Reusing the equation does not establish complete appearance equivalence.

## Implementation slice

The owner approved the signed Amount and Auto/eyedropper/swatch design. Share the correction across supported camera families in Color and Monochrome. Monochrome must remain achromatic. Work in normalized camera RGB with explicit recipe-supplied transforms. Monochrome uses its existing achromatic signal. Preserve negative residuals and highlight headroom. An empty Auto estimate must leave the image unchanged. An empty manual pick must retain the previous settings and explain that a different area is needed.

Auto analyzes an area-averaged overview of at most 512 pixels per axis. Supported color recipes supply a fixed quarter-pixel whole-frame Fast basis, independently of the correction's resolution and viewport. Other recipes use their whole-frame linear input. Manual selection uses a supported patch of up to 133 native pixels per axis. Both quantize analysis samples to 16 bits and use bounded dark-channel candidate averaging. A 256-bin channel histogram estimates the lower and upper response levels.

Signed correction uses the recovered pointwise response in white-normalized camera RGB, retaining negative residuals and unclipped highlight inputs. Camera recipes supply their own selected matrix and the raw brightness proxy. Convert the existing cleaned linear image into camera coordinates and back inside the same correction kernel. Preserve reconstruction order, preview geometry and the zero bypass.

Store the estimated or manually selected color in normalized linear sRGB for the swatch and sidecars, and transform it into the same camera basis for correction. Estimation and picking must select candidates and derive response levels in that camera basis too. Their canonical color output must use the same bounded transport for Auto and manual application so freezing Auto remains exact. Validate photographic strength and placement without changing the zero default.

## Validation

Check recovered arithmetic, finite edge cases, exact zero bypass, saved-setting round trips, cached/fresh agreement, preview/export consistency and picker behavior during pending renders. Review haze removal and addition on hazy shadows, sky, foliage and low-contrast scenes. Keep unperformed photographic acceptance explicit.

## Shadow-signal protection

Positive Dehaze must share a signal-preserving shoulder across Classic, Merrill and Quattro, including Monochrome and DNG baking. Evaluate the proposed camera-basis correction back in the working RGB basis, then blend toward the original as it consumes weak channel signal. Use one weight for RGB so protection does not independently clamp channels. Existing signed input remains signed. Zero and negative Amount retain their existing response, and empty estimates remain exact bypasses. The saturation clipping toggle does not control this Dehaze-specific protection.

For each channel, compute the positive relative loss `(input - candidate) / max(abs(input), 1e-6 * code_white)`. Let `r` be the greatest positive loss and blend the whole RGB correction with weight `1 / (1 + r)`. This makes the greatest relative darkening approach `r / (1 + r)` while preserving meaningful positive signal. Use absolute input magnitude to keep the weight continuous through signed zero. Existing negative channels remain signed while further losses are restrained. Monochrome blends from its existing green-derived neutral signal. Retain exact candidate values when no darkening needs protection.

Fuse this calculation into the existing pointwise pass. Add no image buffers, analysis passes or new sensor dependencies. Keep color estimation independent of Amount and protection, and preserve exact Auto-to-manual freezing. Validate protected photographs separately from the independent unprotected reference-equation fixtures.
