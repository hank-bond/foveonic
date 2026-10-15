---
summary: "Control help explains photographic effects in plain English, with reviewed A/B crops where they teach the adjustment clearly."
kind: reference
status: accepted
implementation: incomplete
---

# Photographic control help

## Requirements

- Editing controls, advanced controls and their link buttons, inspection choices, and scope choices **must** have explanations written for people familiar with photo editing, without assuming programming or Foveon sensor knowledge.
- Noise preset and Detail zero/default explanations **must** live in hover popovers, not as a persistent note below the controls. Use simple photographic language; professional photography knowledge **may** be assumed, but Merrill architecture expertise **must not** be assumed.
- Help **should** explain what changes, when it is useful, and the important tradeoff. It **must** describe the implemented behavior, including neutral/off settings and relevant dependencies. Technical names alone are not explanations.
- Highlights under Corrections **must** have help explaining Auto, the relative threshold offset, its neutralization/color tradeoff, and the distinction between manual 0 and the enable checkbox. It **must** remain distinct from tone Highlights and avoid promising recovery of lost capture detail.
- Advanced help **must** explain the visible effect and how to switch between following a basic control and a separate manual setting. Camera-dependent noise presets **must not** be presented as uniformly increasing strength or as off at zero.
- Example comparisons **should** use obvious, intuitive crops from real photographs. Each pair **must** show the same crop at the same scale, name the settings, and hold unrelated settings fixed. Companion settings needed for the effect **must** be stated.
- Minimum/maximum comparisons **should** be used when informative; representative values or off/on comparisons **may** replace uninformative extremes.
- Example crops **must** be reviewed before becoming application help assets. If existing sources do not demonstrate an effect clearly, request a suitable photograph from the owner rather than exaggerating or claiming a subtle difference is obvious.

## Popover presentation

- Popover body text, notes, and captions **must** use the app's normal 13 pt body style. They **must not** use tiny footnote text.
- Titles **must** use the app's normal heading style. The example photograph's name **must** appear to the right of the title in the same header row, using the normal 13 pt body size in italics. The viewport information bar remains the smallest text tier.
- The example name **must** come from the manifest's source filename with its extension removed, without a separate author-maintained display field.
- Popovers **must** inherit the app's shared button, spacing, background, and border styling. The image and caption **must** form one card with a small offset border. The caption **must** touch the image's bottom edge and use a slightly lighter gray background. Switching examples **must not** shift the card or image.

## Highlights example

Use `sunlit-red-flower.x3f` at Exposure −0.5 EV, with the same oriented crop `(1150,2094)–(2292,3123)` inclusive (1143×1030 pixels) in all three views:

- **Off:** flower color remains, but bright leaves show harsh yellow clipping.
- **Auto:** yellow leaf patches are removed, but leaves become too pale and petals show posterized, flat patches.
- **+0.8:** flower color is largely retained and leaf color looks more natural in this photograph.

This example teaches the correction/color tradeoff, not a universal recommended value. Keep other edits fixed and review the rendered crops before embedding them.

## Bad pixels and Speckles examples

These examples **must** distinguish what the two corrections can repair. Use Off/On variants with the companion correction enabled in both views. Keep Exposure 0 EV, Sunlight white balance, Standard color mode, Noise and Detail off, and sharpening off. Enlarge each 64×64-pixel crop to 400×400 and identify the enlargement in the note.

- **Bad pixels:** use `glossy-leaves-and-buds.x3f`, centered on the 2×2 magenta defect at `(1582,2737)`, with inclusive crop `(1550,2705)–(1613,2768)`. Speckles alone leaves this patch; Bad pixels repairs it.
- **Speckles:** use `turquoise-still-life.x3f`, centered on the isolated magenta dot at `(1278,1993)`, with inclusive crop `(1246,1961)–(1309,2024)`. Bad pixels alone leaves this dot; Speckles repairs it.

These magnified examples explain defect correction, not general grain reduction. Do not imply that either correction replaces the other.

## Row banding example

Use `paper-and-recycling-can.x3f` at Exposure +3 EV, with the inclusive oriented crop `(1940,3672)–(2339,4071)` at native 400×400 pixels. Show Row banding Off/On with Noise and Detail and sharpening off. Keep Bad pixels, Speckles, and highlight correction enabled in both views.

The note **must** identify the brightened shadow and explain that the stripes appear vertical because this photograph is rotated. Both views **must** use the same exposure without artificially amplifying the removed pattern. This is the Row banding correction, separate from the Banding noise slider.

## Comparison management

- Official source X3Fs **must** be hosted without editing sidecars in one long-lived supplemental [Example photographs release](https://github.com/hank-bond/foveonic/releases/tag/example-photographs). New files **may** be appended; published filenames and contents **must** remain stable. Source provenance **may** identify photographs by official filename.
- Downloads **must** use an ignored local directory and skip existing files. Importing **must not** overwrite local originals or modify local editing sidecars.
- Each comparison **must** have a stable ID and a manifest recording its copy, source filename, oriented crop, common rendering recipe, and at least two labeled variants with their setting differences and captions.
- Image generation **must** be a separate explicit step. Manifests and generated sRGB PNGs **must** be checked into the repository together after review. Changing rendering settings **must** be followed by explicit image regeneration; copy-only changes **must not** require rerendering.
- Normal application builds **must** check that every referenced PNG exists at its declared dimensions and embed the existing assets. Builds **must not** download sources, render photographs, or require Python, the original X3Fs, or GPU execution.
- One shared popover template **must** switch a fixed-size image and caption with labeled variant buttons. Hover help **must** remain accessible on disabled controls. Example selection **must not** change photographic settings, request image processing, or alter export.

## Implementation direction

Text-only controls keep their existing hover popovers. Reviewed comparisons use the shared manifest-driven template. Follow the [maintenance workflow](../help-examples.md) when adding examples. The [text/example review plan](../../plans/photographic-help.md) records source-image findings, and the [comparison plan](../../plans/help-comparisons.md) records tooling and integration checks. Processing behavior remains unchanged.
