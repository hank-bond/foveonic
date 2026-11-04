---
summary: "A default-off Tone prototype reverses perceptual lightness, retains chromatic components and exports through shared linear edits."
kind: reference
status: accepted
implementation: conforming
---

# Lightness inversion prototype

The owner requested luminance inversion while retaining chroma and selected an editable effect saved with the photograph and carried into exports. Monochrome must invert tones without introducing color.

The initial prototype must offer Invert lightness in Tone for both Reference and Universal methods. It defaults off. Tone bypass, processing-section omission, resets, presets and defaults must include the choice through the existing photographic recipe. The CLI must expose `--invert-lightness`.

Invert Oklab lightness using `L' = 1 - L`, retaining `a` and `b`. Normalize by nominal white and Exposure so Exposure prepares the signal being inverted. Apply the operation after other shared linear edits and before the camera response. Monochrome must use the equivalent scalar response and produce exactly equal RGB channels. Preserve signed values and headroom without a new RGB clamp. Existing display/export gamut handling remains applicable, so fixed chromatic components may exceed a destination gamut near the inverted endpoints.

False must be an exact processing bypass. Full, Fast and regional rendering must share the same pointwise arithmetic. Retained inputs and source samples must remain immutable. Photographic sample arithmetic must stay on the GPU and reuse the shared Oklab conversion. Inspect must expose the operation's input and output.

TIFF must include the effect through ordinary photographic rendering. DNG must bake the complete edited RGB through its existing technical basis mapping. Inversion must handle exact black becoming white, including a Monochrome storage basis that collapses to one signal. Multiplicative gain transfer is insufficient for that endpoint. Keep camera appearance deferred through the existing profile.

Mathematical behavior and pipeline integration are the scope of this prototype. Photographic feedback may change the response or placement before broader release. The [plan](../../plans/lightness-inversion.md) tracks implementation and checks.
