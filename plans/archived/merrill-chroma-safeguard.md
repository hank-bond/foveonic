---
summary: "The approved chroma safeguard is fused into Merrill texture restoration with matching photographic output and small measured cost."
status: archived
---

# Merrill chroma safeguard integration

The owner approved the more protective finalist from the [parameter sweep](./merrill-neutral-grain-sweep.md). Implement the accepted behavior in [detail and noise processing](../../docs/specs/detail-and-noise.md).

- [x] Fuse the correction into texture restoration using gain 1–2, weak-signal proxy 0.5–2 and input disagreement 0.20–0.45.
- [x] Prepare its neutral direction and RGB luminance from the following color conversion. Cover native, Fast, Smart and DNG recipes without changing Monochrome.
- [x] Include variance and color couplings in recipe keys. Preserve identity, diagnostic and disabled-group behavior.
- [x] Retain a test-only pre-adoption baseline and compare the fused pass against the approved standalone method.
- [x] Check weak false color, supported/noisy real color, signed inputs, chroma amounts and nonneutral detail.
- [x] Compare photographic output and completed texture latency, then run repository checks and prepare the app for testing.

The production path must not gain another stage, full-frame analysis, mask buffer, neighborhood pass, preset or user control. Research thresholds and rejected methods remain in the test-only harness. The approved behavior makes only extra restored brightness neutral when the filtered color estimate is weak and disagrees with the input. It preserves linear RGB luminance before ColorDQ, not necessarily displayed brightness after tone and clipping.

## Implementation

The following technical color matrix is prepared before texture recipe construction and supplied as metadata-derived coordinates. The actual color conversion remains after texture restoration. The node prepares a neutral sensor direction and linear sRGB luminance using the existing shared matrix inverse. Native active bounds scale to the working grid. Regional windows retain full sensor coordinates. DNG supplies its unselected technical conversion, and Monochrome branches before this preparation.

The correction is fused into the existing texture mix dispatch. Its gain is the actual restored gain, including selected Fine/Coarse amounts. Chroma zero and disabled Noise bypass it. Nonneutral Detail therefore retains the chosen brightness while its unreliable restored color is handled consistently. The existing Fine/Coarse inspection remains independent of chroma cleanup.

Texture cache identity now includes variance coefficients and prepared color coordinates. Persistent display previews use a new appearance signature so old rendered previews regenerate. Source decode bundles and saved recipes are unchanged.

## Validation findings

Thirty-five full-atlas comparisons cover all five Chroma amounts on seven Merrill photographs. Fused-versus-standalone projected-core differences are at most 0.000003815 camera codes. Outside-mask differences are exactly zero. Seven final photographic comparisons also match the approved standalone candidate within the declared floating-point tolerance. Four texture GPU tests cover signed cancellation, independent spatial bands, the brightness diagnostic, weak false color, noisy and supported real color, active bounds, immutable inputs, zero cleanup and nonneutral detail. Ordinary tests check matrix neutrality, invalid color coordinates and cache dependency changes.

Interleaved completed texture measurements use three warmups and twelve alternating rounds per photograph. They include uniforms and output allocation while excluding upstream filtering, metadata preparation and photographic continuation. Per-photo median differences range from approximately -0.44 to +1.40 ms, with a middle value near +0.09 ms against roughly 17–20 ms baseline texture work. Other GPU use was uncontrolled. These results show small measured cost without claiming universal timing.

All three native preview GPU tests pass, including Fast cached edits, independent section bypasses and Smart regional continuation across orientations and filtering context. Multiscale export-routing checks pass for technical DNG and photographic recipes. The preserved pre-adoption diagnostic also passes after moving its original texture computation into the frozen test-only shader.

Workspace tests, Clippy, formatting, five artwork tests and document checks pass. The GUI debug binary and native app bundle build successfully through the workspace task. The running app is not interrupted, preserving its current editing session. A new app launch uses the rebuilt implementation. No commit, release or sidecar migration is part of this change.
