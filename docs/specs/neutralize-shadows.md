---
summary: "Neutralize shadows selectively reduces green and magenta casts in dark, nearly neutral regions."
kind: reference
status: accepted
implementation: conforming
---

# Neutralize shadows

The owner requested a separate experimental shadow-color correction after the speckle work. The control is named Neutralize shadows. It should move green and magenta shadow biases toward neutral without seeking maximum general noise reduction or predicting missing scene color.

Correction must be strongest in dark regions with low regional chroma. It must fade smoothly as brightness or genuine colorfulness increases and target the observed green and magenta directions. Useful brightness texture and grain should remain unchanged. Saturated real colors, skin, eyes and colored shadow detail require explicit regression checks. Regional color measurements should prevent fine noise from determining the hue gate independently at every pixel.

Trials must compare unchanged denoising against several gate widths and strengths. Both green and magenta casts, neutral backgrounds, broad casts and small blotches must be reviewed visually. The correction must remain independently adjustable with a neutral default until the owner accepts the photographic behavior. Sensor-specific cap maps are not required for this appearance control.

The experimental Merrill control must be in Noise for Color rendering. Strength 0 must be an exact bypass. Inactive settings must survive Monochrome switching, but the adjustment must not affect monochrome output. Noise-scoped recipes must copy the control and the Noise bypass must disable it. The GPU pass operates in linear working RGB before camera tone and display exposure, with a regional color guide and a brightness-preserving color change. Photographic and editable DNG processing must share the selected technical adjustment. Regional and reduced previews must retain native spatial support.

Strength ranges from 0 to 1, initially 0. Shadow limit ranges from -10 to -1 stops relative to nominal linear white, initially -4. It describes capture-domain brightness before display exposure. Color tolerance ranges from 0.05 to 1, initially 0.25. Hue width ranges from 10 to 75 degrees, initially 35. Lower color tolerance protects more genuine color. Wider hue settings affect more neighboring hues. The regional guide uses an eight-native-pixel radius, scaled for reduced previews.

The operation must attenuate chroma rather than add an opposite tint. It should approach neutral without reversing green into magenta or magenta into green. Real muted green or magenta shadow colors can still be affected and must remain part of photographic review.
