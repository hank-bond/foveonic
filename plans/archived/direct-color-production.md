---
summary: "Align direct color editing with shared controls, worker jobs and GPU selection contracts."
status: archived
---

# Direct color production cleanup

The owner requested shared button sizing and a review of the new interaction against the existing architecture and UI. Preserve simultaneous dragging and all accepted [direct color requirements](../../docs/specs/direct-color-editing.md).

- [x] Use shared button sizing, padding, styling and icon bounds for the picker and its settings.
- [x] Review settings layout and transient interaction ownership without introducing a second editor or processing path.
- [x] Align color picking with the existing worker reply and cancellation mechanisms.
- [x] Give GPU picking explicit parameters while sharing the photographic color classifier.
- [x] Verify button geometry across font sizes/themes, gesture behavior, GPU results and the native workflow.

The shared icon controls use the text button height, padding and visual states. Browser icon menus use the same primitive. Glyph bounds scale with the control instead of imposing a fixed button size. The configuration popover measures its dropdown labels and retains the shared numeric formatter.

Picking uses the existing worker update channel and render session. Pending photo changes, recipe changes, cancellation and shutdown invalidate work. As Shot resolves through the ordinary camera settings adapter. The native guide keeps selection consistent across preview qualities. GPU picking has its own uniform structure and shares range classification with photographic processing. Only eight weights return to the CPU.

Regression checks cover both themes at three font sizes, simultaneous axes, modifiers, stale replies, crop and all four orientations. Six Color Editor GPU checks and the original-file worker check passed. The native smoke workflow exercised activation, diagonal editing, affected swatches, settings and Escape without writing a source sidecar. Workspace tests, Clippy, formatting, documentation checks and artwork checks passed. The application bundle was rebuilt.
