---
summary: "GPU monochrome rendering, a triangular layer mixer, mode-specific editing, and linear monochrome DNG export are implemented and checked."
---

# Implement monochrome rendering

Requirements: [monochrome rendering](../docs/specs/monochrome-rendering.md), [processing graphs](../docs/specs/processing-graphs.md), [render resources](../docs/specs/render-resources.md), [interactive rendering](../docs/specs/interactive-rendering.md), [output strategies](../docs/specs/output-strategies.md), and [editing sidecars](../docs/specs/editing-sidecars.md).

## Scope and approval

The owner approved the full monochrome implementation after the rendering reorganization in `4d529cd`. Subsequent approvals covered histogram optimization, hiding inactive controls, and cleanup followed by commit and push.

- [x] Inspect reference processing, controls, WB/gain conventions, tone, and export boundaries.
- [x] Add shared settings, validated layer proportions, sidecar persistence, and CLI options.
- [x] Implement GPU mixing, directional filtering, column correction, histogram filtering, despeckling, signed detail, gain replication, and sharpening.
- [x] Connect selective inspection, tone, cache invalidation, and retained ownership.
- [x] Add the rendering toggle, triangular mixer, mode-specific controls, and help.
- [x] Export linear neutral monochrome DNG with separate presentation metadata.
- [x] Check reference arithmetic, original photographs, cached edits, exports, UI interaction, and color goldens.

## Processing decisions

The reference implementation's secondary-monochrome chain uses ordinary calibration. Primary-monochrome calibration is unverified and was not selected. WB-normalized layer weights share a factor of `1 / (2 * min(raw WB))`. Triangle coordinates are actual proportions normalized once, rather than the reference implementation's sequential normalization.

After shared sensor corrections, the recipe initializes working borders to camera code 1000, clamps and mixes layers, then applies directional means, 40-row column-offset blocks within eight fixed strips, 19×19 histogram means, despeckling, signed detail, processing gain, optional additive sharpening, and monochrome tone. Noise 0 skips the histogram but retains column correction and detail. Its despeckle neighbors come from the directional plane rather than the corrected monochrome plane.

Monochrome brightness has its own scalar type. Operation discriminants and distinct output names prevent invalid color/mono cache reuse. Equal-channel RGB provides display, TIFF, and DNG interoperability without preserving recoverable sensor color. Color matrices, ColorDQ, residual color reconstruction, and saturation are bypassed.

`Settings::rendering` and `Settings::monochrome` persist both modes without a sidecar version bump. The CLI uses the same settings but does not load sidecars. The GUI hides irrelevant controls and retains their values. Shared editing controls remain available in both modes.

## Numerical and scheduling departures

The reference implementation carries Float32 histogram residuals across adjacent columns. GPU columns initialize independently while retaining vertical traversal and strip boundaries. The mathematical filter is unchanged, but rounding history differs. The complete 48×48 reference fixture measures maximum directional, column, histogram, and despeckle differences of approximately 0.000008, 0.000023, 0.000191, and 0.000168 camera codes.

Large private histograms produced nonrepeatable output on Apple M1 Pro Metal. Reducing workgroup width did not fix it. Workgroup-owned state passed the full-sensor checks but was slow. The [performance pass](./archived/monochrome-performance.md) replaced it with column-parallel GPU scratch and retained byte-identical output to the implemented filter. Full-sensor repeatability checks remain necessary when changing storage or traversal.

At extreme tone settings, Float32 Bezier evaluation introduced tiny inversions on a nearly flat highlight plateau. A mono-only prefix maximum restores monotonicity for DNG presentation. Native eight-bit tone samples still agree. Color tone is unchanged.

The interactive cache retains continuation boundaries, not every finished result. Inactive color edits do not invalidate additional monochrome work, but an unchanged request can still rerun a suffix.

## Validation

The final workspace passes 79 ordinary processor, three CLI, 113 GUI, and 18 documentation-tool checks. The complete serial GPU suite passes 69 processor and eight GUI/worker checks. Strict all-target Clippy, formatting, generated indexes, and whitespace checks pass. An earlier color comparison failed intermittently and passed on rerun. The [mismatch investigation](./gpu-output-mismatch.md) remains unresolved.

Tests cover pure layers, noise and banding presets, sharpening, selective inspection, scopes, textures, retained ownership, fresh/session equality, repeated stages, hidden settings, mode switches, and TIFF/DNG boundaries. All seven originals produce finite, nonblack, neutral monochrome output. The all-enabled Sunlight/Vivid color render retains digest `57dfda91a95f6e08` and exact float/TIFF goldens.

Adobe DNG SDK 1.7.1 (2724) validates three 800×1200 monochrome exports without warnings. Independent strip extraction matches SDK stage-1 decoding for all 8,640,000 components. A native 3136×4704 export validates and matches all 44,255,232 decoded components. Every stored RGB triplet is neutral. Exposure/tone changes preserve encoded samples while changing presentation metadata.

On the blue example, SDK/native RGB16 presentation differs by approximately 0.465% mean and 2.61% maximum. Presentation is not claimed exact across engines.

## Remaining acceptance

Owner photographic review and Capture One/Lightroom appearance and editing-latitude checks remain external acceptance. No high-ISO photographic survey was available. The established column table ends at estimated ISO 6400, and larger requests fail explicitly. No installation, native GUI launch, or default generated-output change was made during this work.
