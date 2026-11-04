---
summary: "Advanced Color Range uses soft hue and saturation selections for saved HSL edits."
kind: reference
status: accepted
implementation: conforming
---

# Advanced color range prototype

Keep Basic's existing swatches and direct editing. Add an Advanced tab that always shows its wheel, including with an empty edit list, with a color picker, hue boundaries, inner and outer saturation boundaries, Smoothness and an enabled edit list. Each entry must offer Hue, Saturation and Lightness. Entries may be added and deleted. Limit the prototype to 30 entries.

Place the Basic/Advanced mode buttons in a shared group inline immediately to the right of the Color Range title, preserving reset, preset and drag actions. Use Advanced when its padded button fits alongside the header actions, shortening it to Adv. only when needed. Use the shared button-label fit helper and keep both choices inline as the pane resizes. Basic’s color swatches must form a centered shared button group. Center the Advanced wheel horizontally in the available pane, both with and without entries. With no selected entry, retain disabled Smoothness and HSL controls in their ordinary positions. Place the picking prompt in the selection area below the divider, reserving enough minimum height to keep the first entry from moving the controls or action buttons.

Each range may have a saved name of up to 64 Unicode characters. Double-click its name to edit it. Enter or clicking elsewhere commits, and Escape cancels. Unnamed ranges display their ordinal label. Names follow entries through deletion, section presets and saved edits, and must not alter GPU parameters or trigger another image render.

Select colors from the existing cleaned, white-balanced color guide, independent of authored color edits and Monochrome. The wheel's angle represents hue and radius represents relative saturation in the existing guide basis. Boundaries must wrap smoothly through red. Smoothness softens hue and saturation edges. An explicit near-neutral fade protects unstable hues. Picking must run through the existing asynchronous image worker and transfer only a small selection record from the GPU. Reject stale replies after a photo or recipe change.

Combine weighted HSL amounts from original guide coordinates, then apply the combined correction. Saturation displays -1 through +1 using the shared balanced mapping to the existing -1 through +2 stored amounts, and uses the shared [perceptual shoulder and clipping policy](./saturation.md#shared-clipping-protection). Hue and Lightness remain -1 to +1. Monochrome must remain achromatic and support selective tonal edits based on original subject colors. Preserve signed values and headroom for existing display and export handling.

Each range row must place a preview eye immediately after its enable checkbox. Eyes are closed by default. At most one range is previewed at a time. Clicking another eye switches the preview, and clicking the open eye clears it. Remove the separate View selected color range checkbox. The preview must desaturate unselected colors, remain transient and never enter saved recipes, presets, browser thumbnails or exports. Deleting the previewed entry clears preview, and deleting an earlier entry preserves the previewed entry’s identity. Selection changes and tool state must remain separate from photographic edits.

Active edits must share the ordinary Full, Fast, regional and export processing paths. Color DNG must bake the full RGB transform for Advanced HSL, including Lightness at fixed perceptual color components. Monochrome Lightness remains a scalar gain. Section bypass, reset, defaults and presets must include all Advanced entries. Missing Advanced data defaults to an empty list. No migration of earlier experimental settings is required.

Picking must use the shared [direct color gestures](./direct-color-editing.md), creating and selecting a range on a valid sample and adjusting its HSL values during the same drag. The right-aligned action row must read +, −, eyedropper. Use shared numerical widgets and icon-button sizing. Photographic arithmetic stays on the GPU. Evaluate the list in one processing pass, without a full-image mask or pass for every row. The [research](../advanced-color-range-research.md) records color-selection findings and the [plan](../../plans/archived/advanced-color-range.md) tracks implementation and photographic feedback.
