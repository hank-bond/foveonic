---
summary: "Merrill GPU reconstruction separates noise cleanup from independently sized Fine and Coarse brightness detail."
kind: reference
status: accepted
implementation: conforming
---

# Detail and noise processing

This specification describes Merrill color reconstruction. [Monochrome rendering](./monochrome-rendering.md) and Quattro retain their own algorithms and controls. The owner accepted the neutral portrait rendering and selected this approach to replace the earlier reference residual-gain restoration.

## Requirements

- Noise filtering and texture restoration must run on GPU through the shared GUI/CLI engine, including Full TIFF/DNG, standalone Fast, and regional Smart refinement.
- New Merrill photographs opened in the GUI without a saved recipe must start with Noise and Detail enabled independently. Existing explicit off selections remain authoritative. CLI selection remains explicit through reconstruction options.
- Chroma and Luma noise must control cleanup independently of texture contrast. Fine and Coarse detail must offer separate amounts and sizes. Broad shading must remain available at zero detail amounts.
- Full processing and inspection must retain native framing. [Fast previews](./merrill-fast-preview.md) use their reduced grid with the requested controls. Smart's initial Draft bypasses optional reconstruction without changing saved edits.
- Retained images and original sensor samples must remain immutable. Inspection must compute requested outputs without changing the photograph.

## Reconstruction

The horizontal recursive mean uses transposed RGB and edge layouts so neighboring GPU lanes access contiguous samples. It preserves each line's original arithmetic order and restores native tile layout before the vertical scan. RGB workspaces are reused, with one additional scalar edge buffer.

The camera-derived noise base retains the reference implementation's bilateral/edge filtering, recursive means, diffusion and pattern conditioning at the middle noise selection. Metadata still selects full-resolution, reduced local or global filtering as described in [reduced reconstruction](./reduced-reconstruction.md). [Column conditioning](./column-conditioning.md) retains its separate five Banding noise presets. Shared calibration, repair, despeckle, [row correction](./row-banding.md), and [highlight restoration](./highlight-restoration.md) retain their positions.

Luma noise blends original column-conditioned brightness toward cleaned brightness. The stabilizing epsilon in the conditioned ratio is undone before restoration. Chroma noise blends original and filtered sensor-layer color proportions at a common camera-weighted brightness. Both choices run from 0 to 4, starting at 2. Luma cleanup is linear in that choice. Chroma cleanup uses `1 - (1 - choice/4)^2`, removing three quarters of chromatic residual at the middle setting. These edits do not change base filtering, pattern selection, automatic ColorDQ, or texture gain. Zero cleanup with unit detail must reproduce the column-conditioned sensor samples exactly, including signed shadows. Other selected sensor corrections still apply.

Fine and Coarse amounts act on log-luminance residual bands. Zero suppresses a band, one restores it neutrally, and two enhances it. Four separable binomial steps start with native strides 1, 2, 4 and 8. The first pair separates Fine from Coarse. The last pair separates Coarse from broad shading, which is always restored at unit strength. Fine size and Coarse size independently dilate these pairs from 0.5× to 2×, starting at 1×. Fractional taps interpolate between samples. The bands share their boundary, so changing Fine size also changes the smaller-scale edge of Coarse. Maximum support is 60 pixels inside the existing 64-pixel processing-tile context. Fast scales the default strides to its working grid before applying size multipliers.

Both detail amounts start at one. At those values, the bands recombine neutrally and the spatial pyramid is skipped, including for the neutral change map. One effective-control resolver treats disabled and neutral detail identically for recipe construction, inspection availability and GPU preparation. Saved enable choices and sizes remain unchanged. With Noise off and Detail neutral, the recipe bypasses the entire optional filtering foundation. When luminance cleanup is zero, photographic texture processing must not depend on luminance diffusion or pattern conditioning. Explicit inspection of those stages may still run them. Column-only cleanup must not require the filtered base. Lower amounts deliberately soften their bands. These frequency bands cannot distinguish hair from pores at the same scale.

Restoration stays in calibrated sensor tiles before camera color conversion and ColorDQ. ColorDQ uses its camera-derived amount with the middle-preset cap unless manually set. [Final sharpening](./native-sharpening.md) remains a separate edit using the actual reconstructed RGB. Merrill color Auto sharpening is zero. Manual sharpening retains its meaning.

## Controls and inspection

`Settings::reconstruction` stores shared `noise::Controls`. Merrill has independent Noise and Detail section checkboxes, styled with the same subdued text as other section headings. Noise contains Chroma, Luma and Banding rows. Detail contains Fine and Coarse amounts, each paired with its indented Size row inside a padded linked group. Unchecked sections keep their controls visible but disabled so the layout remains stable. Each section sizes its own compact label column. Slider tracks and numeric fields remain aligned within each section. Numeric fields should fit normal signed values without an extra em of unused width. ColorDQ gain remains under Noise → Advanced with its existing automatic/manual toggle. There is one Merrill reconstruction method.

Disabling a section must preserve all its values. Disabled Detail uses unit Fine/Coarse amounts and skips the spatial detail pyramid. Disabled Noise bypasses chroma/luma cleanup and column correction. Detail can still use the filtered sensor base to separate its frequency bands. Disabling both groups bypasses their tile processing. An enabled neutral Detail section with Noise off takes the same bypass. Automatic ColorDQ follows Noise independently of Detail. Explicit ColorDQ gain remains authoritative, including zero. Shared sensor corrections remain independently selectable. Smart Draft temporarily bypasses both groups without changing these saved selections.

CLI controls are `--reconstruction`, `--chroma-noise`, `--luma-noise`, `--banding-noise`, `--fine-detail`, `--coarse-detail`, `--fine-size`, and `--coarse-size`. Parameter options enable reconstruction. `--smooth-crisp` belongs to Quattro.

Useful inspection roots include:

| Output | Meaning |
| --- | --- |
| `noise.layers` | Filtered calibrated sensor base. |
| `noise.edges` | Recursive smoothing weights, nominal 0–1. |
| `column.layers` | Column-conditioned sensor input. |
| `detail.raw-ratio` / `detail.ratio` | Brightness ratios before/after pattern conditioning, neutral 1. |
| `pattern.mask` | Pattern-conditioning selection, independently of camera defects. |
| `texture.change` | Combined Fine/Coarse gain relative to neutral detail, excluding denoising and later color/sharpening. |
| `color.rgb` / `color-dq.rgb` | Restored sensor layers converted to working RGB before/after ColorDQ. |
| `reconstruction.rgb` | Working RGB projected from processing tiles. |
| `detail.signal` | Signed high-pass of reconstructed RGB for optional final sharpening. |

`texture.change` is a scalar multiplicative brightness map. One displays as middle gray, lighter means increased and darker means decreased brightness. Both amounts at one produce flat gray. Set an amount away from one to inspect the effect of its size. Individual Fine/Coarse inspection maps are retired. Projection is not a separate correction, so `reconstruction.rgb` does not expose a before/after comparison pair.

## Saved settings and migration

The independent enable flags, four texture amounts and sizes persist with the recipe. Older populated reconstruction settings enable both groups. An older null reconstruction setting leaves both off. Merely displaying an old recipe must not populate or rewrite it. Saved experimental texture values are retained while the retired `enabled` flag is ignored. Earlier sidecars missing texture start at neutral amounts and sizes. The earlier residual multiplier has no equivalent and is reset for Merrill when camera capabilities normalize the recipe. Quattro retains that stored field for its Smooth/Crisp behavior. Retired linked overrides are accepted only while reading older sidecars and are omitted on save. Unknown and duplicate fields remain errors. Loading alone must not rewrite sidecars.

The old linked-control UI, alternate method selector, residual-gain kernels, core-only detail projection and comparison generator are removed. Pattern preparation uses camera metadata and processing geometry independently of cleanup controls. The camera-derived base algorithms and their attribution remain.

## Validation

Checks cover fixed camera-base parameters, independent cleanup endpoints, signed identity, spatial band response, size bounds, exact combined-change mapping, neutral pyramid bypass, independent group toggles, selective inspection, and retained ownership. Native fixtures cover cached/fresh edits, regional seams and corners, all four orientations, Fast/Draft behavior, Full continuation, TIFF and DNG geometry. Sidecar checks cover old and experimental settings, strict schema handling, and save/reload. The filter optimization preserves the accepted portrait's sensor-base and final floating-point samples exactly. Cross-platform photographic/runtime checks remain unperformed.
