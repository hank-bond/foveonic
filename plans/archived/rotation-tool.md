---
summary: "Implement a shared saved rotation transform and Straighten/Freehand toolbar gestures."
status: archived
---

# Rotation tool

Implement the accepted [rotation requirements](../../docs/specs/rotation.md). The owner supplied interaction examples and approved automatic interior cropping without blank corners.

- [x] Map current crop, camera orientation, output sampling and tool routing.
- [x] Add saved relative quarter turns and fine-angle geometry with interior framing.
- [x] Integrate draft presentation, Straighten/Freehand gestures, menus and shortcuts.
- [x] Verify crop, preview, picker, export and DNG coordinate agreement and neutral-output preservation.
- [x] Run workspace/native checks, rebuild and reopen for owner review.

Use one output-to-storage affine for fine rotation across display, scope and export sampling. Keep the existing exact path for neutral and quarter-turn geometry. Draft presentation uses the complete existing texture, with metadata retained by the artifact. Rotation does not enter raw photographic processing.


## Findings and validation

Fine rotation shares a normalized output-to-storage affine across preview, scopes, pickers, photographic export and DNG. The native quarter-turn path stays exact. Export uses the existing photographic interpolation kernel. The first GPU comparison caught a redundant half-pixel offset in the new affine adapter, which is corrected. Tests retain immutable source pixels and compare preview, TIFF and DNG samples across orientations, crops and output sizes.

Quattro materializes camera framing into its buffer before shared edits. Authored composition still applies afterward. Native checks cover Merrill, Quattro and Classic. Their workflow draws Straighten and Freehand drafts, preserves the fine angle through a quarter turn, returns to Hand, reopens Rotate and switches to Crop. Separate preview-mode stages verify reduced texture geometry and Smart whole-frame refinement. Drafts do not change settings or queue photographic renders. Escape cancels only the draft before exiting the tool. Navigator preparation reuses the viewport presentation transform. Viewport scopes map the visible saved framing into the composed buffer, avoiding source-size regions outside the trimmed image.

Workspace tests, strict all-target Clippy, formatting, documentation indexes and artwork checks pass. GPU checks cover photographic preview/export agreement, DNG geometry, exact quarter-turn presentation, native crop preservation and the unchanged reference resize samples. The native app is rebuilt for owner review. Source sidecars and ordinary preferences stay outside the isolated smoke workflows.

The owner requested distinct mode icons and Shift+R cycling after reviewing the tool. All four choices now share one mode list and icon renderer. Cycling selects a mode without modifying the photograph. R and the main button invoke it, and menu selection invokes quarter-turn commands directly. Keyboard quarter turns join the ordinary edit dispatch so sidecars and render requests observe the change.
