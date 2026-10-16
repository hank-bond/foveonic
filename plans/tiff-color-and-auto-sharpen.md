---
summary: "Add selectable TIFF destination color spaces and camera-derived Auto sharpening without changing existing defaults."
---

# Add TIFF color selection and Auto sharpening

The owner approved TIFF color-space selection and Auto sharpening. They also permitted reading the reference implementation for the strength calculation and copying its attributed ROMM ICC asset. Gray gains, noise-slider interpolation, processing-resolution presets, and photographic resizing are outside this slice.

Requirements: [output strategies](../docs/specs/output-strategies.md), [native sharpening](../docs/specs/native-sharpening.md), and [editing sidecars](../docs/specs/editing-sidecars.md).

## Steps

- [x] Add TIFF-only sRGB, Adobe RGB, and ProPhoto RGB conversion with matching profiles. Preserve the camera space by default.
- [x] Add shared Auto sharpening using the noise-table base. Preserve manual amounts and default Off.
- [x] Connect GUI, CLI, sidecars, inspection, caching, and DNG technical sharpening.
- [x] Check transforms, transfer curves, embedded profiles, automatic/manual equality, persistence, and unchanged defaults.
- [x] Run ordinary workspace tests, full serial core/GUI GPU suites, and strict Clippy. Core exact-cache failure is recorded below.
- [x] Complete documentation/index checks and owner review for commit.
- [x] Owner approved committing the implemented features, interface refinements, and refreshed README screenshot.
- [ ] Broader exact-output signoff remains blocked by the [unresolved GPU mismatch](./gpu-output-mismatch.md).

## Findings

The reference implementation's Auto amount with user control zero, HI preset, and unit reconstruction-relative scales is the nonnegative Float32 product of noise-table column 12 and the selected luminance-slider column 12. Monochrome uses the same base with its own sharpening application. Final export dimensions do not scale sharpening.

The copied ROMM V4 profile comes from CC0 Compact-ICC-Profiles. Its transfer includes a linear toe below linear 1/512, followed by gamma 1.8. TIFF destination conversion must occur after tone/saturation and before transfer/clipping. Identity export must bypass matrix arithmetic to preserve existing output.

Concurrent owner changes reorganize existing plans and update specifications. They were preserved.

## Implementation and checks

`Settings::tiff_color_space` is an optional TIFF destination. None preserves the camera space. Working-space and encoded-destination types remain separate. GPU export transforms tone-mapped samples through the embedded profiles' D50 colorants before transfer and clipping. Identity and neutral monochrome conversions bypass matrix arithmetic. Synthetic GPU TIFFs cover both source spaces, every destination, neutral samples, negative/headroom input, ProPhoto's toe, exact identity-file equality, profile bytes, and unchanged preview/storage.

`sharpen_amount` is now optional. Null is Auto, numeric values keep their existing manual meaning, and default zero remains Off. Auto resolves through shared noise preparation, even without color reconstruction. A zero camera base preserves signed RGB and headroom instead of invoking the native kernel's incidental clamps. GUI A-toggle and TIFF dropdown pointer tests pass. CLI parsing, sidecar defaults/round trips, coefficient interpolation, selected luminance, zero base, Detail/output-size independence, and parameter identities pass.

On an original blue-sign photograph, Auto equals its explicit manual amount exactly across reconstruction off/on, monochrome, selected noise endpoints/midpoint, cached/fresh rendering, retained snapshots, and DNG technical bases. TIFF choices leave working and DNG samples unchanged. Existing component and photographic regressions passed except the broad cache-equality case below.

Ordinary workspace checks pass: 82 core tests, 3 CLI tests, 115 GUI tests, 19 documentation-tool tests, and 16 task tests. Strict workspace all-target Clippy, generated documentation/index checks, formatting, and whitespace checks pass. The complete serial core ignored run passed 71 tests and failed one existing exact-cache regression. All eight GUI ignored tests passed independently afterward. No failed comparison was retried or weakened.

The failure was `active_inputs_match_fresh_outputs_and_keep_retained_images_immutable`, leaves photograph, Detail step 2 (0.8), manual sharpening 0.75, camera TIFF space. It reported 29,432,493 differing float components, maximum absolute difference 0.004963994, first component 12,011,991 with 0.09225379 versus 0.09225992. The prior blue-sign sequence passed. The first divergent stage was not captured and no cause is established. This is additional evidence for the existing mismatch investigation, not a claimed fix or proof of feature causation. The owner approved committing the implemented feature and interface changes. This does not establish broader photographic or exact-output signoff.

## Commit checkpoint

The separate photographic-resizing slice is now committed. Its export regression also checks all three TIFF destinations after sampling, with matching profiles, one image readback, signed/headroom inputs, and neutral monochrome. The four targeted export GPU/original-file checks passed together at the final checkpoint, including Auto/manual color and mono, DNG technical bases, native-size destinations, zero Auto strength, and resized destinations. The failed broad cache-equality comparison was not retried. Ordinary workspace tests, strict all-target Clippy, formatting, and documentation/index checks pass. The [toolbar plan](./archived/export-toolbar.md) records owner interface approval and the refreshed README screenshot.
