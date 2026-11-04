---
summary: "A shared linear Color Editor adjusts eight subject-color ranges with Hue, Saturation and Lightness controls."
kind: reference
status: accepted
implementation: conforming
---

# Color Editor

The owner approved replacing the Sensitivity presentation with **Color Editor**, using a basic swatch-and-three-slider layout. The processing operation is named `LinearColorEditor`. All camera families must share its numerical operation and independent cleaned color guide.

Offer Red, Orange, Yellow, Green, Cyan, Blue, Purple, Magenta and All Colors selections. Selection is transient UI state and must not modify an image or request a render. Each selection displays Hue, Saturation and Lightness on -1 through +1 controls. Saturation uses the shared balanced control mapping, while stored saturation amounts retain -1 through +2 for unchanged existing recipes. Preserve the common three-significant-digit readout and stable panel width. Advanced range editing is specified separately in [Advanced Color Range](./advanced-color-range.md), and direct picking in [Direct color editing](./direct-color-editing.md). Skin-tone tools remain outside this scope.

Lightness retains the established stabilized subject-color brightness response in Color and Monochrome. Monochrome retains its authored sensor mixture; Hue and Saturation remain visible but disabled and their stored values remain intact. Recipes serialize the current eight-range Color Editor directly. No six-color conversion or retired sensitivity alias is supported. The CLI Lightness amounts use the same eight displayed ranges.

The Color Editor precedes HDR, Local Contrast, Universal sharpening and camera response. Its checkbox header independently bypasses the editor while retaining its values. Disabled controls remain visible. Display it as Color Range, following Tone and Saturation in the default layout. It has its own frozen per-output section choice. Neutral controls must skip the guide and processing. Classify original cleaned subject colors so changing one range does not move pixels into another selection. Preserve signed samples and linear headroom.

Hue rotates Oklab chroma. Saturation uses the shared [perceptual shoulder and clipping policy](./saturation.md#shared-clipping-protection), retaining Oklab lightness. Direct editing uses the same bounds as the sliders. The controls follow the studied color-range workflow. Numerical equivalence to its HSL implementation is not claimed. [Published Oklab matrices](https://bottosson.github.io/posts/oklab/) are public domain. Attribute their use beside the shader.

DNG must support the active editor. Lightness-only edits retain shared scalar-gain baking. When chromatic edits are active in Color, map the edited selected-look pre-tone RGB through the inverse linear basis already used by the DNG profile. Its profile restores that mapping once and retains camera tone/exposure presentation. This route also bakes the selected pre-tone color reconstruction; it must not claim to retain an untouched canonical technical base. Existing profile-fitting and unsigned DNG storage limitations remain. Monochrome continues to bake only Lightness.

Validate neutral bypass, current-schema round trips, independent range selection, achromatic protection, chromatic changes, identical Lightness response between renderings, signed/headroom data, cached/fresh agreement and regional/reduced rendering. For each camera family, reproject baked DNG samples through the profile basis and compare with the same pre-tone photographic result. Downstream-editor photographic review remains separate from that numerical check.
