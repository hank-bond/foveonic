---
summary: "GPU pattern conditioning deterministically permutes selected near-neutral luminance ratios after native grouped min/max masking and before residual-detail mixing."
kind: reference
status: accepted
implementation: conforming
---

# Pattern conditioning

## Requirements

- Full-resolution reconstruction **must** condition luminance ratios after diffusion/extrema/ratio creation and before residual-gain mixing.
- Native grouped transposed extrema extents, strict near-neutral min/max selection, deterministic xorshift128 offsets, row-major horizontal swaps, column-major vertical swaps, sequence wrapping, and final threshold/blend **must** be preserved. Swaps within a line **must** remain sequential; parallel lines **must** start at the native ordered selected-pixel prefix.
- The unwritten extrema scratch seed **must** explicitly retain the native F backing: full/local reduced LocalMean transposed writes inside margin 8, including their compact original allocation offsets; the global branch has zero seed. Mutable scratch aliases **must not** be exposed through artifacts.
- Ratio inputs and retained buffers **must** remain immutable. Photographic masking, counts, prefixes, swaps, and blending **must** execute on GPU; pure metadata/RNG preparation **may** execute on the host.
- Additional-table selection **must** use effective ISO for the row pair and clamped capture ISO for interpolation, including extrapolation. Chroma presets 3/4 **must** add their native blend adjustment. Native half-size limit 0–4 **must** be validated.
- Inspection **must** distinguish the original ratio, conditioned ratio, and pattern-selection mask. Earlier ratio inspection **must** skip pattern preparation/execution; mask-only inspection **must** skip permutations and residual mixing.

## Implementation and evidence

Typed `Ratio` creates `UnconditionedRatioTiles`; `Pattern` consumes them and an explicit immutable `PatternSeedTiles` dependency, producing `RatioTiles`; `Detail` mixes conditioned raw tiles, filtered base tiles, and conditioned ratios into residual gains. `detail.raw-ratio` is before pattern conditioning; `detail.ratio` is after. `pattern.mask` projects 0/4095 selected flags into a scalar visual buffer, with native overlap ramps producing gray boundaries. It is a texture-selection mask, not a camera-defect map. GUI and CLI inspection share these roots; reconstruction remains opt-in.

The component fixture uses edge-seeded, margin-limited transposed scratch, grouped written extents, negative/out-of-threshold ratios, sequence wrapping, blends 0/0.65/1, and two independent tiles. Native masks/offsets match exactly; ratio maximum error is below `0.0000003`. Inputs remain bitwise unchanged; zero ratios remain finite zero. Blend 0 is not a general Off operation: native swaps affect outer half-window margins outside the final blending rectangle.

Native parameter captures cover six originals × five chroma presets, including capture/effective-ISO extrapolation. Current GPU/fixture regressions pass, including six-photo/noise-choice finite and zero-gain checks, selective inspection, and retained ownership. [Reduced local/global integration](./reduced-reconstruction.md) now supplies explicit native F seeds independently of visual edge projection. Final photographic/native review remains tracked in the [completion plan](../../plans/complete-reference-algorithms.md); component conformance is not owner photographic acceptance.
