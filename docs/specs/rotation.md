---
summary: "Saved Straighten, Freehand and relative quarter turns share crop geometry across display and exports."
kind: reference
status: accepted
implementation: conforming
---

# Rotation

The photograph toolbar must offer Rotate beside Crop with the same grouped split-button styling. Its menu must offer Straighten, Rotate Freehand, Rotate Left, Rotate Right and reset. Each choice must have a distinct icon in the menu and on the toolbar button. Freehand uses a plain circular arrow. Straighten adds crosshairs inside the arrow. Left and Right use mirrored circular arrows with a vertical stop bar beyond the arrowhead. Shift+R cycles Straighten, Freehand, Left and Right without changing the photograph. R invokes the selected choice. Left and Right remain selected after applying their relative quarter turn. Selecting Left or Right from the menu applies one turn. Hand and Enter finish editing. Escape cancels a drag before leaving the tool. Space temporarily pans. Other on-image tools must remain mutually exclusive.

Straighten must support both click-drag-release and click-release-move-click-release. After a first click without a drag, retain the starting point and follow the pointer until the second click is released. Keep the photograph still while drawing and apply alignment only on the final release. Escape cancels an unfinished line while keeping Straighten active. Lines below 45 degrees align to horizontal. Lines above 45 degrees align to vertical, so either a horizon or a building edge can guide the correction. Freehand must rotate around the photograph center and display a guide grid while dragging. Fine rotation is limited to -45 through +45 degrees. Left and Right must add a relative 90-degree turn while preserving the fine angle and selected content. Reset must restore the original orientation independently of photographic adjustments. Double-click resets the fine angle while retaining quarter turns.

The crop must remain inside the rotated photograph without blank corners. Automatically reduce its available area while keeping the chosen framing and aspect constraint. Show the exterior photograph dimmed while editing and the saved cropped result on returning to Hand. Report the proposed or saved fine angle in degrees using the shared upper-left [viewport badge](./gui-workspace.md) styling, in the same position as Before. Keep photographic dimensions in the existing pixel units.

Rotation belongs to the saved composition, alongside crop. Shared preview, photographic export, DNG, scope and picker sampling must use the same geometric mapping. Keep original processing buffers immutable and apply composition after photographic processing, before final resizing. Relative quarter turns must retain exact native sampling when no fine rotation or resizing is requested.

Crop and Rotate must share one untrimmed image plane, crop frame, exterior shading and photographic dimensions. Switching between them changes drag controls while preserving preview ownership, zoom and pan. It must not refit the image or request a replacement preview solely because the tool changed.

Dragging must update presentation using the existing texture without queuing a render for every pointer movement. Commit the draft on release through the ordinary render and sidecar path. Scope, navigator and browser preparation must preserve their artifact pairing. Smart may use whole-frame refinement when regional rotation geometry is unsupported.
