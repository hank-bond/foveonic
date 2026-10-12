---
summary: "Metadata-selected eighth-step reconstruction uses native polyphase reduced local or whole-image fixed filtering, restores full tile geometry, and preserves explicit transposed pattern scratch provenance."
kind: reference
status: accepted
implementation: conforming
---

# Reduced reconstruction

## Requirements

- Optional reconstruction **must** select its native eighth-quantized scale from effective-ISO/luminance metadata, minimum 0.375, including the 0.875 snap below the full-scale boundary.
- Local scales 0.625/0.75/0.875 **must** downsample raw tiles, run adaptive bilateral/edge/LocalMean at reduced geometry with the native 0.92 decay ceiling, then restore filtered RGB to full tile geometry. Scale 1 **must** retain direct full-resolution filtering.
- Scales 0.375/0.5 **must** prepare the working whole image, downsample once, apply fixed sparse bilateral and source blending, edge LUT margin 0, and LocalMean margin 4, then copy reduced tile contexts with native first-edge extension and restore full tile geometry. Column/detail/color/restoration **must** remain at their full-resolution positions.
- Polyphase preparation **must** preserve native Float32 coefficient generation, first-coefficient period detection, retained untrimmed source start, phase advances, signed top/left extension, linear right-row spill, and four untouched initialized-zero bottom rows. Photographic sampling **must** run on GPU.
- Compact local reduced source tails **must** remain zero rather than read the next processing tile. Final pattern scratch **must** be independently seeded from native reduced transposed F allocation writes; global F **must** be zero. Reconstructed, original, and retained buffers **must** remain immutable.
- Inspection **must** expose truthful full image geometry. Reduced edge diagnostics **may** use polyphase projection to the full tile lattice; this diagnostic **must not** substitute for the native compact transposed pattern seed.

## Implementation and evidence

Shared `Parameters` includes native scale, fixed window, and explicit mean/LUT margins. `resample.rs` prepares geometric coefficient phases; `resample.wgsl` samples immutable images. The base operation chooses the selected recipe. `noise.pattern-seed-tiles: PatternSeedTiles` replaces the prior raw edge alias and is an explicit pattern dependency. Base/edge roots remain selective; column/ratio/mixing/restoration stay at original sensor/tile extent. Original source/settings/crop/orientation provenance remains attached to projected inspection images.

Native fixtures cover coefficient metadata at all six down/restore scales, two independent tile resets, RGB/scalar sampling, zero bottom rows, reduced local filtering/restoration/seed, global fixed filtering/margin-4 means, and global half-size restoration. Coefficient metadata is exact; polyphase output error is below 0.005 codes. Current reduced fixture maxima: global bilateral 0.000123 codes, global fixed/mean 0.06745 codes, local restoration 0.005311 codes, half-size global restoration 0.000306 codes, and local seed 0.00000653. Float32/native-double edge decisions can differ by a single LUT bin (measured weight maximum 0.004208); native thresholds remain unchanged. The global fixture exposed previously invisible zeroed rotating-row columns, now reproduced explicitly by GPU edge generation.

Thirty native metadata cases confirm scale/window selection. Six photographs × five noise choices stay finite, including zero-gain displayed black. All current ignored GPU/fixture tests, ordinary workspace tests, and strict Clippy pass; integrated identical-input photograph replay and owner acceptance remain in the [completion plan](../../plans/complete-reference-algorithms.md). The photographic output crop/resize kernel is distinct from this triangular processing-buffer resampler and remains a separate task.
