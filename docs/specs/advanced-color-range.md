---
summary: "Advanced Color Range uses soft hue and saturation selections for saved HSL edits and selective lightness inversion."
kind: reference
status: accepted
implementation: conforming
---

# Advanced color range prototype

Keep Basic's existing swatches and direct editing. Add an Advanced tab with a color picker, hue boundaries, inner and outer saturation boundaries, Smoothness and an enabled edit list. Each entry must offer Hue, Saturation, Lightness and Invert lightness. Entries may be added and deleted. Limit the prototype to 30 entries.

Select colors from the existing cleaned, white-balanced color guide, independent of authored color edits and Monochrome. The wheel's angle represents hue and radius represents relative saturation in the existing guide basis. Boundaries must wrap smoothly through red. Smoothness softens hue and saturation edges. An explicit near-neutral fade protects unstable hues. Picking must run through the existing asynchronous image worker and transfer only a small selection record from the GPU. Reject stale replies after a photo or recipe change.

Combine weighted HSL amounts from original guide coordinates, then apply the combined correction. Cap combined inversion coverage at one and invert once, so overlapping entries cannot cancel inversion. Apply the combined Lightness lift after inversion so its positive direction remains a lift. Selective inversion must retain perceptual color components and use the exposed Oklab lightness response of the global prototype. Monochrome must remain achromatic and support selective tonal edits based on original subject colors. Preserve signed values and headroom for existing display and export handling.

The selected-range preview must desaturate unselected colors, remain transient and never enter saved recipes, presets, browser thumbnails or exports. Selection changes and tool state must remain separate from photographic edits.

Active edits must share the ordinary Full, Fast, regional and export processing paths. DNG must bake full RGB where inversion can create nonzero output from black. Section bypass, reset, defaults and presets must include all Advanced entries. Missing Advanced data defaults to an empty list. No migration of earlier experimental settings is required.

Use shared numerical widgets and icon-button sizing. Photographic arithmetic stays on the GPU. Evaluate the list in one processing pass, without a full-image mask or pass for every row. The [research](../advanced-color-range-research.md) records color-selection findings and the [plan](../../plans/advanced-color-range.md) tracks implementation and photographic feedback.
