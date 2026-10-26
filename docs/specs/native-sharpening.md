---
summary: "Optional GPU sharpening uses signed high-pass before exposure/tone, with distinct color-proportional and additive monochrome applications."
kind: reference
status: accepted
implementation: incomplete
---

# Native sharpening

## Requirements

- Selected sharpening **must** apply native signed mono high-pass arithmetic before Exposure/tone and photographic resizing, not Gaussian per-channel unsharp masking after tone.
- Merrill color sharpening must consume the actual reconstructed linear RGB. The engine generates native mono high-pass from nonnegative RGB brightness with unity ratio and the 32767 brightness ceiling, independently of optional noise cleanup and texture restoration.
- [Monochrome rendering](./monochrome-rendering.md) must instead derive signed high-pass from its filtered mono plane, apply processing gain before ISO-specific magnitude and ±35% limits, and add the quarter-code packed detail equally to neutral channels. It must not apply color brightness-proportional sharpening.
- Color application **must** retain upper-only 8191 signal ceiling, quarter-code packing by `signal*4+0.5` → truncating i32 → wrapping i16, nonnegative RGB brightness proportions, absolute ±1000 and relative ±35% limits, and output ceiling 32767.
- Inputs and retained artifacts **must** remain immutable. Photographic signal generation, packing, and application **must** run on GPU.
- Sharpening **must** offer Off, Auto, and a finite nonnegative manual amount, defaulting to Off. Merrill color Auto must resolve to zero. Monochrome Auto retains the camera/noise recipe's nonnegative sharpening base. Color and monochrome **must** retain their distinct applications. Final export dimensions **must not** scale the amount.
- GUI, CLI, sidecars, inspection, interactive reuse, and DNG technical processing **must** share this interpretation. Numeric sidecar amounts **must** retain their manual meaning. Explicit null **must** select Auto. Omitted values **must** retain default Off. Final sharpening must remain independent of Fine/Coarse detail amounts and sizes.
- Gaussian radius, blur operations, blur inspection roots, and `--sharpen-radius` **must** be removed rather than retained as a second photographic path.

## Implementation and evidence

`detail.signal` remains raw floating signed detail generated from reconstructed linear RGB. Packing occurs only inside native `Sharpen`; `sharpen.rgb` is linear RGB before tone. Tone/saturation roots therefore include sharpening when enabled. Earlier roots remain selective. GUI and CLI share `sharpen_amount`/`--sharpen`. Zero is Off, positive values are manual effective amounts, and null/`auto` selects zero for Merrill color or the camera-derived monochrome base. The GUI A-toggle selects Auto and shows its resolved amount when camera metadata is available. A zero Auto amount bypasses native clamps and preserves signed RGB/headroom, matching Off. The obsolete Radius control is gone.

Native component fixture covers signal margins, signed/zero/HDR RGB, quarter-code boundaries, negative wrapping, ceilings, proportional/absolute/relative limits, and amounts 0/0.75/2. Maximum component error is below 0.005 codes. Identical-input final-application replay on six full sensors × reconstruction off/on has maximum error 0.003907 codes. Integration tests confirm both signal recipes, linear working-space typing, no tone execution for sharp inspection, zero-gain black in the displayed crop, and retained input/signal immutability. The owner accepted the earlier manual-sharpening photographic results. New Auto coefficient/persistence/UI checks and exact Auto/manual color, monochrome, cached/fresh, and DNG-base comparisons pass. Broader exact-output signoff remains blocked by an existing cache-equality failure recorded in the [implementation plan](../../plans/tiff-color-and-auto-sharpen.md). Remaining algorithm integration and coverage work is tracked in the [completion plan](../../plans/complete-reference-algorithms.md).
