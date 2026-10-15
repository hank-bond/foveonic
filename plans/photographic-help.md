---
summary: "Replace technical hover copy with photographic explanations, then review a small set of obvious A/B examples before expanding coverage."
---

# Improve photographic help

Requirements: [photographic control help](../docs/specs/photographic-help.md), [GUI workspace](../docs/specs/gui-workspace.md), [detail and noise](../docs/specs/detail-and-noise.md), and [pipeline inspection](../docs/specs/pipeline-inspection.md).

## Steps

- [x] Add plain-English help to every editing slider, preset, checkbox, and advanced link/parameter.
- [x] Explain inspection stages, component views, and all three scopes without assuming sensor knowledge.
- [x] Check hover coverage, disabled linked-control help, existing input behavior, and documentation indexes.
- [x] Assess the six existing originals and prepare a small set of labeled, reproducible A/B crops for owner review.
- [x] Try the owner's suggested adjacent sunlit/shadowed concrete in the stroller photograph before requesting a new noisy-shadow source.
- [ ] Obtain a cleaner sharpening-edge source if the existing photographs do not demonstrate it well.
- [ ] Review copy and examples with the owner before embedding examples or committing.

## Approach

Use existing hover popovers, available on the label, slider, and editable value. Keep settings, slider ranges, processing, and scheduling unchanged. Explain the ISO analogy accurately: Sensor gain scales the existing signal and noise early; Exposure scales the processed color image later. Neither changes the capture.

Noise presets and advanced coefficients have coupled, camera-dependent effects. Avoid unsupported promises that every larger value means more cleanup. Manual camera repair selects both a pixel-defect threshold and a separate group of larger defects; it is not a universal strength slider.

Existing unstaged processor, viewport, smoke, and workspace-documentation work predates this pass and must be preserved. Example candidates stay local and do not become default app assets without review.

## Text coverage and checks

`crates/foveonic-gui/src/help.rs` owns editing, stage, and scope explanations. Advanced parameter explanations retain their existing home in `noise::Parameter::help`; their arithmetic is unchanged. Labels, sliders, and value boxes share help. Read-only linked advanced rows explain both their effect and how to edit separately. Existing settings, input behavior, and render scheduling are unchanged by this help pass.

The concurrent processor work introduced Highlight restoration and two new inspection images while this pass ran. Its explanations were included and simplified as well. The inspection coverage test caught the new outputs before the descriptions were added.

Workspace tests pass, including 36 GUI tests with one GPU fixture skipped. New checks exercise actual hover popovers on all three parts of the Sensor gain row and every disabled linked advanced row, check visible editing-control coverage, and require help for every available inspection image. GUI all-target Clippy with warnings denied, changed-file formatting, documentation generation/read-only checks, and diff checks pass. Native photographic/interaction review remains pending; GPU algorithm checks are outside this text pass.

## Example candidates

Six labeled A/B cards, individual crops, exports, and exact render commands are in `.local/photographic-help/`. The local `build-examples.py` uses the existing reference virtual environment without installing dependencies, converts exported embedded profiles to sRGB, and records crop coordinates, display scales, and renderer SHA-256 in `manifest.json`. It refuses to mix renderer builds in the same directory. These candidates used the renderer before the concurrent highlight-restoration integration; regenerate with the reviewed processing recipe before embedding any assets.

| Card | Assessment before owner review |
| --- | --- |
| `saturation.png` | Obvious grayscale versus stronger turquoise color; promising. |
| `detail.png` | Leaf veins and bud texture visibly return between Detail 0 and 2, with more grain; promising. Noise presets remain 2, sharpening off. |
| `camera-repair.png` | Known magenta leaf defect disappears; promising at explicitly labeled 6× nearest-pixel magnification. Despeckle stays off. |
| `despeckle.png` | Known isolated magenta bolt dot disappears; promising at explicitly labeled 8× magnification. Camera repair stays on. |
| `sharpen.png` | Obvious grain amplification, but the reflective sign pattern and existing noise distract from edge sharpening/halos. Hold for a cleaner source. |
| `gain-vs-exposure.png` | Equal one-stop brightening produces nearly identical-looking leaf crops. Hold: this does not teach their processing-order difference clearly. Do not exaggerate the difference. |

A clean, in-focus high-contrast edge with a plain background may still be useful for sharpening amount/radius and halos. A noisy or underexposed example can explain gain's brightness/noise tradeoff, but is not guaranteed to visibly distinguish Sensor gain from an equivalent Exposure adjustment. Existing flower/rock images may suit later tone examples; the remaining sliders and advanced coefficients have not yet received dedicated example pairs.

## Managed comparisons

The owner selected the red flower's crop and Off/Auto/+0.8 states at −0.5 EV as the first integrated example, followed by complementary Bad pixels and Speckles examples verified with all four correction combinations. Their manifests, generated PNGs, named source photographs, and shared popover integration follow the [comparison plan](./help-comparisons.md) and [maintenance workflow](../docs/help-examples.md). Official source photographs now live in the long-lived supplemental release; the owner chose not to distribute editing sidecars. The earlier local cards remain historical candidates rather than the current app assets. Native review of the integrated popovers remains pending.

## Stroller light/shadow boundary

The owner pointed out that the stroller photograph already has the needed scene: adjacent sunlit and shadowed areas of the same concrete. Keeping the surface in one crop makes differences in grain easier to understand without a change of subject. This is preferable to requesting another noisy-shadow file prematurely.

The follow-up candidates are in `.local/photographic-help/stroller-boundary/`, with a context image, local reproduction script, frozen renderer, settings/crop manifest, and render log. The 512×512 crop is `[900,2592,1412,3104]` in oriented export coordinates, with right/bottom exclusive, displayed at native pixel scale. The frozen renderer includes the concurrent highlight-restoration integration; all comparisons keep repair and highlight restoration enabled, ColorDQ and sharpening off, and camera/color-mode choices fixed.

- `noise-reduction.png`: off versus on with noise presets 2 and Detail 1. Colored grain disappears clearly in the shadow while the same surface remains visible in sunlight. Promising for the reconstruction toggle; no new source is needed for this general example.
- `detail.png`: Detail 0 versus 2 with both noise presets 2. The return of surface variation and grain is obvious across both lighting areas. Promising for explaining the smoothing/restoration tradeoff; this surface is not sharply focused, so keep the leaf example for recognizable fine detail.
- `chroma-noise.png`: preset 0 versus 4 with Luma 2 and Detail 1. Shows a smoothing/texture change, but does not isolate an obvious colored-speckle difference. Hold as a standalone color-noise explanation.
- `luma-noise.png`: preset 0 versus 4 with Chroma 2 and Detail 1. Difference remains relatively subtle. Hold rather than claim an intuitive min/max demonstration.
- `gain-vs-exposure.png`: gain 2/Exposure 0 versus gain 1/Exposure +1 with identical reconstruction controls. The result still looks very similar. Hold; the adjacent lighting does not by itself make the early/late adjustment distinction obvious.

All ten follow-up exports succeeded. These remain review candidates, not installed or default application assets.
