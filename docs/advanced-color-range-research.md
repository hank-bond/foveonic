---
summary: "Color-wheel inspection separates soft range selection, overlapping adjustments and a possible selective lightness inversion."
kind: reference
---

# Advanced color range research

The owner suggested selecting the blue sky and inverting its lightness while retaining other colors. This follow-up studies soft color-range selection independently of a decision to implement it. The existing global [lightness inversion prototype](./specs/lightness-inversion.md) remains unchanged.

## Interaction and selection

The studied workflow picks a color, adjusts the selection's boundary handles and Smoothness, and shows the affected range by desaturating other colors. Users can add, enable, disable and delete individual corrections. The documented limit is 30 corrections.

The inspected wheel constructor uses HSV-style hue and saturation derived from stored source RGB. Angular boundaries select hue. Inner and outer arcs select saturation. Stored offsets describe the selection relative to the source color. The wheel does not plot lightness. Its display support expands with Smoothness. This display formula does not establish an identical photographic mask.

The photographic operation uses a separate Lab/LCh representation. Its initialization has versioned conversions, including an Adobe RGB HSV to Lab/LCh D50 route. Treat the wheel, picked RGB and photographic correction as distinct domains. Later wheel redesigns are outside this inspection.

## Additional numerical evidence

The independent color-range follow-up inspects nine bounded wheel functions and probes native selection arithmetic. Fourteen additional configurations compare 24,500 scalar values against the original, previously validated point-correction ABI. Maximum error is below 0.000001. These checks supplement the [earlier inspection](./color-and-tone-research.md#local-numerical-inspection), rather than changing its reported 184 configurations.

| Question | Observed behavior in the tested correction function |
| --- | --- |
| Selection interior | Full weight within the hue and chroma intervals. Hue intervals wrap around the circle. |
| Boundary blending | Cubic smoothstep falloff outside each interval, with separate widths at either end. |
| Combined boundaries | Hue weight multiplied by chroma weight. A corner in both falloffs receives less weight. |
| Selection inversion | Complement the combined weight. This is distinct from reversing lightness. |
| Lightness dependence | Holding hue and chroma constant gives the same selection weight across the tested Lab lightness values. |
| Neutral protection | A chroma interval reaching zero can fully select arbitrarily low-chroma samples. The mask has no automatic neutral fade in this test. |
| Preview | A neutral adjustment leaves Lab lightness unchanged and scales color components by capped selection coverage. |
| Overlap | Two tested groups normalize accumulated corrections above unit coverage. Another adds them. Reordering the tested additive corrections leaves the result unchanged within floating precision. |

The overlap group numbers are internal values. This follow-up does not prove which groups every GUI tab produces. The correction record's selection-weighting flag must not be mistaken for a row-enable flag. Exact handle event routing, every Smoothness conversion and full photographic equivalence remain unvalidated.

Selection inversion selects everything outside a range. Selective lightness inversion would be a new Foveonic effect using the selected range.

## Candidate Foveonic prototype

A possible Advanced tab could keep Basic's existing swatches and offer an eyedropper, hue and saturation boundaries, Smoothness, a selection preview and a list of enabled edits. Each edit could carry Hue, Saturation, Lightness and Invert lightness. The selected first prototype is specified separately in [Advanced color range](./specs/advanced-color-range.md). The selection findings do not establish numerical equivalence.

Use the existing stable color guide for classification so adjustments do not change their own selection. Keep selection weights separate from adjustment math. Reuse our perceptual conversions and lightness inversion rather than importing the inspected operation's internal units or version branches. An explicit neutral fade would protect clouds and gray objects from unstable hue selection. A selected range would affect matching colors throughout the photograph, including other blue objects.

Overlapping inversions need a deliberate policy. Sequential full inversion twice restores the original. Combining their selections and applying inversion once would avoid that cancellation. For one range with weight `w`, a candidate response is `L' = mix(L, 1-L, w)`, retaining the color components. At half strength this becomes constant lightness, so feathered boundaries can flatten contrast. Photographic feedback must evaluate those transitions, noise and destination gamut limits.

A compact range table evaluated in one GPU pass could share our existing guide and rendering/export boundary. A separate full-image mask or processing pass for every row is unnecessary. Performance and exact overlap behavior remain prototype decisions, not measured claims.
