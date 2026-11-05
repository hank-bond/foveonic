---
summary: "A saved rectangular crop shares oriented framing across preview, navigation and exports."
kind: reference
status: accepted
implementation: conforming
---

# Cropping

The main toolbar must offer Hand, Crop and [Rotate](./rotation.md) in a shared button group centered between a vertical separator and the window's right edge. Align the separator with the right pane's divider, including while resizing. Crop must use one wide button with an integrated options arrow and separate action/menu click targets. When the main controls need more room, wrap before this region and retain its right alignment. Use the shared controls and spacing.

C must activate Crop and H must return to Hand, while respecting focused text fields and open menus. Crop must remain active until another tool is chosen. Show the complete photograph with the exterior dimmed and corner and edge handles. Dragging a handle resizes the crop. Dragging inside moves it. Dragging outside starts a new rectangle. Double-clicking the photograph must reset to the full uncropped image while keeping Crop active. Show a thirds grid while dragging and dimensions in pixels. Constrain all gestures to the photograph and preserve the selected aspect ratio. Space temporarily pans. Escape cancels an active drag, otherwise returns to Hand. Enter returns to Hand. Returning to Hand must fit the saved cropped view. Switching between Crop and Rotate must preserve their shared untrimmed editing plane, crop overlay, zoom and pan. Re-entering must restore its rectangle over the complete photograph.

Offer Unconstrained, Original and common aspect ratios, a crop-orientation switch, reset, and adding/removing named custom ratios. Custom ratios belong to application preferences. The selected constraint and rectangle belong to the photograph. Removing a custom preset must preserve that ratio in photographs already using it.

Save the rectangle in normalized coordinates relative to the camera's active, oriented image, with rotation providing its available interior frame. Missing crop data means the complete photograph. Cropping must compose with camera orientation, reduced previews and existing output resizing. Apply the crop before requested output resizing. Keep the original working pixels intact and reuse the existing GPU view and export samplers. All photographic formats and DNG must honor the same framing.

While editing the crop, only presentation expands to the complete photograph. This transient view must not enter saved edits, browser preview caches or exports. Scope and navigator geometry must match the displayed view. Smart must retain the previous image while preparing shared framing and use a complete frame for its editing plane. Regional detail outside framing must map into the correct crop coordinates. Other on-image tools must not stay active simultaneously.

Photographic section presets, camera defaults and Before comparisons must preserve composition. Whole-recipe copy/paste may include the crop. Crop reset is independent of tone and color adjustments. Straightening follows the shared rotation requirements. Print dimensions and perspective correction remain separate work.
