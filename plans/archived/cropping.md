---
summary: "Add saved crop geometry, a shared viewport tool strip and direct crop gestures."
status: archived
---

# Add crop editing

Requirements: [cropping](../../docs/specs/cropping.md).

- [x] Add validated normalized crop settings and compose them with the existing image view before resizing.
- [x] Add the right-aligned Hand/Crop strip and aspect-ratio preferences.
- [x] Implement resize, move and new-rectangle gestures with handles, dimming and a drag grid.
- [x] Preserve cropped/full presentation, Smart detail, scopes, color picking and preview-cache ownership.
- [x] Verify orientation, persistence, exports and native interaction. Rebuild and reopen for owner review.

The existing renderer retains camera-active image geometry independently of stored GPU pixels. Quattro can bake camera framing and output sizing into a texture. Authored crops therefore need to defer requested output sizing until the terminal view/export sampler. Full-sensor diagnostics keep their existing geometry.

The UI sizing and range-name cleanup preceding this feature is committed as `313e520`.

The shared crop view passes exact RGB16 export comparisons against the full image at all four camera orientations, retains original GPU samples and applies output sizing last. DNG encoding checks confirm that both packed samples and range statistics use the cropped view. Sidecar roundtrips preserve the rectangle, aspect and orientation; editing defaults preserve the target photograph’s composition.

Native macOS checks cover Classic, Merrill and Quattro crop gestures, release-only saves, returning to Hand, reopening the saved rectangle, Fast and Smart transitions. A rotated Merrill check captures the Smart regional tile and confirms its full-image coordinates contain the visible cropped area. Shared toolbar tests cover right alignment on one and two rows. Workspace tests, strict Clippy, formatting, documentation and artwork checks pass. The rebuilt review app is reopened on the combined example folder for owner review.

Owner review refined the tool strip to a centered shared group aligned with the current right-pane divider. Crop now combines its action and ratio menu in one wide split button. Hand uses a rounded open-hand outline. The split control reuses workspace button and menu styling.

Double-clicking the photograph in Crop resets the rectangle and constraint to the complete original image, cancels the pending drag and leaves Crop active. Space retains ordinary viewport navigation.

The toolbar cleanup separates its ordered tool list and common Hand/Escape/availability routing into `photo_tools.rs`. Shared toolbar layout derives group width from that same list. Crop owns its button, ratio dialog and gestures, with one activation helper for menu, button and keyboard entry. Toolbar edits now return through the standard drawing result; the app loop no longer needs a separate crop-specific change check.
