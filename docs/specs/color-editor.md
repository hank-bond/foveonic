---
summary: "A shared linear Color Editor adjusts eight subject-color ranges with Hue, Saturation and Lightness controls."
kind: reference
status: accepted
implementation: conforming
---

# Color Editor

The owner approved replacing the Sensitivity presentation with **Color Editor**, following the reference implementation's basic swatch-and-three-slider layout. The processing operation is named `LinearColorEditor`. All camera families must share its numerical operation and independent cleaned color guide.

Offer Red, Orange, Yellow, Green, Cyan, Blue, Purple, Magenta and All Colors selections. Selection is transient UI state and must not modify an image or request a render. Each selection retains normalized -1 through +1 Hue, Saturation and Lightness values. Preserve the common three-significant-digit readout and stable panel width. Do not imply that advanced range editing, a picker or skin-tone tools are implemented.

Lightness retains the established stabilized subject-color brightness response in Color and Monochrome. Monochrome retains its authored sensor mixture; Hue and Saturation remain visible but disabled and their stored values remain intact. Old six-color sidecars migrate into the editor, with interpolated Orange and Purple values preserving their response. New sidecars serialize the Color Editor directly.

The Color Editor precedes HDR, Local Contrast, Universal sharpening and camera response. Its checkbox header independently bypasses the editor while retaining its values. Disabled controls remain visible. Place it immediately below Tone and Color in the default layout. It has its own frozen per-output section choice. Neutral controls must skip the guide and processing. Classify original cleaned subject colors so changing one range does not move pixels into another selection. Preserve signed samples and linear headroom.

Hue and Saturation use perceptual chroma rotation/scaling through Oklab, retaining its lightness coordinate. The controls are inspired by the reference workflow; numerical equivalence to its HSL implementation is not claimed. [Published Oklab matrices](https://bottosson.github.io/posts/oklab/) are public domain; attribute their use beside the shader.

DNG must support the active editor. Lightness-only edits retain shared scalar-gain baking. When chromatic edits are active in Color, map the edited selected-look pre-tone RGB through the inverse linear basis already used by the DNG profile. Its profile restores that mapping once and retains camera tone/exposure presentation. This route also bakes the selected pre-tone color reconstruction; it must not claim to retain an untouched canonical technical base. Existing profile-fitting and unsigned DNG storage limitations remain. Monochrome continues to bake only Lightness.

Validate neutral bypass, saved-edit migration, independent range selection, achromatic protection, chromatic changes, identical Lightness response between renderings, signed/headroom data, cached/fresh agreement and regional/reduced rendering. For each camera family, reproject baked DNG samples through the profile basis and compare with the same pre-tone photographic result. Downstream-editor photographic review remains separate from that numerical check.
