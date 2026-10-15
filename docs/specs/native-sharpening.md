---
summary: "Optional GPU sharpening uses signed high-pass before exposure/tone, with distinct color-proportional and additive monochrome applications."
kind: reference
status: accepted
implementation: conforming
---

# Native sharpening

## Requirements

- Selected sharpening **must** apply native signed mono high-pass arithmetic before Exposure/tone and photographic resizing, not Gaussian per-channel unsharp masking after tone.
- In color rendering with reconstruction, sharpening **must** consume its ratio-aware pre-residual signed signal and post-residual RGB. Without reconstruction, the engine **must** generate native mono high-pass from nonnegative linear RGB brightness with unity ratio and the 32767 brightness ceiling.
- [Monochrome rendering](./monochrome-rendering.md) must instead derive signed high-pass from its filtered mono plane, apply processing gain before ISO-specific magnitude and ±35% limits, and add the quarter-code packed detail equally to neutral channels. It must not apply color brightness-proportional sharpening.
- Color application **must** retain upper-only 8191 signal ceiling, quarter-code packing by `signal*4+0.5` → truncating i32 → wrapping i16, nonnegative RGB brightness proportions, absolute ±1000 and relative ±35% limits, and output ceiling 32767.
- Inputs and retained artifacts **must** remain immutable. Photographic signal generation, packing, and application **must** run on GPU.
- Shared effective amount **must** remain finite/nonnegative, default 0 Off. Final sharpening **must** remain independent of residual Detail. Native preset-relative/automatic sharpening is not required to replace the selected optional algorithm.
- Gaussian radius, blur operations, blur inspection roots, and `--sharpen-radius` **must** be removed rather than retained as a second photographic path.

## Implementation and evidence

`detail.signal` remains raw floating signed detail, projected from tile cores with reconstruction or generated from linear RGB without it. Packing occurs only inside native `Sharpen`; `sharpen.rgb` is linear RGB before tone. Tone/saturation roots therefore include sharpening when enabled. Earlier roots remain selective. GUI and CLI use the same `sharpen_amount`/`--sharpen` effective amount, with existing 0-Off behavior; the obsolete Radius control is gone.

Native component fixture covers signal margins, signed/zero/HDR RGB, quarter-code boundaries, negative wrapping, ceilings, proportional/absolute/relative limits, and amounts 0/0.75/2. Maximum component error is below 0.005 codes. Identical-input final-application replay on six full sensors × reconstruction off/on has maximum error 0.003907 codes. Integration tests confirm both signal recipes, linear working-space typing, no tone execution for sharp inspection, zero-gain black in the displayed crop, and retained input/signal immutability. Owner photographic review and remaining reduced-resolution/default-recipe work are tracked in the [completion plan](../../plans/complete-reference-algorithms.md).
